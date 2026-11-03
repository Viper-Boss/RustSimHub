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

// Only the exact owned child pipes are sampled. An observation timeout poisons
// this reader, never closes/restarts the shared SDK or renews its Root lease.
// Root's original process/kernel checks remain a separate evidence source.
func unknownSDK(code string) bridge.Status {
	return bridge.Status{Contract: 1, Source: "unavailable", Enabled: "unknown", Service: "unknown", Path: "unknown", DHT: "unknown", LastError: &code}
}
func strictObject(dec *json.Decoder, depth int) error {
	if depth > 8 {
		return ErrDenied
	}
	token, e := dec.Token()
	if e != nil {
		return ErrDenied
	}
	switch token {
	case json.Delim('{'):
		seen := map[string]bool{}
		for dec.More() {
			t, e := dec.Token()
			k, ok := t.(string)
			if e != nil || !ok || seen[k] {
				return ErrDenied
			}
			seen[k] = true
			if strictObject(dec, depth+1) != nil {
				return ErrDenied
			}
		}
		t, e := dec.Token()
		if e != nil || t != json.Delim('}') {
			return ErrDenied
		}
	case json.Delim('['):
		for dec.More() {
			if strictObject(dec, depth+1) != nil {
				return ErrDenied
			}
		}
		t, e := dec.Token()
		if e != nil || t != json.Delim(']') {
			return ErrDenied
		}
	case json.Delim('}'), json.Delim(']'):
		return ErrDenied
	}
	return nil
}
func sdkReply(raw []byte, id string, generation, sequence uint32) (bridge.Status, error) {
	if len(raw) == 0 || len(raw) > bridge.MaxResponse {
		return bridge.Status{}, ErrDenied
	}
	pre := json.NewDecoder(bytes.NewReader(raw))
	if strictObject(pre, 0) != nil {
		return bridge.Status{}, ErrDenied
	}
	if _, e := pre.Token(); e != io.EOF {
		return bridge.Status{}, ErrDenied
	}
	dec := json.NewDecoder(bytes.NewReader(raw))
	dec.DisallowUnknownFields()
	var reply bridge.Response
	if dec.Decode(&reply) != nil || reply.Protocol != 1 || reply.ID == nil || *reply.ID != id || reply.Error != "" || reply.Result == nil || reply.Result.Kind != "status" || !reply.Result.SDKLinked {
		return bridge.Status{}, ErrDenied
	}
	var envelope map[string]json.RawMessage
	var result map[string]json.RawMessage
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &envelope) != nil || json.Unmarshal(envelope["result"], &result) != nil || json.Unmarshal(result["status"], &fields) != nil || len(fields) != 14 {
		return bridge.Status{}, ErrDenied
	}
	s := reply.Result.Status
	if !bridge.ValidStatus(s, sequence) || s.Source != "engine" || s.Generation != generation {
		return bridge.Status{}, ErrDenied
	}
	return s, nil
}
func (e *ownedEngine) readonly(ctx context.Context, generation uint32) (bridge.Status, uint64) {
	now, err := bootMillis()
	if now >= 10 {
		now -= 10
	} // conservative start stamp from the kernel centisecond floor
	if err != nil {
		return unknownSDK("engine_unavailable"), 0
	}
	unavailable := func(code string) (bridge.Status, uint64) { return unknownSDK(code), now }
	if e == nil {
		return unavailable("engine_unavailable")
	}
	if !e.statusMu.TryLock() {
		return unavailable("stale")
	}
	defer e.statusMu.Unlock()
	if e.statusPoisoned || e.statusEpoch == 0 || e.statusFrames >= 1024 || e.input == nil || e.output == nil {
		return unavailable("engine_unavailable")
	}
	// No cached sample is relabelled fresh; no more than one SDK observation per
	// second, even if an admitted Core floods its private read-only operation.
	if !e.lastStatus.IsZero() && time.Since(e.lastStatus) < time.Second {
		return unavailable("stale")
	}
	e.lastStatus = time.Now()
	ready, reaped, _ := e.state()
	if !ready || reaped {
		return unavailable("engine_unavailable")
	}
	deadline := time.Now().Add(200 * time.Millisecond)
	if d, ok := ctx.Deadline(); ok && d.Before(deadline) {
		deadline = d
	}
	if ctx.Err() != nil || !time.Now().Before(deadline) {
		return unavailable("deadline")
	}
	var nonce [16]byte
	if _, err = rand.Read(nonce[:]); err != nil {
		return unavailable("engine_unavailable")
	}
	nonce[6] = (nonce[6] & 15) | 0x40
	nonce[8] = (nonce[8] & 63) | 0x80
	text := hex.EncodeToString(nonce[:])
	id := text[:8] + "-" + text[8:12] + "-" + text[12:16] + "-" + text[16:20] + "-" + text[20:]
	request, _ := json.Marshal(protocol.Request{Protocol: 1, ID: id, Op: protocol.Status})
	var size [4]byte
	binary.BigEndian.PutUint32(size[:], uint32(len(request)))
	poison := func() (bridge.Status, uint64) { e.statusPoisoned = true; return unavailable("protocol_error") }
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
	if _, err = io.ReadFull(e.output, size[:]); err != nil {
		return poison()
	}
	length := binary.BigEndian.Uint32(size[:])
	if length == 0 || length > bridge.MaxResponse {
		return poison()
	}
	raw := make([]byte, length)
	if _, err = io.ReadFull(e.output, raw); err != nil {
		return poison()
	}
	sample, err := sdkReply(raw, id, generation, e.statusFrames)
	if err != nil || ctx.Err() != nil || !time.Now().Before(deadline) {
		return poison()
	}
	// T5 generation identifies the trusted observation process, not permission.
	// Root assigned this persistent epoch during explicit launch. The original
	// SDK/Root authorization generation was checked above and remains unchanged.
	sample.Generation = e.statusEpoch
	return sample, now
}
func (s *registration) readonly(ctx context.Context) []byte {
	reply := s.snapshot()
	var sample bridge.Status
	var at uint64
	now, clockError := bootMillis()
	if reply[85] != 1 || reply[86] != 2 || clockError != nil || now >= s.request.deadline {
		sample = unknownSDK("engine_unavailable")
		at, _ = bootMillis()
		if at >= 10 {
			at -= 10
		}
	} else {
		sample, at = s.engine.readonly(ctx, s.request.generation)
	}
	raw, err := json.Marshal(sample)
	if err != nil || len(raw) > bridge.MaxResponse {
		raw, _ = json.Marshal(unknownSDK("protocol_error"))
	}
	tail := make([]byte, 17)
	copy(tail, []byte("RSRD\x01"))
	binary.BigEndian.PutUint64(tail[5:13], at)
	binary.BigEndian.PutUint32(tail[13:17], uint32(len(raw)))
	reply = append(reply, tail...)
	return append(reply, raw...)
}
