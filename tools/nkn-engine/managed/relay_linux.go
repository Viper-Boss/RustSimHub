// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"io"
	"net"
	"strings"
	"sync"
	"syscall"
	"time"
)

// relay owns exactly one peer/Core descriptor. Network identities and encrypted
// flags are transport metadata; only the original Core Noise/Auth path grants
// business authority. On any uncertain exchange the descriptor is discarded.
type relay struct {
	lease             *Lease
	mu                sync.Mutex
	conn              *net.UnixConn
	source            string
	phase             uint8
	handshakeDeadline time.Time
	seen              map[[8]byte][32]byte
}

func (r *relay) close() {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.conn != nil {
		_ = r.conn.Close()
	}
	r.conn = nil
	r.source = ""
	r.phase = 0
}
func (r *relay) current() *net.UnixConn { r.mu.Lock(); defer r.mu.Unlock(); return r.conn }
func canonicalSource(source string) bool {
	if len(source) != 64 || strings.ToLower(source) != source {
		return false
	}
	var raw [32]byte
	_, e := hex.Decode(raw[:], []byte(source))
	return e == nil && raw != [32]byte{}
}
func validPacket(data []byte) bool {
	return len(data) >= 2 && len(data) <= 2050 && data[0] == 1 && data[1] >= 1 && data[1] <= 3 && (data[1] != 3 || len(data) == 2)
}
func freshHello(data []byte) bool {
	return len(data) == 108 && data[0] == 1 && data[1] == 1 && data[2] == 1 && data[3] == 2 && binary.BigEndian.Uint64(data[36:44]) != 0
}
func (r *relay) connect(ctx context.Context, source string) error {
	if r.lease.Check() != nil {
		return ErrLaunch
	}
	c, e := (&net.Dialer{Timeout: time.Second}).DialContext(ctx, "unix", CoreSocket)
	if e != nil {
		return ErrLaunch
	}
	u, ok := c.(*net.UnixConn)
	if !ok {
		c.Close()
		return ErrLaunch
	}
	fd, e := u.SyscallConn()
	if e != nil {
		u.Close()
		return ErrLaunch
	}
	valid := false
	if e = fd.Control(func(raw uintptr) {
		peer, err := syscall.GetsockoptUcred(int(raw), syscall.SOL_SOCKET, syscall.SO_PEERCRED)
		valid = err == nil && peer.Pid > 1 && peer.Uid == r.lease.launch.coreUID
	}); e != nil || !valid {
		u.Close()
		return ErrLaunch
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.conn != nil || r.lease.Check() != nil {
		u.Close()
		return ErrLaunch
	}
	r.conn = u
	r.source = source
	r.phase = 0
	r.handshakeDeadline = time.Now().Add(10 * time.Second)
	return nil
}

// exchange runs serially on the owned SDK consumer, without retry or replay.
// Duplicate SDK IDs are dropped before Core, including after disconnection;
// the finite lifetime-wide map cannot be reset by a remote reconnect.
func (r *relay) exchange(source string, id [8]byte, data []byte) ([]byte, error) {
	if !canonicalSource(source) || !validPacket(data) || id == [8]byte{} || r.lease.Check() != nil {
		return nil, ErrLaunch
	}
	hash := sha256.Sum256(data)
	// A different peer cannot cancel or replace the current admitted peer,
	// including by supplying a colliding SDK message identifier.
	r.mu.Lock()
	active := r.conn != nil
	other := active && r.source != source
	r.mu.Unlock()
	if other {
		return nil, nil
	}
	if r.seen == nil {
		r.seen = make(map[[8]byte][32]byte)
	}
	if previous, ok := r.seen[id]; ok {
		if previous != hash {
			r.close()
			return nil, ErrLaunch
		}
		return nil, nil
	}
	if len(r.seen) >= 128 {
		r.close()
		return nil, ErrLaunch
	}
	r.seen[id] = hash
	if !active {
		if !freshHello(data) {
			return nil, ErrLaunch
		}
		if e := r.connect(r.lease.Context(), source); e != nil {
			return nil, e
		}
	}
	c := r.current()
	if c == nil {
		return nil, ErrLaunch
	}
	r.mu.Lock()
	phase, handshakeDeadline := r.phase, r.handshakeDeadline
	r.mu.Unlock()
	failed := true
	defer func() {
		if failed {
			r.close()
		}
	}()
	if data[1] == 3 {
		return nil, ErrLaunch
	}
	if phase < 4 && data[1] != 1 || phase == 4 && data[1] != 2 {
		return nil, ErrLaunch
	}
	deadline := time.Now().Add(3 * time.Second)
	if phase < 4 && handshakeDeadline.Before(deadline) {
		deadline = handshakeDeadline
	}
	if d, ok := r.lease.Context().Deadline(); ok && d.Before(deadline) {
		deadline = d
	}
	if !time.Now().Before(deadline) || c.SetDeadline(deadline) != nil {
		return nil, ErrLaunch
	}
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(data)))
	for _, part := range [][]byte{header[:], data} {
		if n, e := c.Write(part); e != nil || n != len(part) {
			return nil, ErrLaunch
		}
	}
	if _, e := io.ReadFull(c, header[:]); e != nil {
		return nil, ErrLaunch
	}
	n := binary.BigEndian.Uint32(header[:])
	if n < 2 || n > 2050 {
		return nil, ErrLaunch
	}
	reply := make([]byte, n)
	if _, e := io.ReadFull(c, reply); e != nil || !validPacket(reply) || reply[1] != data[1] || r.lease.Check() != nil {
		clear(reply)
		return nil, ErrLaunch
	}
	r.mu.Lock()
	if r.conn != c {
		r.mu.Unlock()
		clear(reply)
		return nil, ErrLaunch
	}
	if phase < 4 {
		r.phase++
	}
	r.mu.Unlock()
	failed = false
	return reply, nil
}

// Kernel EOF observation performs no read of application bytes. Core's own
// finite framing deadline may close an idle source; never reuse it for a new
// Noise handshake or auto-resend a previous operation.
func (r *relay) observeEOF() {
	c := r.current()
	if c == nil {
		return
	}
	fd, e := c.SyscallConn()
	if e != nil {
		r.close()
		return
	}
	closed := false
	e = fd.Control(func(raw uintptr) {
		var b [1]byte
		n, _, err := syscall.Recvfrom(int(raw), b[:], syscall.MSG_PEEK|syscall.MSG_DONTWAIT)
		closed = err == nil && n == 0 || err != nil && err != syscall.EAGAIN && err != syscall.EWOULDBLOCK
	})
	if e != nil || closed {
		r.close()
	}
}
