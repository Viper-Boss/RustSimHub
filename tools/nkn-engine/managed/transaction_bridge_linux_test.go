// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"sync/atomic"
	"testing"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

// This private test host exercises the REAL scoped/TLS/RPC backend through the
// actual owned bridge. It does not stand in for SDK startup or kernel Root proof.
type transactionRPCHost struct{ port *usageTransactions }

func (*transactionRPCHost) SDKLinked() bool { return true }
func (*transactionRPCHost) Read(context.Context, uint32) (bridge.Status, error) {
	return bridge.Status{}, ErrLaunch
}
func (h *transactionRPCHost) Transaction(ctx context.Context, op usagetx.Operation, r usagetx.Request) (usagetx.Reply, error) {
	v, e := h.port.execute(ctx, op, r)
	if e != nil {
		return usagetx.Reply{}, e
	}
	state := v.State
	if op == usagetx.Preflight {
		state = "ready"
	}
	return usagetx.NewReply(op, r, state, v.Nonce)
}

func TestOwnedBridgeActuallyRunsTLSOnceAndPreservesNonceBudget(t *testing.T) {
	var nonces, submits, queries atomic.Int32
	r := signedFixture(t)
	c, ctx := transactionTLS(t, func(w http.ResponseWriter, request *http.Request) {
		method, _ := rpcMethod(t, request)
		switch method {
		case "getnoncebyaddr":
			nonces.Add(1)
			reply(w, `{"nonce":0,"nonceInTxPool":0}`)
		case "sendrawtransaction":
			submits.Add(1)
			w.WriteHeader(502)
		case "gettransaction":
			queries.Add(1)
			reply(w, `{"height":123456,"confirmations":999999}`)
		default:
			t.Error("unapproved RPC", method)
		}
	})
	inputR, inputW, e := os.Pipe()
	if e != nil {
		t.Fatal(e)
	}
	outputR, outputW, e := os.Pipe()
	if e != nil {
		t.Fatal(e)
	}
	done := make(chan error, 1)
	go func() {
		done <- bridge.ServeOwnedTransactions(ctx, inputR, outputW, nil, nil, &transactionRPCHost{port: c})
	}()
	t.Cleanup(func() {
		inputW.Close()
		inputR.Close()
		outputR.Close()
		outputW.Close()
		select {
		case <-done:
		case <-time.After(time.Second):
			t.Error("private bridge leaked")
		}
	})
	call := func(op usagetx.Operation, tx usagetx.Request) usagetx.Reply {
		t.Helper()
		name := protocol.UsageNonce
		if op == usagetx.Submit {
			name = protocol.UsageSubmit
		} else if op == usagetx.Query {
			name = protocol.UsageQuery
		}
		id := "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
		raw, _ := json.Marshal(protocol.Request{Protocol: 1, ID: id, Op: name, Transaction: &tx})
		var size [4]byte
		binary.BigEndian.PutUint32(size[:], uint32(len(raw)))
		inputW.SetWriteDeadline(time.Now().Add(time.Second))
		outputR.SetReadDeadline(time.Now().Add(2 * time.Second))
		if _, e := inputW.Write(append(size[:], raw...)); e != nil {
			t.Fatal(e)
		}
		if _, e := io.ReadFull(outputR, size[:]); e != nil {
			t.Fatal(e)
		}
		length := binary.BigEndian.Uint32(size[:])
		if length == 0 || length > bridge.MaxResponse {
			t.Fatal("bad reply size")
		}
		value := make([]byte, length)
		if _, e := io.ReadFull(outputR, value); e != nil {
			t.Fatal(e)
		}
		fields, e := usagetx.Fields(value, bridge.MaxResponse)
		if e != nil || len(fields) != 3 {
			t.Fatal("wrong actual bridge reply")
		}
		var actualID string
		json.Unmarshal(fields["id"], &actualID)
		if actualID != id {
			t.Fatal("reply ID changed")
		}
		v, e := usagetx.ParseReply(op, tx, fields["transaction"])
		if e != nil {
			t.Fatal("unbound reply", e, string(value))
		}
		return v
	}
	if value := call(usagetx.Preflight, preflight(r)); value.Status != "ready" || value.Nonce == nil || *value.Nonce != 0 {
		t.Fatal("nonce not real zero")
	}
	deadline := c.reserved.deadline
	if time.Until(deadline) < 2*time.Second {
		t.Fatal("per-RPC timeout replaced original nonce reservation budget")
	}
	for range 2 {
		if value := call(usagetx.Submit, r); value.Status != "unknown" {
			t.Fatal("lost ack invented acceptance")
		}
	}
	if value := call(usagetx.Query, query(r)); value.Status != "unknown" {
		t.Fatal("node height invented confirmation")
	}
	if nonces.Load() != 1 || submits.Load() != 1 || queries.Load() != 1 || c.reserved.deadline != deadline || !c.reserved.consumed {
		t.Fatal("framing renewed budget, lost pending or repeated IO")
	}
}
