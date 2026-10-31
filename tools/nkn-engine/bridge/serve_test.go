// SPDX-License-Identifier: AGPL-3.0-only
package bridge

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"testing"
)

const request = `{"protocol":1,"id":"11111111-1111-4111-8111-111111111111","op":"status"}`

func frame(data string) []byte {
	var b bytes.Buffer
	_ = binary.Write(&b, binary.BigEndian, uint32(len(data)))
	b.WriteString(data)
	return b.Bytes()
}
func decode(t *testing.T, data []byte) []Response {
	t.Helper()
	r := bytes.NewReader(data)
	var results []Response
	for r.Len() != 0 {
		var n uint32
		if binary.Read(r, binary.BigEndian, &n) != nil || n > MaxResponse {
			t.Fatal("bad response frame")
		}
		b := make([]byte, n)
		if _, err := io.ReadFull(r, b); err != nil {
			t.Fatal("truncated response")
		}
		var out Response
		if json.Unmarshal(b, &out) != nil {
			t.Fatal("invalid response")
		}
		results = append(results, out)
	}
	return results
}
func TestActualUnavailableStatusAndStop(t *testing.T) {
	var out bytes.Buffer
	data := append(frame(request), frame(strings.Replace(request, "status", "stop", 1))...)
	data = append(data, frame(request)...)
	if Serve(context.Background(), bytes.NewReader(data), &out, nil, nil) != nil {
		t.Fatal("finite serve failed")
	}
	r := decode(t, out.Bytes())
	if len(r) != 2 || r[1].Result.Kind != "closing_this_bridge" {
		t.Fatal("stop did not end local loop")
	}
	s := r[0].Result.Status
	if s.Source != "unavailable" || s.Enabled != "unknown" || s.Service != "unknown" || s.RX != nil || s.TX != nil || s.RTT != nil || s.Peers != nil || s.AgeMS != nil || r[0].Result.SDKLinked {
		t.Fatal("fabricated NKN evidence")
	}
}
func TestFrameTruncationLimitsAndRedaction(t *testing.T) {
	for _, input := range [][]byte{{1}, {0, 0, 0, 0}, {0, 0, 4, 1}, append([]byte{0, 0, 0, 4}, 'x')} {
		var out bytes.Buffer
		if !errors.Is(Serve(context.Background(), bytes.NewReader(input), &out, nil, nil), ErrFrame) {
			t.Fatal("invalid frame accepted")
		}
		if len(decode(t, out.Bytes())) != 1 {
			t.Fatal("no finite error")
		}
	}
	var out bytes.Buffer
	bad := frame(strings.Replace(request, `"protocol":1`, `"protocol":1,"seed":"SYNTHETIC_MUST_NOT_ECHO"`, 1))
	if Serve(context.Background(), bytes.NewReader(bad), &out, nil, nil) != nil {
		t.Fatal("bounded JSON error should not desync frame")
	}
	if bytes.Contains(out.Bytes(), []byte("SYNTHETIC")) {
		t.Fatal("input echoed")
	}
	r := decode(t, out.Bytes())
	if r[0].ID != nil || r[0].Error != "invalid_request" {
		t.Fatal("untrusted ID adopted")
	}
}
func TestBudgetAndCancelledSourceHaveNoDispatch(t *testing.T) {
	var out bytes.Buffer
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if !errors.Is(Serve(ctx, bytes.NewReader(frame(request)), &out, nil, nil), context.Canceled) || out.Len() != 0 {
		t.Fatal("cancelled input dispatched")
	}
	calls := 0
	fail := func() error { calls++; return io.ErrClosedPipe }
	if Serve(context.Background(), bytes.NewReader(frame(request)), &out, fail, nil) == nil || out.Len() != 0 || calls != 1 {
		t.Fatal("read deadline failure ignored")
	}
	if Serve(context.Background(), bytes.NewReader(frame(request)), &out, nil, fail) == nil || out.Len() != 0 {
		t.Fatal("write deadline failure ignored")
	}
}
func TestFiniteRequestCountAndEOF(t *testing.T) {
	var out bytes.Buffer
	input := bytes.Repeat(frame(request), MaxFrames+1)
	if !errors.Is(Serve(context.Background(), bytes.NewReader(input), &out, nil, nil), ErrLimit) {
		t.Fatal("unbounded request loop")
	}
	r := decode(t, out.Bytes())
	if len(r) != MaxFrames+1 || r[MaxFrames].Error != "frame_limit" {
		t.Fatal("wrong frame cap")
	}
	out.Reset()
	if Serve(context.Background(), bytes.NewReader(nil), &out, nil, nil) != nil || out.Len() != 0 {
		t.Fatal("clean EOF changed")
	}
}
