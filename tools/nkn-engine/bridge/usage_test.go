// SPDX-License-Identifier: AGPL-3.0-only
package bridge

import (
	"bytes"
	"context"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"strings"
	"testing"
)

type usageSource struct {
	sourceFixture
	queries int
	window  string
	count   uint64
	cancel  context.CancelFunc
}

func (s *usageSource) Usage(ctx context.Context, op protocol.Operation) (UsageCount, error) {
	s.queries++
	if s.cancel != nil {
		s.cancel()
	}
	return UsageCount{Window: s.window, Count: s.count}, nil
}
func TestUsageRequiresSeparateSourceAndNoStatusIO(t *testing.T) {
	input := frame(strings.Replace(request, "status", "usage_24h", 1))
	for _, source := range []StatusSource{nil, &sourceFixture{status: freshStatus()}} {
		var out bytes.Buffer
		if ServeWithSource(context.Background(), bytes.NewReader(input), &out, nil, nil, source) != nil {
			t.Fatal("serve")
		}
		r := decode(t, out.Bytes())
		if len(r) != 1 || r[0].Usage != nil || r[0].Error != "usage_unavailable" {
			t.Fatal("default source queried")
		}
	}
	source := &usageSource{window: "24h", count: 0}
	var out bytes.Buffer
	if ServeWithSource(context.Background(), bytes.NewReader(input), &out, nil, nil, source) != nil {
		t.Fatal("serve")
	}
	r := decode(t, out.Bytes())
	if source.queries != 1 || source.calls != 0 || r[0].Usage == nil || r[0].Usage.Count != 0 {
		t.Fatal("query/status confused")
	}
}
func TestUsageWrongWindowOverflowAndCancellationAreNotPublished(t *testing.T) {
	input := frame(strings.Replace(request, "status", "usage_24h", 1))
	for _, source := range []*usageSource{{window: "30d", count: 1}, {window: "24h", count: 1000000001}} {
		var out bytes.Buffer
		_ = ServeWithSource(context.Background(), bytes.NewReader(input), &out, nil, nil, source)
		if r := decode(t, out.Bytes()); r[0].Usage != nil || r[0].Error == "" {
			t.Fatal("bad source accepted")
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	source := &usageSource{window: "24h", count: 1, cancel: cancel}
	var out bytes.Buffer
	if ServeWithSource(ctx, bytes.NewReader(input), &out, nil, nil, source) == nil || out.Len() != 0 {
		t.Fatal("late count published")
	}
}
