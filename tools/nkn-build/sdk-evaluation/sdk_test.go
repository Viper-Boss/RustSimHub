// SPDX-License-Identifier: AGPL-3.0-only
// These tests execute the exact, unmodified SDK with only synthetic identities
// and an explicitly owned loopback fixture. No public NKN/RPC/STUN operation.
package sdkevaluation

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"runtime/pprof"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gorilla/websocket"
	sdk "github.com/nknorg/nkn-sdk-go"
	"github.com/nknorg/nkngomobile"
)

func TestMain(m *testing.M) {
	// This fixture owns the SDK's process-global logger. The log-specific test
	// below captures the actual upstream logger separately, without echoing it.
	log.SetOutput(io.Discard)
	os.Exit(m.Run())
}

func wait(t *testing.T, predicate func() bool) {
	t.Helper()
	until := time.Now().Add(time.Second)
	for time.Now().Before(until) {
		if predicate() {
			return
		}
		time.Sleep(2 * time.Millisecond)
	}
	t.Fatal("bounded fixture condition not observed")
}

func reconnectWorkers() int {
	var buffer bytes.Buffer
	_ = pprof.Lookup("goroutine").WriteTo(&buffer, 2)
	return bytes.Count(buffer.Bytes(), []byte("nkn-sdk-go.(*Client).handleReconnect("))
}

func account(t *testing.T) *sdk.Account {
	t.Helper()
	account, err := sdk.NewAccount(nil)
	if err != nil {
		t.Fatal("synthetic entropy/account unavailable")
	}
	return account
}

func patched() bool { return os.Getenv("RSH_NKN_PATCHED") == "1" }

func TestConstructorFailureWorkerOwnership(t *testing.T) {
	before := reconnectWorkers()
	var attempts atomic.Int32
	deny := func(context.Context, string, string) (net.Conn, error) {
		attempts.Add(1)
		return nil, errors.New("synthetic deny before any socket or DNS")
	}
	// A reserved invalid name is never resolved: both dial paths reject first.
	client, err := sdk.NewClient(account(t), "synthetic", &sdk.ClientConfig{
		SeedRPCServerAddr: nkngomobile.NewStringArray("http://sdk-fixture.invalid:80"),
		ConnectRetries:    1, RPCTimeout: 50, RPCConcurrency: 1,
		HttpDialContext: deny, WsDialContext: deny,
	})
	if err == nil || client != nil || attempts.Load() == 0 {
		t.Fatal("actual denied constructor was not exercised")
	}
	if patched() {
		wait(t, func() bool { return reconnectWorkers() == before })
	} else {
		wait(t, func() bool { return reconnectWorkers() >= before+1 })
	}
	// This confirms an upstream limitation, NOT successful library cleanup.
	// The owning bounded test PROCESS exits; there is no returned SDK handle
	// with which an in-process adapter could close this worker.
}

type fixture struct {
	client    *sdk.Client
	attempts  atomic.Int32
	rpcCalls  atomic.Int32
	deny      atomic.Bool
	rpc       *httptest.Server
	ws        *httptest.Server
	socketsMu sync.Mutex
	sockets   []*websocket.Conn
}

func connected(t *testing.T) *fixture {
	t.Helper()
	f := &fixture{}
	f.ws = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		connection, err := (&websocket.Upgrader{}).Upgrade(w, r, nil)
		if err != nil {
			return
		}
		f.socketsMu.Lock()
		f.sockets = append(f.sockets, connection)
		f.socketsMu.Unlock()
		defer connection.Close()
		for {
			if _, _, err := connection.ReadMessage(); err != nil {
				return
			}
		}
	}))
	f.rpc = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		f.rpcCalls.Add(1)
		var request struct {
			Method string `json:"method"`
		}
		if json.NewDecoder(io.LimitReader(r.Body, 4096)).Decode(&request) != nil {
			w.WriteHeader(400)
			return
		}
		var result any
		switch request.Method {
		case "getwsaddr":
			result = map[string]any{"addr": strings.TrimPrefix(f.ws.URL, "http://"), "rpcAddr": strings.TrimPrefix(f.rpc.URL, "http://"), "pubkey": strings.Repeat("11", 32), "id": strings.Repeat("22", 32)}
		case "getnodestate":
			result = map[string]any{"syncState": "PERSIST_FINISHED"}
		case "getlatestblockheight":
			result = 42
		default:
			w.WriteHeader(400)
			return
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"result": result})
	}))
	t.Cleanup(func() {
		if f.client != nil {
			_ = f.client.Close()
		}
		f.socketsMu.Lock()
		for _, connection := range f.sockets {
			_ = connection.Close()
		}
		f.socketsMu.Unlock()
		f.ws.Close()
		f.rpc.Close()
	})
	dial := func(ctx context.Context, network, address string) (net.Conn, error) {
		f.attempts.Add(1)
		if f.deny.Load() {
			return nil, errors.New("synthetic fixed-path denial")
		}
		// Address allowlist BEFORE net.Dialer: no DNS, proxy or other fallback.
		if network != "tcp" || address != strings.TrimPrefix(f.rpc.URL, "http://") && address != strings.TrimPrefix(f.ws.URL, "http://") {
			return nil, errors.New("nonfixture address denied")
		}
		return (&net.Dialer{Timeout: 100 * time.Millisecond}).DialContext(ctx, network, address)
	}
	var err error
	f.client, err = sdk.NewClient(account(t), "synthetic", &sdk.ClientConfig{
		SeedRPCServerAddr: nkngomobile.NewStringArray(f.rpc.URL),
		ConnectRetries:    1, RPCTimeout: 100, RPCConcurrency: 1, WsHandshakeTimeout: 100,
		MinReconnectInterval: 5, MaxReconnectInterval: 10,
		HttpDialContext: dial, WsDialContext: dial,
	})
	if err != nil {
		t.Fatal("actual SDK could not connect to its owned loopback fixture")
	}
	return f
}

func TestClientWalletFixedDialer(t *testing.T) {
	f := connected(t)
	positiveCtx, positiveCancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	positiveHeight, positiveErr := f.client.GetHeightContext(positiveCtx)
	positiveCancel()
	if positiveErr != nil || positiveHeight != 42 {
		t.Fatal("ordinary bounded loopback RPC did not work")
	}
	f.deny.Store(true)
	before, calls := f.attempts.Load(), f.rpcCalls.Load()
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	height, err := f.client.GetHeightContext(ctx)
	if patched() {
		if err == nil || f.attempts.Load() <= before || f.rpcCalls.Load() != calls {
			t.Fatal("patched wallet escaped its original fixed-path denial")
		}
		return
	}
	if err != nil || height != 42 {
		t.Fatal("fixture did not observe the actual wallet RPC path")
	}
	if f.attempts.Load() != before || f.rpcCalls.Load() <= calls {
		t.Fatal("expected guard bypass was not observed")
	}
	// Only this explicitly owned loopback server was reached. This is a
	// compatibility/egress blocker, never permission to test a public endpoint.
}

func TestCloseActiveReconnectLoop(t *testing.T) {
	workers := reconnectWorkers()
	f := connected(t)
	f.deny.Store(true)
	before := f.attempts.Load()
	f.client.Reconnect(nil)
	wait(t, func() bool { return f.attempts.Load() > before })
	_ = f.client.Close()
	if !f.client.IsClosed() {
		t.Fatal("Close did not set SDK's own flag")
	}
	after := f.attempts.Load()
	if patched() {
		wait(t, func() bool { return reconnectWorkers() <= workers })
		after = f.attempts.Load()
		time.Sleep(30 * time.Millisecond)
		if f.attempts.Load() != after {
			t.Fatal("patched reconnect continued dialing after exit")
		}
		return
	}
	wait(t, func() bool { return f.attempts.Load() >= after+2 })
	// All those continued attempts are stopped by deny BEFORE a real dial.
	// Bounded child-process retirement remains necessary; IsClosed is not it.
}

type lockedLog struct {
	mu     sync.Mutex
	buffer bytes.Buffer
}

func (l *lockedLog) Write(p []byte) (int, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.buffer.Write(p)
}
func (l *lockedLog) contains(value string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	return strings.Contains(l.buffer.String(), value)
}

func TestMalformedRPCLogRedaction(t *testing.T) {
	const marker = "synthetic-sensitive-payload-never-publish"
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { _, _ = io.WriteString(w, marker) }))
	defer server.Close()
	var capture lockedLog
	previous := log.Writer()
	log.SetOutput(&capture)
	defer log.SetOutput(previous)
	ctx, cancel := context.WithTimeout(context.Background(), 200*time.Millisecond)
	defer cancel()
	_, err := sdk.GetHeightContext(ctx, &sdk.RPCConfig{SeedRPCServerAddr: nkngomobile.NewStringArray(server.URL), RPCTimeout: 100, RPCConcurrency: 1})
	if err == nil || capture.contains(marker) == patched() {
		t.Fatal("actual upstream raw-body log path was not observed")
	}
}

func TestRPCResponseBudget(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"result": 42, "padding": strings.Repeat("x", 2*1024*1024)})
	}))
	defer server.Close()
	// This large-body fixture measures allocation acceptance, not a 100ms
	// latency requirement. Product/session leases are not changed by this test.
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	height, err := sdk.GetHeightContext(ctx, &sdk.RPCConfig{SeedRPCServerAddr: nkngomobile.NewStringArray(server.URL), RPCTimeout: 500, RPCConcurrency: 1})
	if patched() {
		if err == nil {
			t.Fatal("patched fixed RPC byte budget was not enforced")
		}
		return
	}
	if err != nil || height != 42 {
		t.Fatal("actual SDK did not consume the bounded oversized synthetic RPC response")
	}
	// The SDK has its own RPC budget, not RustSimHub's 2048B stdio contract.
	// Adapter response limiting must happen before allocation/logging upstream.
}
