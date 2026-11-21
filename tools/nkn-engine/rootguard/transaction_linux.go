// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"io"
	"net"
	"syscall"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

func transactionOperation(op byte) usagetx.Operation {
	if op >= byte(usagetx.Preflight) && op <= byte(usagetx.Query) {
		return usagetx.Operation(op)
	}
	return 0
}

// A transaction uses its own one-shot Core observer socket. Cancelling it must
// not close the original Root owner connection or stop Remote/Manual users.
func (m *manager) transactionAllowed(r request, c *net.UnixConn) bool {
	if c == nil || !peerIsCore(c, m.policy.roles.core) || !m.mu.TryLock() {
		return false
	}
	defer m.mu.Unlock()
	return m.transactionAllowedLocked(r, c)
}
func (m *manager) transactionAllowedLocked(r request, c *net.UnixConn) bool {
	s := m.scope
	now, e := bootMillis()
	return transactionOperation(r.op) != 0 && m.policy.config.UsageTransactionProtocol == 1 && r.policy == m.policy.hash &&
		s.matches(r) && c != nil && c != s.owner && s.owner != nil && !s.unknown && !s.released && e == nil && now < r.deadline &&
		s.engine != nil && s.engine.transactionProtocol == 1
}

func readTransaction(r io.Reader, op usagetx.Operation) (usagetx.Request, error) {
	var header [9]byte
	if _, e := io.ReadFull(r, header[:]); e != nil || !bytes.Equal(header[:5], []byte("RSTX\x01")) {
		return usagetx.Request{}, ErrDenied
	}
	size := binary.BigEndian.Uint32(header[5:])
	if size == 0 || size > usagetx.MaxRequestBytes {
		return usagetx.Request{}, ErrDenied
	}
	raw := make([]byte, size)
	if _, e := io.ReadFull(r, raw); e != nil {
		return usagetx.Request{}, ErrDenied
	}
	v, e := usagetx.Parse(op, raw)
	if e != nil {
		return usagetx.Request{}, ErrDenied
	}
	return v, nil
}

func transactionTail(snapshot []byte, v usagetx.Reply) []byte {
	raw, e := json.Marshal(v)
	if e != nil || len(raw) > usagetx.MaxRequestBytes {
		return nil
	}
	header := make([]byte, 9)
	copy(header, []byte("RSTX\x01"))
	binary.BigEndian.PutUint32(header[5:], uint32(len(raw)))
	return append(append(snapshot, header...), raw...)
}

// No read consumes a future frame. EOF, unexpected pipelining, or descriptor
// failure cancels only this observer. The goroutine is joined before return.
func observerOpen(c *net.UnixConn) bool {
	raw, e := c.SyscallConn()
	if e != nil {
		return false
	}
	open := false
	e = raw.Control(func(fd uintptr) {
		var one [1]byte
		_, _, err := syscall.Recvfrom(int(fd), one[:], syscall.MSG_PEEK|syscall.MSG_DONTWAIT)
		open = err == syscall.EAGAIN || err == syscall.EWOULDBLOCK
	})
	return e == nil && open
}
func watchObserver(parent context.Context, c *net.UnixConn) (context.Context, func()) {
	ctx, cancel := context.WithCancel(parent)
	stop := make(chan struct{})
	done := make(chan struct{})
	if !observerOpen(c) {
		cancel()
	}
	go func() {
		defer close(done)
		tick := time.NewTicker(10 * time.Millisecond)
		defer tick.Stop()
		for {
			select {
			case <-stop:
				return
			case <-ctx.Done():
				return
			case <-tick.C:
				if !observerOpen(c) {
					cancel()
					return
				}
			}
		}
	}()
	return ctx, func() { close(stop); cancel(); <-done }
}

func (m *manager) handleTransaction(parent context.Context, r request, c *net.UnixConn, tx usagetx.Request) ([]byte, error) {
	op := transactionOperation(r.op)
	if op == 0 || tx.Validate(op) != nil || c == nil || !peerIsCore(c, m.policy.roles.core) || !m.mu.TryLock() {
		return nil, ErrDenied
	}
	defer m.mu.Unlock()
	if !m.transactionAllowedLocked(r, c) || tx.PublicKey != m.policy.config.StatisticsKey {
		return nil, ErrDenied
	}
	if op == usagetx.Submit {
		if _, e := usagetx.VerifyCanonicalSubmission(tx); e != nil {
			return nil, ErrDenied
		}
	}
	s := m.scope
	now, e := bootMillis()
	if e != nil || now >= r.deadline {
		return nil, ErrDenied
	}
	duration := min(1750*time.Millisecond, time.Duration(r.deadline-now)*time.Millisecond)
	bounded, cancel := context.WithTimeout(parent, duration)
	defer cancel()
	ctx, finish := watchObserver(bounded, c)
	defer finish()
	v, _ := usagetx.NewReply(op, tx, "unavailable", nil)
	snapshot := s.snapshot()
	if ctx.Err() == nil && snapshot[85] == 1 && snapshot[86] == 2 {
		if result := s.engine.transaction(ctx, op, tx); result != nil {
			v = *result
		}
	}
	snapshot = s.snapshot()
	if ctx.Err() != nil || snapshot[85] != 1 || snapshot[86] != 2 {
		state := "unavailable"
		if op != usagetx.Preflight {
			state = "unknown"
		}
		v, _ = usagetx.NewReply(op, tx, state, nil)
	}
	return transactionTail(snapshot, v), nil
}

func transactionReply(raw []byte, id string, op usagetx.Operation, r usagetx.Request) (*usagetx.Reply, error) {
	f, e := usagetx.Fields(raw, bridge.MaxResponse)
	if e != nil || len(f) != 3 || f["protocol"] == nil || f["id"] == nil {
		return nil, ErrDenied
	}
	var version uint8
	var actualID string
	if json.Unmarshal(f["protocol"], &version) != nil || version != 1 || json.Unmarshal(f["id"], &actualID) != nil || actualID != id {
		return nil, ErrDenied
	}
	if errorValue, ok := f["error"]; ok {
		var code string
		if json.Unmarshal(errorValue, &code) != nil || code != "transaction_unavailable" {
			return nil, ErrDenied
		}
		return nil, nil
	}
	if f["transaction"] == nil {
		return nil, ErrDenied
	}
	v, e := usagetx.ParseReply(op, r, f["transaction"])
	if e != nil {
		return nil, ErrDenied
	}
	return &v, nil
}

func (e *ownedEngine) transaction(ctx context.Context, op usagetx.Operation, tx usagetx.Request) *usagetx.Reply {
	if e == nil || e.transactionProtocol != 1 || tx.Validate(op) != nil || op < usagetx.Preflight || op > usagetx.Query || !e.statusMu.TryLock() {
		return nil
	}
	defer e.statusMu.Unlock()
	index := int(op - usagetx.Preflight)
	if e.statusPoisoned || e.statusEpoch == 0 || e.statusFrames >= 1024 || e.input == nil || e.output == nil || !e.lastTransaction[index].IsZero() && time.Since(e.lastTransaction[index]) < 5*time.Second {
		return nil
	}
	ready, reaped, _ := e.state()
	if !ready || reaped {
		return nil
	}
	deadline := time.Now().Add(1500 * time.Millisecond)
	if d, ok := ctx.Deadline(); ok && d.Before(deadline) {
		deadline = d
	}
	if ctx.Err() != nil || !time.Now().Before(deadline) {
		return nil
	}
	var nonce [16]byte
	if _, err := rand.Read(nonce[:]); err != nil {
		return nil
	}
	nonce[6] = (nonce[6] & 15) | 64
	nonce[8] = (nonce[8] & 63) | 128
	text := hex.EncodeToString(nonce[:])
	id := text[:8] + "-" + text[8:12] + "-" + text[12:16] + "-" + text[16:20] + "-" + text[20:]
	operation := []protocol.Operation{protocol.UsageNonce, protocol.UsageSubmit, protocol.UsageQuery}[index]
	raw, _ := json.Marshal(protocol.Request{Protocol: 1, ID: id, Op: operation, Transaction: &tx})
	if len(raw) > protocol.MaxOwnedRequestBytes {
		return nil
	}
	poison := func() *usagetx.Reply { e.statusPoisoned = true; return nil }
	if e.input.SetWriteDeadline(deadline) != nil || e.output.SetReadDeadline(deadline) != nil {
		return poison()
	}
	interrupted := make(chan struct{})
	stop := context.AfterFunc(ctx, func() {
		defer close(interrupted)
		_ = e.input.SetWriteDeadline(time.Now())
		_ = e.output.SetReadDeadline(time.Now())
	})
	defer func() {
		if !stop() {
			<-interrupted
		}
	}()
	e.lastTransaction[index] = time.Now()
	var size [4]byte
	binary.BigEndian.PutUint32(size[:], uint32(len(raw)))
	if ctx.Err() != nil {
		return nil
	}
	if n, err := e.input.Write(size[:]); err != nil || n != len(size) {
		return poison()
	}
	if n, err := e.input.Write(raw); err != nil || n != len(raw) {
		return poison()
	}
	e.statusFrames++
	if _, err := io.ReadFull(e.output, size[:]); err != nil {
		return poison()
	}
	length := binary.BigEndian.Uint32(size[:])
	if length == 0 || length > bridge.MaxResponse {
		return poison()
	}
	value := make([]byte, length)
	if _, err := io.ReadFull(e.output, value); err != nil {
		return poison()
	}
	v, err := transactionReply(value, id, op, tx)
	if err != nil || ctx.Err() != nil || !time.Now().Before(deadline) {
		return poison()
	}
	return v
}
