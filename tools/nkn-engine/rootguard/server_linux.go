// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"encoding/binary"
	"io"
	"net"
	"os"
	"path/filepath"
	"sync"
	"syscall"
	"time"
)

// This private binary protocol is exclusive to kernel-admitted Core. It is
// deliberately distinct from the public status/stop stdio and browser JSON.
// Request82 = RSCN v1 / op / generation4 / expiryBoot8 / operationNonce32 /
// fixedPolicySHA32. Root replies RSHS v1 / generation4 / expiryBoot8 / nonce32 /
// policySHA32 / exactChildPID4 / RootState1 / EngineState1 (87 bytes).
// RootState: 1 active,2 released,3 unknown. Engine:1 starting,2 ready,3 reaped,4 unknown.
const requestBytes = 82
const responseBytes = 87
const (
	prepare    = 1
	launch     = 2
	status     = 3
	stopEngine = 4
	release    = 5
)

type request struct {
	op            byte
	generation    uint32
	deadline      uint64
	nonce, policy [32]byte
}

func parseRequest(raw []byte) (request, error) {
	if len(raw) != requestBytes || !bytes.Equal(raw[:5], []byte("RSCN\x01")) || raw[5] < prepare || raw[5] > release {
		return request{}, ErrDenied
	}
	r := request{op: raw[5], generation: binary.BigEndian.Uint32(raw[6:10]), deadline: binary.BigEndian.Uint64(raw[10:18])}
	copy(r.nonce[:], raw[18:50])
	copy(r.policy[:], raw[50:82])
	if r.generation == 0 && r.op != prepare && r.op != status && r.op != release || r.deadline == 0 || r.nonce == [32]byte{} || r.policy == [32]byte{} {
		return request{}, ErrDenied
	}
	return r, nil
}

type registration struct {
	request                request
	resources              *resources
	engine                 *ownedEngine
	owner                  *net.UnixConn
	start, cleanupDeadline time.Time
	lastLaunch             time.Time
	attempts               uint8
	unknown                bool
	released               bool
}

func (s *registration) matches(r request) bool {
	return s != nil && (s.request.generation == r.generation || r.generation == 0 && (r.op == prepare || r.op == status || r.op == release)) && s.request.deadline == r.deadline && s.request.nonce == r.nonce && s.request.policy == r.policy
}
func (s *registration) snapshot() []byte {
	b := make([]byte, responseBytes)
	copy(b, []byte("RSHS\x01"))
	binary.BigEndian.PutUint32(b[5:9], s.request.generation)
	binary.BigEndian.PutUint64(b[9:17], s.request.deadline)
	copy(b[17:49], s.request.nonce[:])
	copy(b[49:81], s.request.policy[:])
	ready, reaped, pid := s.engine.state()
	kernelInvalid := ready && !s.engine.kernelLive()
	binary.BigEndian.PutUint32(b[81:85], pid)
	b[85] = 1
	if s.released {
		b[85] = 2
	}
	if s.unknown || kernelInvalid {
		b[85] = 3
	}
	b[86] = 1
	if ready && !kernelInvalid {
		b[86] = 2
	}
	if reaped {
		b[86] = 3
	}
	if (s.unknown || kernelInvalid) && !reaped {
		b[86] = 4
	}
	return b
}
func (s *registration) cleanup(ctx context.Context) error {
	if s == nil || s.released {
		return nil
	}
	if s.cleanupDeadline.IsZero() {
		s.cleanupDeadline = time.Now().Add(5 * time.Second)
	}
	ctx, cancel := context.WithDeadline(ctx, s.cleanupDeadline)
	defer cancel()
	if s.engine.stop(ctx) != nil {
		s.unknown = true
		return ErrDenied
	}
	if s.resources == nil || s.resources.release(ctx) != nil {
		s.unknown = true
		return ErrDenied
	}
	s.released = true
	s.unknown = false
	return nil
}

type manager struct {
	policy *Policy
	mu     sync.Mutex
	scope  *registration
}

func (m *manager) handle(r request, connection *net.UnixConn) ([]byte, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if r.policy != m.policy.hash {
		return nil, ErrDenied
	}
	if r.op == prepare {
		if m.scope != nil {
			if m.scope.matches(r) {
				return m.scope.snapshot(), nil
			} // Query original operation, never start twice.
			if !m.scope.released {
				return nil, ErrDenied
			}
		}
		now, e := bootMillis()
		if e != nil || r.deadline <= now || r.deadline-now > 300000 {
			return nil, ErrDenied
		}
		generation, e := nextGeneration()
		if e != nil {
			return nil, e
		}
		if r.generation != 0 && r.generation != generation {
			return nil, ErrDenied
		}
		r.generation = generation
		scope := &registration{request: r, owner: connection, start: time.Now()}
		m.scope = scope
		ctx, cancel := context.WithDeadline(context.Background(), scope.start.Add(10*time.Second))
		defer cancel()
		resource, e := prepareResources(ctx, m.policy)
		scope.resources = resource
		if e != nil {
			scope.unknown = true
			cleanup, c := context.WithTimeout(context.Background(), 5*time.Second)
			defer c()
			_ = scope.cleanup(cleanup)
		}
		return scope.snapshot(), nil
	}
	if !m.scope.matches(r) {
		return nil, ErrDenied
	}
	s := m.scope
	if s.released {
		return s.snapshot(), nil
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if r.op == release {
		_ = s.cleanup(ctx)
		return s.snapshot(), nil
	}
	if r.op == stopEngine {
		if s.engine.stop(ctx) != nil {
			s.unknown = true
		}
		return s.snapshot(), nil
	}
	if r.op == launch {
		_, reaped, _ := s.engine.state()
		if s.owner != connection || s.unknown || s.engine != nil && !reaped ||
			s.attempts >= 3 || s.attempts == 0 && !time.Now().Before(s.start.Add(10*time.Second)) ||
			s.attempts > 0 && (time.Since(s.lastLaunch) < 5*time.Second || time.Since(s.start) >= 30*time.Second) {
			return nil, ErrDenied
		}
		s.attempts++
		s.lastLaunch = time.Now()
		start, c := context.WithDeadline(context.Background(), s.lastLaunch.Add(10*time.Second))
		defer c()
		engine, e := launchEngine(start, s.resources, r.generation, r.nonce, r.deadline)
		s.engine = engine
		if e != nil {
			s.unknown = true
			_ = s.cleanup(ctx)
		}
	}
	return s.snapshot(), nil
}
func (m *manager) fence() {
	m.mu.Lock()
	defer m.mu.Unlock()
	s := m.scope
	if s == nil || s.released {
		return
	}
	now, e := bootMillis()
	ready, reaped, _ := s.engine.state()
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	invalid := e != nil || now >= s.request.deadline || s.unknown || s.resources.check(ctx) != nil || ready && !s.engine.kernelLive() || !ready && !reaped && time.Since(s.lastLaunch) >= 10*time.Second
	if invalid {
		_ = s.cleanup(ctx)
	}
}
func peerIsCore(c *net.UnixConn, uid uint32) bool {
	fd, e := c.SyscallConn()
	if e != nil {
		return false
	}
	valid := false
	e = fd.Control(func(raw uintptr) {
		peer, err := syscall.GetsockoptUcred(int(raw), syscall.SOL_SOCKET, syscall.SO_PEERCRED)
		valid = err == nil && peer.Pid > 1 && peer.Uid == uid
	})
	return e == nil && valid
}
func serveConnection(c *net.UnixConn, m *manager) {
	defer c.Close()
	if !peerIsCore(c, m.policy.roles.core) {
		return
	}
	defer func() {
		m.mu.Lock()
		defer m.mu.Unlock()
		if m.scope != nil && m.scope.owner == c && !m.scope.released {
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			_ = m.scope.cleanup(ctx)
		}
	}()
	// Core-only observation traffic has its own bounded lifetime budget; it
	// does not consume the separate 128-record encrypted business quota.
	for index := 0; index < 4096; index++ {
		deadline := time.Now().Add(2 * time.Second)
		m.mu.Lock()
		if m.scope != nil && m.scope.owner == c && !m.scope.released {
			now, e := bootMillis()
			if e == nil && now < m.scope.request.deadline {
				deadline = time.Now().Add(time.Duration(m.scope.request.deadline-now) * time.Millisecond)
			}
		}
		m.mu.Unlock()
		_ = c.SetReadDeadline(deadline)
		var raw [requestBytes]byte
		if _, e := io.ReadFull(c, raw[:]); e != nil {
			return
		}
		r, e := parseRequest(raw[:])
		if e != nil {
			return
		}
		reply, e := m.handle(r, c)
		if e != nil {
			return
		}
		_ = c.SetWriteDeadline(time.Now().Add(time.Second))
		if n, e := c.Write(reply); e != nil || n != len(reply) {
			return
		}
		m.mu.Lock()
		owner := m.scope != nil && m.scope.owner == c && !m.scope.released
		m.mu.Unlock()
		if !owner {
			return
		} // Recovery connections query/clean only the exact nonce.
	}
}

// Serve is explicit candidate activation by an already installed Root unit.
// It creates no account, policy, seed, nft tool or systemd activation path.
// Unknown leftover sockets/locks/resources are preserved and rejected.
func Serve(ctx context.Context, p *Policy) error {
	if p == nil || os.Getuid() != 0 || os.Geteuid() != 0 || rootDirectory(filepath.Dir(SocketPath)) != nil {
		return ErrDenied
	}
	// PR_SET_NO_NEW_PRIVS affects one OS thread. Setting it on a Go goroutine
	// is insufficient: exec may run on another thread. Require NNP inherited
	// at service exec, and verify every existing thread before serving.
	tasks, e := os.ReadDir("/proc/self/task")
	if e != nil || len(tasks) == 0 || len(tasks) > 64 {
		return ErrDenied
	}
	for _, task := range tasks {
		raw, e := os.ReadFile("/proc/self/task/" + task.Name() + "/status")
		if e != nil || len(raw) > 8192 || !bytes.Contains(raw, []byte("\nNoNewPrivs:\t1\n")) {
			return ErrDenied
		}
	}
	lockFD, e := syscall.Open(filepath.Dir(SocketPath)+"/root.lock", syscall.O_RDWR|syscall.O_CREAT|syscall.O_NOFOLLOW|syscall.O_CLOEXEC, 0600)
	if e != nil {
		return ErrDenied
	}
	lock := os.NewFile(uintptr(lockFD), "owned-root-manager-lock")
	defer lock.Close()
	if syscall.Flock(lockFD, syscall.LOCK_EX|syscall.LOCK_NB) != nil {
		return ErrDenied
	}
	defer syscall.Flock(lockFD, syscall.LOCK_UN)
	var st syscall.Stat_t
	if syscall.Fstat(lockFD, &st) != nil || st.Uid != 0 || st.Mode&0777 != 0600 || st.Nlink != 1 {
		return ErrDenied
	}
	if _, e := os.Lstat(SocketPath); !os.IsNotExist(e) {
		return ErrDenied
	}
	listener, e := net.ListenUnix("unix", &net.UnixAddr{Name: SocketPath, Net: "unix"})
	if e != nil {
		return ErrDenied
	}
	listener.SetUnlinkOnClose(false)
	defer listener.Close()
	if os.Chown(SocketPath, 0, int(p.roles.coreGID)) != nil || os.Chmod(SocketPath, 0660) != nil {
		return ErrDenied
	}
	m := &manager{policy: p}
	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-ctx.Done():
			listener.Close()
		case <-done:
		}
	}()
	tick := time.NewTicker(250 * time.Millisecond)
	defer tick.Stop()
	fenceDone := make(chan struct{})
	defer close(fenceDone)
	go func() {
		for {
			select {
			case <-tick.C:
				m.fence()
			case <-fenceDone:
				return
			}
		}
	}()
	defer func() {
		m.mu.Lock()
		defer m.mu.Unlock()
		if m.scope != nil {
			cleanup, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			_ = m.scope.cleanup(cleanup)
		}
	}()
	// One lifetime owner connection plus at most one recovery/query connection.
	// EOF of the exact original Core descriptor immediately starts cleanup.
	slots := make(chan struct{}, 2)
	var conns sync.Map
	defer func() { conns.Range(func(key, value any) bool { key.(*net.UnixConn).Close(); return true }) }()
	for {
		c, e := listener.AcceptUnix()
		if e != nil {
			return ErrDenied
		}
		select {
		case slots <- struct{}{}:
			conns.Store(c, true)
			go func() { defer func() { conns.Delete(c); <-slots }(); serveConnection(c, m) }()
		default:
			c.Close()
		}
	}
}
