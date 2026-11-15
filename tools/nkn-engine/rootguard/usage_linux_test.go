// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"context"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"os"
	"testing"
	"time"
)

type usagePipeSource struct {
	pipeSource
	usageCalls int
}

func (s *usagePipeSource) Usage(_ context.Context, op protocol.Operation) (bridge.UsageCount, error) {
	s.usageCalls++
	return bridge.UsageCount{Window: protocol.UsageWindow(op), Count: 42}, nil
}
func TestUsageOriginalPipesAreSharedWithStatusAndStayOpen(t *testing.T) {
	inputR, inputW, _ := os.Pipe()
	outputR, outputW, _ := os.Pipe()
	defer inputR.Close()
	defer inputW.Close()
	defer outputR.Close()
	defer outputW.Close()
	source := &usagePipeSource{}
	done := make(chan error, 1)
	go func() { done <- bridge.ServeOwnedWithSource(context.Background(), inputR, outputW, nil, nil, source) }()
	engine := &ownedEngine{input: inputW, output: outputR, ready: true, statusEpoch: 99}
	value := engine.usage(context.Background(), usage24h)
	if value == nil || value.Count != 42 || source.usageCalls != 1 || engine.statusFrames != 1 {
		t.Fatal("pipe count unavailable")
	}
	if engine.usage(context.Background(), usage24h) != nil || source.usageCalls != 1 {
		t.Fatal("throttle bypass")
	}
	status, _ := engine.readonly(context.Background(), 7)
	if status.Source != "engine" || status.Sequence != 2 || engine.statusFrames != 2 {
		t.Fatal("count consumed/changed status context")
	}
	if _, err := inputW.Stat(); err != nil {
		t.Fatal("query closed shared engine")
	}
	inputW.Close()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("leaked bridge")
	}
}

func TestUsageReplyExactIdentityAndWindow(t *testing.T) {
	id := "11111111-1111-4111-8111-111111111111"
	prefix := `{"protocol":1,"id":"` + id + `",`
	value, err := usageReply([]byte(prefix+`"usage":{"window":"24h","count":0}}`), id, protocol.Usage24h)
	if err != nil || value == nil || value.Count != 0 {
		t.Fatal("real zero refused")
	}
	for _, tail := range []string{
		`"usage":{"window":"30d","count":1}}`, `"usage":{"window":"24h"}}`,
		`"usage":{"window":"24h","count":null}}`, `"usage":{"window":"24h","count":1,"count":2}}`,
		`"usage":{"window":"24h","count":1000000001}}`, `"usage":{"window":"24h","count":1,"key":"private"}}`,
		`"usage":{"window":"24h","count":1},"error":"usage_unavailable"}`, `"usage":{"window":"24h","count":1}} {}`,
	} {
		if _, err := usageReply([]byte(prefix+tail), id, protocol.Usage24h); err == nil {
			t.Fatal("accepted invalid", tail)
		}
	}
	if _, err := usageReply([]byte(prefix+`"usage":{"window":"24h","count":1}}`), "other", protocol.Usage24h); err == nil {
		t.Fatal("wrong request")
	}
	value, err = usageReply([]byte(prefix+`"error":"usage_unavailable"}`), id, protocol.Usage24h)
	if err != nil || value != nil {
		t.Fatal("unknown is not zero")
	}
}

func TestUsageObserverRequiresSeparatePolicy(t *testing.T) {
	var nonce, hash [32]byte
	nonce[0] = 1
	hash[0] = 2
	r := request{op: usage24h, generation: 1, deadline: 1, nonce: nonce, policy: hash}
	reg := &registration{request: r}
	m := manager{policy: &Policy{hash: hash}, scope: reg}
	if _, err := m.handle(r, nil); err == nil {
		t.Fatal("query accepted without explicit policy and observer")
	}
	if reg.engine != nil || reg.released || reg.unknown {
		t.Fatal("query caused lifecycle mutation")
	}
}
