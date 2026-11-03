// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"io"
	"net"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

type ownedEngine struct {
	statusMu                  sync.Mutex
	statusEpoch, statusFrames uint32
	statusPoisoned            bool
	lastStatus                time.Time

	cmd            *exec.Cmd
	control        *net.UnixConn
	input, output  *os.File
	done           chan struct{}
	mu             sync.Mutex
	ready, reaped  bool
	pid            uint32
	uid            uint32
	startTicks     string
	exeDev, exeIno uint64
}

func processStart(pid uint32) (string, error) {
	raw, e := readKernel("/proc/" + strconv.FormatUint(uint64(pid), 10) + "/stat")
	if e != nil {
		return "", ErrDenied
	}
	end := strings.LastIndex(raw, ") ")
	if end < 0 {
		return "", ErrDenied
	}
	fields := strings.Fields(raw[end+2:])
	if len(fields) < 20 {
		return "", ErrDenied
	}
	if _, e := strconv.ParseUint(fields[19], 10, 64); e != nil {
		return "", ErrDenied
	}
	return fields[19], nil
}
func (e *ownedEngine) kernelLive() bool {
	if e == nil {
		return false
	}
	base := "/proc/" + strconv.FormatUint(uint64(e.pid), 10)
	ticks, err := processStart(e.pid)
	if err != nil || ticks != e.startTicks {
		return false
	}
	var image syscall.Stat_t
	if syscall.Stat(base+"/exe", &image) != nil || image.Dev != e.exeDev || image.Ino != e.exeIno {
		return false
	}
	group, err := readKernel(base + "/cgroup")
	if err != nil || group != "0::/system.slice/rustsimhub-nkn-root.service/engine" {
		return false
	}
	f, err := os.Open(base + "/status")
	if err != nil {
		return false
	}
	raw, err := io.ReadAll(io.LimitReader(f, 8193))
	f.Close()
	if err != nil || len(raw) > 8192 {
		return false
	}
	if !bytes.Contains(raw, []byte("\nNoNewPrivs:\t1\n")) {
		return false
	}
	for _, cap := range []string{"CapInh", "CapPrm", "CapEff", "CapAmb"} {
		if !bytes.Contains(raw, []byte("\n"+cap+":\t0000000000000000\n")) {
			return false
		}
	}
	for _, line := range strings.Split(string(raw), "\n") {
		if strings.HasPrefix(line, "Uid:") {
			uids := strings.Fields(strings.TrimPrefix(line, "Uid:"))
			if len(uids) != 4 {
				return false
			}
			for _, uid := range uids {
				if uid != strconv.FormatUint(uint64(e.uid), 10) {
					return false
				}
			}
			return true
		}
	}
	return false
}

func bootMillis() (uint64, error) {
	f, e := os.Open("/proc/uptime")
	if e != nil {
		return 0, ErrDenied
	}
	defer f.Close()
	b, e := io.ReadAll(io.LimitReader(f, 129))
	if e != nil || len(b) > 128 {
		return 0, ErrDenied
	}
	var seconds, centi uint64
	// The kernel's fixed centisecond representation is parsed without floats.
	first := bytes.Fields(b)
	if len(first) != 2 {
		return 0, ErrDenied
	}
	parts := bytes.Split(first[0], []byte("."))
	if len(parts) != 2 || len(parts[1]) != 2 {
		return 0, ErrDenied
	}
	seconds, e = strconv.ParseUint(string(parts[0]), 10, 54)
	if e != nil {
		return 0, ErrDenied
	}
	centi, e = strconv.ParseUint(string(parts[1]), 10, 8)
	if e != nil || centi >= 100 {
		return 0, ErrDenied
	}
	return seconds*1000 + centi*10 + 10, nil
}
func openExecutable(p *Policy) (*os.File, error) {
	if rootDirectory("/opt/rustsimhub/selected") != nil {
		return nil, ErrDenied
	}
	fd, e := syscall.Open(EnginePath, syscall.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_CLOEXEC, 0)
	if e != nil {
		return nil, ErrDenied
	}
	f := os.NewFile(uintptr(fd), "selected-pinned-executable")
	failed := true
	defer func() {
		if failed {
			f.Close()
		}
	}()
	var stat syscall.Stat_t
	if syscall.Fstat(fd, &stat) != nil || stat.Uid != 0 || stat.Mode&syscall.S_IFMT != syscall.S_IFREG || stat.Mode&0022 != 0 || stat.Mode&0111 == 0 || stat.Nlink != 1 || stat.Size > 32*1024*1024 {
		return nil, ErrDenied
	}
	hash := sha256.New()
	if n, e := io.Copy(hash, io.LimitReader(f, 32*1024*1024+1)); e != nil || n > 32*1024*1024 || !bytes.Equal(hash.Sum(nil), p.executable[:]) {
		return nil, ErrDenied
	}
	if _, e := f.Seek(0, 0); e != nil {
		return nil, ErrDenied
	}
	failed = false
	return f, nil
}
func launchEngine(ctx context.Context, r *resources, generation uint32, nonce [32]byte, deadline uint64) (result *ownedEngine, resultError error) {
	if generation == 0 || nonce == [32]byte{} || r.check(ctx) != nil {
		return nil, ErrDenied
	}
	now, e := bootMillis()
	if e != nil || deadline <= now || deadline-now > 300000 {
		return nil, ErrDenied
	}
	executable, e := openExecutable(r.policy)
	if e != nil {
		return nil, e
	}
	defer executable.Close()
	seed, e := rootFile(seedPath, 32, true)
	if e != nil || len(seed) != 32 {
		return nil, ErrDenied
	}
	defer clear(seed)
	private := ed25519.NewKeyFromSeed(seed)
	public := private.Public().(ed25519.PublicKey)
	if hex.EncodeToString(public) == r.policy.config.CorePin || hex.EncodeToString(public) == r.policy.config.StatisticsKey {
		clear(private)
		return nil, ErrDenied
	}
	clear(private)
	seedJSON := make([]byte, 66)
	seedJSON[0] = '"'
	seedJSON[65] = '"'
	hex.Encode(seedJSON[1:65], seed)
	defer clear(seedJSON)
	credential := struct {
		Format     uint8           `json:"format"`
		Generation uint32          `json:"generation"`
		Core       uint32          `json:"core_uid"`
		Engine     uint32          `json:"engine_uid"`
		Seed       json.RawMessage `json:"seed"`
		CorePin    string          `json:"core_pin"`
		Statistics string          `json:"statistics_public_key"`
		Executable string          `json:"engine_sha256"`
		Policy     string          `json:"policy_sha256"`
		Seeds      []string        `json:"seed_rpc"`
		DNS        []string        `json:"dns_servers"`
		Mode       string          `json:"mode"`
	}{
		1, generation, r.policy.roles.core, r.policy.roles.engine, seedJSON, r.policy.config.CorePin, r.policy.config.StatisticsKey, r.policy.config.Executable, hex.EncodeToString(r.policy.hash[:]), r.policy.config.Seeds, nil, "public"}
	for _, dns := range r.route.dns {
		credential.DNS = append(credential.DNS, dns.String())
	}
	raw, e := json.Marshal(credential)
	credential.Seed = nil
	if e != nil || len(raw) > 4096 {
		return nil, ErrDenied
	}
	defer clear(raw)
	read, write, e := os.Pipe()
	if e != nil {
		return nil, ErrDenied
	}
	defer read.Close()
	defer write.Close()
	fds, e := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM|syscall.SOCK_CLOEXEC, 0)
	if e != nil {
		return nil, ErrDenied
	}
	rootFile := os.NewFile(uintptr(fds[0]), "owned-root-control")
	defer rootFile.Close()
	childFile := os.NewFile(uintptr(fds[1]), "owned-child-control")
	defer childFile.Close()
	conn, e := net.FileConn(rootFile)
	if e != nil {
		return nil, ErrDenied
	}
	control, ok := conn.(*net.UnixConn)
	if !ok {
		conn.Close()
		return nil, ErrDenied
	}
	owned := &ownedEngine{control: control, done: make(chan struct{})}
	if r.policy.config.StatusProtocol == 1 {
		owned.statusEpoch, e = nextGeneration()
		if e != nil {
			control.Close()
			return nil, e
		}
	}
	failed := true
	defer func() {
		if failed {
			owned.stop(context.Background())
			if owned.cmd != nil {
				result = owned
			}
		}
	}()
	// FD 5 pins the exact checked inode through exec; selected-app replacement
	// cannot substitute a different program between hash checking and exec.
	cmd := exec.Command("/proc/self/fd/5", "--root-stdio")
	cmd.Args[0] = EnginePath
	cmd.Env = []string{"LC_ALL=C", "GOMAXPROCS=2", "GOMEMLIMIT=96MiB"}
	cmd.ExtraFiles = []*os.File{read, childFile, executable}
	cmd.Stderr = io.Discard
	cmd.SysProcAttr = &syscall.SysProcAttr{Credential: &syscall.Credential{Uid: r.policy.roles.engine, Gid: r.policy.roles.engineGID, Groups: []uint32{}}, Pdeathsig: syscall.SIGKILL}
	stdin, e := cmd.StdinPipe()
	if e != nil {
		return nil, ErrDenied
	}
	stdout, e := cmd.StdoutPipe()
	if e != nil {
		stdin.Close()
		return nil, ErrDenied
	}
	var valid bool
	owned.input, valid = stdin.(*os.File)
	if !valid {
		stdin.Close()
		stdout.Close()
		return nil, ErrDenied
	}
	owned.output, valid = stdout.(*os.File)
	if !valid {
		stdin.Close()
		stdout.Close()
		return nil, ErrDenied
	}
	// The Root service sets NNP before accepting requests. It is inherited by
	// this child before dropping every real/saved UID, group and capability.
	if cmd.Start() != nil {
		return nil, ErrDenied
	}
	owned.cmd = cmd
	owned.pid = uint32(cmd.Process.Pid)
	go func() {
		_ = cmd.Wait()
		owned.mu.Lock()
		owned.reaped = true
		owned.ready = false
		owned.mu.Unlock()
		close(owned.done)
	}()
	owned.uid = r.policy.roles.engine
	var image syscall.Stat_t
	if syscall.Fstat(int(executable.Fd()), &image) != nil {
		return nil, ErrDenied
	}
	owned.exeDev, owned.exeIno = image.Dev, image.Ino
	owned.startTicks, e = processStart(owned.pid)
	if e != nil {
		return nil, ErrDenied
	}
	if writeKernel(groupPath+"/cgroup.procs", strconv.Itoa(cmd.Process.Pid)) != nil {
		return nil, ErrDenied
	}
	group, e := readKernel("/proc/" + strconv.Itoa(cmd.Process.Pid) + "/cgroup")
	if e != nil || group != "0::/system.slice/rustsimhub-nkn-root.service/engine" {
		return nil, ErrDenied
	}
	if r.check(ctx) != nil {
		return nil, ErrDenied
	}
	var root [81]byte
	copy(root[:5], []byte("RSHL\x01"))
	binary.BigEndian.PutUint32(root[5:9], generation)
	binary.BigEndian.PutUint64(root[9:17], deadline)
	copy(root[17:49], nonce[:])
	hash := sha256.Sum256(raw)
	copy(root[49:81], hash[:])
	_ = control.SetWriteDeadline(time.Now().Add(time.Second))
	if n, e := control.Write(root[:]); e != nil || n != len(root) {
		return nil, ErrDenied
	}
	if write.SetWriteDeadline(time.Now().Add(time.Second)) != nil {
		return nil, ErrDenied
	}
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(raw)))
	if n, e := write.Write(header[:]); e != nil || n != 4 {
		return nil, ErrDenied
	}
	if n, e := write.Write(raw); e != nil || n != len(raw) {
		return nil, ErrDenied
	}
	write.Close()
	read.Close()
	childFile.Close()
	readyDeadline := time.Now().Add(10 * time.Second)
	if d, ok := ctx.Deadline(); ok && d.Before(readyDeadline) {
		readyDeadline = d
	}
	remaining := time.Duration(deadline-now) * time.Millisecond
	if limit := time.Now().Add(remaining); limit.Before(readyDeadline) {
		readyDeadline = limit
	}
	_ = control.SetReadDeadline(readyDeadline)
	go func() {
		var ready [41]byte
		if _, e := io.ReadFull(control, ready[:]); e != nil || !bytes.Equal(ready[:5], []byte("RSHR\x01")) || binary.BigEndian.Uint32(ready[5:9]) != generation || !bytes.Equal(ready[9:], nonce[:]) {
			control.Close()
			return
		}
		owned.mu.Lock()
		if !owned.reaped {
			owned.ready = true
		}
		owned.mu.Unlock()
	}()
	failed = false
	return owned, nil
}
func (e *ownedEngine) state() (ready, reaped bool, pid uint32) {
	if e == nil {
		return false, true, 0
	}
	e.mu.Lock()
	defer e.mu.Unlock()
	return e.ready, e.reaped, e.pid
}
func (e *ownedEngine) stop(ctx context.Context) error {
	if e == nil {
		return nil
	}
	if e.control != nil {
		e.control.Close()
	}
	if e.input != nil {
		e.input.Close()
	}
	if e.output != nil {
		e.output.Close()
	}
	if e.cmd == nil {
		return nil
	}
	_, reaped, _ := e.state()
	if !reaped {
		select {
		case <-e.done:
		case <-time.After(100 * time.Millisecond):
		case <-ctx.Done():
			return ErrDenied
		}
	}
	// The owned exec.Cmd handle cannot target a caller-supplied or reused PID.
	if !reaped {
		_ = e.cmd.Process.Kill()
	}
	// Kill every descendant in the exact still-owned cgroup as well. Never
	// enumerate-and-kill arbitrary numeric PIDs or declare Close to be exit.
	if writeKernel(groupPath+"/cgroup.kill", "1") != nil {
		return ErrDenied
	}
	select {
	case <-e.done:
		return nil
	case <-ctx.Done():
		return ErrDenied
	case <-time.After(4 * time.Second):
		return ErrDenied
	}
}
