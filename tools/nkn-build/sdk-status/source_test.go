// SPDX-License-Identifier: AGPL-3.0-only
package sdkevaluation

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"io"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/sdkstatus"
)

func TestSDKSourceSamplesActualClientWithoutNetworkOrInventedProcessState(t *testing.T) {
	f := connected(t)
	s, err := sdkstatus.Attach(f.client, 41)
	if err != nil {
		t.Fatal("actual SDK source attach failed")
	}
	attempts, calls := f.attempts.Load(), f.rpcCalls.Load()
	active, err := s.Read(context.Background(), 1)
	if err != nil || active.Source != "engine" || active.Generation != 41 || active.Sequence != 1 ||
		active.AgeMS == nil || active.LastError != nil || active.Enabled != "unknown" || active.Service != "unknown" ||
		active.Path != "unknown" || active.DHT != "unknown" || active.RX != nil || active.TX != nil || active.RTT != nil || active.Peers != nil {
		t.Fatal("actual SDK sample invented missing evidence")
	}
	if _, err = s.Read(context.Background(), 1); err == nil {
		t.Fatal("replayed sequence accepted")
	}
	if f.attempts.Load() != attempts || f.rpcCalls.Load() != calls {
		t.Fatal("read-only status performed IO")
	}
	_ = f.client.Close()
	closed, err := s.Read(context.Background(), 2)
	if err != nil || closed.LastError == nil || *closed.LastError != "engine_unavailable" ||
		closed.Service != "unknown" || closed.Enabled != "unknown" {
		t.Fatal("SDK flag was mistaken for process exit/Owner switch")
	}
	if output := os.Getenv("RSH_SDK_STATUS_OUTPUT"); output != "" {
		data, err := json.Marshal([]bridge.Status{active, closed})
		if err != nil || os.WriteFile(output, data, 0o600) != nil {
			t.Fatal("synthetic public projection capture failed")
		}
	}
}

func TestSDKSourceRetirementDoesNotCloseSharedClient(t *testing.T) {
	f := connected(t)
	s, err := sdkstatus.Attach(f.client, 42)
	if err != nil {
		t.Fatal(err)
	}
	s.Retire()
	if _, err = s.Read(context.Background(), 1); err == nil || f.client.IsClosed() {
		t.Fatal("retirement adopted stale context or closed shared SDK")
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	if height, err := f.client.GetHeightContext(ctx); err != nil || height != 42 {
		t.Fatal("explicit owned loopback RPC no longer worked for remaining owner")
	}
}

func TestSDKSourceRejectsMissingContextCancelledAndCrossBridgeSequence(t *testing.T) {
	f := connected(t)
	if _, err := sdkstatus.Attach(nil, 1); err == nil {
		t.Fatal("nil SDK source accepted")
	}
	if _, err := sdkstatus.Attach(f.client, 0); err == nil {
		t.Fatal("unknown generation accepted")
	}
	s, _ := sdkstatus.Attach(f.client, 43)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := s.Read(ctx, 1); err == nil {
		t.Fatal("cancelled sample accepted")
	}
	if _, err := s.Read(context.Background(), 1); err != nil {
		t.Fatal("cancelled call consumed the sequence")
	}
	var out bytes.Buffer
	request := []byte(`{"protocol":1,"id":"11111111-1111-4111-8111-111111111111","op":"status"}`)
	var input bytes.Buffer
	_ = binary.Write(&input, binary.BigEndian, uint32(len(request)))
	input.Write(request)
	if bridge.ServeWithSource(context.Background(), &input, &out, nil, nil, s) != nil || !bytes.Contains(out.Bytes(), []byte("status_unavailable")) {
		t.Fatal("new bridge silently reset an old observation context")
	}
}

func TestSDKSourceActualBridgeOutputPreservesT5AndDoesNotCloseClient(t *testing.T) {
	f := connected(t)
	s, _ := sdkstatus.Attach(f.client, 44)
	var input, output bytes.Buffer
	for _, op := range []string{"status", "stop"} {
		request := []byte(`{"protocol":1,"id":"11111111-1111-4111-8111-111111111111","op":"` + op + `"}`)
		_ = binary.Write(&input, binary.BigEndian, uint32(len(request)))
		input.Write(request)
	}
	attempts, calls := f.attempts.Load(), f.rpcCalls.Load()
	if bridge.ServeWithSource(context.Background(), &input, &output, nil, nil, s) != nil || f.client.IsClosed() ||
		f.attempts.Load() != attempts || f.rpcCalls.Load() != calls {
		t.Fatal("status/bridge stop changed the SDK lifecycle/network")
	}
	for _, private := range []string{f.client.Address(), hex.EncodeToString(f.client.PubKey()), "127.0.0.1", "http://"} {
		if strings.Contains(output.String(), private) {
			t.Fatal("private SDK endpoint/identity escaped into stdio")
		}
	}
	for i := 0; i < 2; i++ {
		var length uint32
		if binary.Read(&output, binary.BigEndian, &length) != nil || length > bridge.MaxResponse {
			t.Fatal("invalid actual response frame")
		}
		data := make([]byte, int(length))
		if _, err := io.ReadFull(&output, data); err != nil {
			t.Fatal(err)
		}
		var response bridge.Response
		if json.Unmarshal(data, &response) != nil || response.Result == nil || response.ID == nil {
			t.Fatal("actual bridge response did not decode")
		}
		if i == 0 && (!response.Result.SDKLinked || response.Result.Status.Source != "engine") ||
			i == 1 && response.Result.Kind != "closing_this_bridge" {
			t.Fatal("wrong actual source/close projection")
		}
	}
}
