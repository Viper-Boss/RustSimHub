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
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
)

func usageReply(raw []byte, id string, op protocol.Operation) (*bridge.UsageCount, error) {
	if len(raw) == 0 || len(raw) > bridge.MaxResponse {
		return nil, ErrDenied
	}
	pre := json.NewDecoder(bytes.NewReader(raw))
	if strictObject(pre, 0) != nil {
		return nil, ErrDenied
	}
	if _, e := pre.Token(); e != io.EOF {
		return nil, ErrDenied
	}
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	var r bridge.Response
	if dec.Decode(&r) != nil || r.Protocol != 1 || r.ID == nil || *r.ID != id || r.Result != nil {
		return nil, ErrDenied
	}
	if r.Error == "usage_unavailable" && r.Usage == nil {
		return nil, nil
	}
	if r.Error != "" || r.Usage == nil || r.Usage.Window != protocol.UsageWindow(op) || r.Usage.Count > 1000000000 {
		return nil, ErrDenied
	}
	// Typed decoding alone would accept a missing/null count as zero.
	var envelope map[string]json.RawMessage
	var count map[string]json.RawMessage
	if json.Unmarshal(raw, &envelope) != nil || json.Unmarshal(envelope["usage"], &count) != nil || len(count) != 2 || bytes.Equal(count["count"], []byte("null")) || count["count"] == nil {
		return nil, ErrDenied
	}
	return r.Usage, nil
}

func (s *registration) usage(ctx context.Context, op byte) []byte {
	reply := s.snapshot()
	var value *bridge.UsageCount
	now, err := bootMillis()
	if reply[85] == 1 && reply[86] == 2 && err == nil && now < s.request.deadline {
		value = s.engine.usage(ctx, op)
	}
	raw := []byte(`{"status":"unknown"}`)
	if value != nil {
		raw, _ = json.Marshal(value)
	}
	// Bound to the exact original Root registration, no lease renewal or launch.
	tail := make([]byte, 9)
	copy(tail, []byte("RSUC\x01"))
	binary.BigEndian.PutUint32(tail[5:], uint32(len(raw)))
	return append(append(reply, tail...), raw...)
}

func (e *ownedEngine) usage(ctx context.Context, op byte) *bridge.UsageCount {
	if e == nil || op < usage24h || op > usage90d || !e.statusMu.TryLock() {
		return nil
	}
	defer e.statusMu.Unlock()
	index := int(op - usage24h)
	if e.statusPoisoned || e.statusEpoch == 0 || e.statusFrames >= 1024 || e.input == nil || e.output == nil || !e.lastUsage[index].IsZero() && time.Since(e.lastUsage[index]) < 5*time.Second {
		return nil
	}
	ready, reaped, _ := e.state()
	if !ready || reaped {
		return nil
	}
	e.lastUsage[index] = time.Now()
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
	nonce[6] = (nonce[6] & 15) | 0x40
	nonce[8] = (nonce[8] & 63) | 0x80
	text := hex.EncodeToString(nonce[:])
	id := text[:8] + "-" + text[8:12] + "-" + text[12:16] + "-" + text[16:20] + "-" + text[20:]
	operation := []protocol.Operation{protocol.Usage24h, protocol.Usage30d, protocol.Usage90d}[index]
	request, _ := json.Marshal(protocol.Request{Protocol: 1, ID: id, Op: operation})
	poison := func() *bridge.UsageCount { e.statusPoisoned = true; return nil }
	var size [4]byte
	binary.BigEndian.PutUint32(size[:], uint32(len(request)))
	if e.input.SetWriteDeadline(deadline) != nil || e.output.SetReadDeadline(deadline) != nil {
		return poison()
	}
	if n, err := e.input.Write(size[:]); err != nil || n != len(size) {
		return poison()
	}
	if n, err := e.input.Write(request); err != nil || n != len(request) {
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
	raw := make([]byte, length)
	if _, err := io.ReadFull(e.output, raw); err != nil {
		return poison()
	}
	value, err := usageReply(raw, id, operation)
	if err != nil || ctx.Err() != nil || !time.Now().Before(deadline) {
		return poison()
	}
	return value
}
