// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"encoding/json"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"io"
	"os"
	"strings"
	"testing"
	"time"
)

func sampleStatus() bridge.Status {
	age := uint64(0)
	return bridge.Status{Contract: 1, Source: "engine", Generation: 7, Sequence: 1, AgeMS: &age, Enabled: "unknown", Service: "unknown", Path: "unknown", DHT: "unknown"}
}
func sampleReply() []byte {
	id := "11111111-1111-4111-8111-111111111111"
	raw, _ := json.Marshal(bridge.Response{Protocol: 1, ID: &id, Result: &bridge.Result{Kind: "status", Status: sampleStatus(), SDKLinked: true}})
	return raw
}
func TestSDKReplyRejectsWrongContextDuplicatesAndPrivateText(t *testing.T) {
	id := "11111111-1111-4111-8111-111111111111"
	raw := sampleReply()
	if _, err := sdkReply(raw, id, 7, 1); err != nil {
		t.Fatal(err)
	}
	for _, bad := range [][]byte{bytes.Replace(raw, []byte(`"protocol":1`), []byte(`"protocol":1,"protocol":1`), 1), bytes.Replace(raw, []byte(`"source":"engine"`), []byte(`"source":"engine","source":"engine"`), 1), bytes.Replace(raw, []byte(`"nkn_sdk_linked":true`), []byte(`"nkn_sdk_linked":false`), 1), bytes.Replace(raw, []byte(`"last_error":null`), []byte(`"last_error":"SYNTHETIC_PRIVATE"`), 1), append(append([]byte{}, raw...), raw...), []byte("null"), bytes.Repeat([]byte(" "), 2049)} {
		if _, err := sdkReply(bad, id, 7, 1); err == nil {
			t.Fatal("accepted invalid SDK reply")
		}
	}
	for _, v := range []struct {
		id   string
		g, s uint32
	}{{"22222222-2222-4222-8222-222222222222", 7, 1}, {id, 8, 1}, {id, 7, 2}} {
		if _, err := sdkReply(raw, v.id, v.g, v.s); err == nil {
			t.Fatal("accepted wrong ordering context")
		}
	}
}

type pipeSource struct{ calls int }

func (*pipeSource) SDKLinked() bool { return true }
func (s *pipeSource) Read(_ context.Context, seq uint32) (bridge.Status, error) {
	s.calls++
	v := sampleStatus()
	v.Sequence = seq
	return v, nil
}
func TestOwnedReaderUsesOriginalStdioAndNeverClosesSharedPipes(t *testing.T) {
	inputR, inputW, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	outputR, outputW, err := os.Pipe()
	if err != nil {
		t.Fatal(err)
	}
	defer inputR.Close()
	defer inputW.Close()
	defer outputR.Close()
	defer outputW.Close()
	source := &pipeSource{}
	done := make(chan error, 1)
	go func() { done <- bridge.ServeOwnedWithSource(context.Background(), inputR, outputW, nil, nil, source) }()
	engine := &ownedEngine{input: inputW, output: outputR, ready: true, statusEpoch: 99}
	s, at := engine.readonly(context.Background(), 7)
	if at == 0 || s.Source != "engine" || s.Generation != 99 || s.Sequence != 1 || s.RX != nil || s.Peers != nil {
		t.Fatal("wrong actual pipe projection")
	}
	second, _ := engine.readonly(context.Background(), 7)
	if second.Source != "unavailable" || second.LastError == nil || *second.LastError != "stale" || engine.statusFrames != 1 {
		t.Fatal("read budget bypassed")
	}
	if _, err = inputW.Stat(); err != nil {
		t.Fatal("read closed shared SDK input")
	}
	select {
	case <-done:
		t.Fatal("read stopped shared bridge")
	default:
	}
	inputW.Close()
	select {
	case err = <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(time.Second):
		t.Fatal("fixture leaked")
	}
	if source.calls != 1 {
		t.Fatal("query invented another observation")
	}
}
func TestPoisonedReaderDoesNotRetryOrStopSDK(t *testing.T) {
	inputR, inputW, _ := os.Pipe()
	outputR, outputW, _ := os.Pipe()
	defer inputR.Close()
	defer inputW.Close()
	defer outputR.Close()
	defer outputW.Close()
	done := make(chan struct{})
	go func() {
		defer close(done)
		var h [4]byte
		_, _ = io.ReadFull(inputR, h[:])
		n := int(h[3])
		_, _ = io.CopyN(io.Discard, inputR, int64(n))
		_, _ = outputW.Write([]byte{0, 0, 9, 0})
	}()
	engine := &ownedEngine{input: inputW, output: outputR, ready: true, statusEpoch: 2}
	s, _ := engine.readonly(context.Background(), 7)
	if s.Source != "unavailable" || !engine.statusPoisoned {
		t.Fatal("bad boundary accepted")
	}
	<-done
	before := engine.statusFrames
	s, _ = engine.readonly(context.Background(), 7)
	if s.Source != "unavailable" || engine.statusFrames != before {
		t.Fatal("poisoned pipe retried")
	}
	if _, err := inputW.Stat(); err != nil {
		t.Fatal("status error closed shared child")
	}
	if !engine.ready || engine.reaped {
		t.Fatal("stdio error invented process exit")
	}
}
func TestPolicyRequiresExplicitReadOnlyProtocolAndOldPolicyBytesStayStable(t *testing.T) {
	c := configuration{Format: 1, Source: strings.Repeat("a", 40), Executable: strings.Repeat("b", 64), CorePin: strings.Repeat("c", 64), StatisticsKey: strings.Repeat("d", 64), Interface: "wlan0", Seeds: []string{"https://example.invalid"}}
	if !configurationValid(c) {
		t.Fatal("old policy changed")
	}
	raw, _ := json.Marshal(c)
	if bytes.Contains(raw, []byte("status_protocol")) {
		t.Fatal("old canonical policy hash changed")
	}
	c.StatusProtocol = 1
	if !configurationValid(c) {
		t.Fatal("candidate status rejected")
	}
	c.StatusProtocol = 2
	if configurationValid(c) {
		t.Fatal("unknown protocol accepted")
	}
}
