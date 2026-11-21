// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

func signedFixture(t *testing.T) usagetx.Request {
	t.Helper()
	raw, e := os.ReadFile("../../nkn-build/usage-vectors/vectors.json")
	if e != nil {
		t.Fatal(e)
	}
	var bundle struct {
		Vectors []struct {
			Intent      usagetx.Intent `json:"intent"`
			Nonce       uint64         `json:"nonce"`
			Transaction string         `json:"transaction_hex"`
			Digest      string         `json:"digest_hex"`
		} `json:"vectors"`
	}
	if json.Unmarshal(raw, &bundle) != nil || len(bundle.Vectors) != 15 || bundle.Vectors[0].Nonce != 0 {
		t.Fatal("missing fixed SDK fixture")
	}
	v := bundle.Vectors[0]
	return usagetx.Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: v.Intent.PublicKey, Intent: &v.Intent, Digest: v.Digest, Transaction: v.Transaction}
}
func preflight(r usagetx.Request) usagetx.Request {
	r.Intent = nil
	r.Digest = ""
	r.Transaction = ""
	return r
}
func query(r usagetx.Request) usagetx.Request { r.Transaction = ""; return r }

// Uses the real scoped dialer, real original Lease.Check/boot clock, and strict
// TLS verification of a synthetic local CA. Only the private launch fixture is
// synthetic; no production endpoint or Root registration is claimed here.
func transactionTLS(t *testing.T, handler http.HandlerFunc) (*usageTransactions, context.Context) {
	t.Helper()
	s := httptest.NewTLSServer(handler)
	t.Cleanup(s.Close)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	t.Cleanup(cancel)
	leaseCtx, leaseCancel := context.WithCancel(context.Background())
	t.Cleanup(leaseCancel)
	now, e := bootMS()
	if e != nil {
		t.Fatal(e)
	}
	r := signedFixture(t)
	key, _ := hex.DecodeString(r.PublicKey)
	l := &Lease{launch: &launch{generation: 9, seeds: []string{s.URL}, fixture: true}, ctx: leaseCtx, cancel: leaseCancel, deadlineBoot: now + 10000, lastBoot: now}
	copy(l.launch.statisticsKey[:], key)
	d := &scopedDialer{lease: l, startDeadline: time.Now().Add(5 * time.Second), connections: make(map[*scopedConn]struct{})}
	t.Cleanup(d.close)
	client := newUsageHTTP(d)
	client.Transport.(*http.Transport).TLSClientConfig = s.Client().Transport.(*http.Transport).TLSClientConfig.Clone()
	return &usageTransactions{lease: l, client: client, address: "synthetic-address-from-public-key"}, ctx
}

func rpcMethod(t *testing.T, r *http.Request) (string, map[string]string) {
	t.Helper()
	var request struct {
		ID     string            `json:"id"`
		Method string            `json:"method"`
		Params map[string]string `json:"params"`
	}
	d := json.NewDecoder(io.LimitReader(r.Body, 2049))
	d.DisallowUnknownFields()
	if r.Method != "POST" || r.Header.Get("Content-Type") != "application/json" || d.Decode(&request) != nil || request.ID != transactionRPCID || len(request.Params) != 1 {
		t.Error("unexpected outbound RPC shape")
	}
	return request.Method, request.Params
}
func reply(w http.ResponseWriter, result string) {
	fmt.Fprintf(w, `{"jsonrpc":"2.0","id":%q,"result":%s}`, transactionRPCID, result)
}

func TestTransactionSingleSubmitAndNoNonceRenewal(t *testing.T) {
	for _, lostAck := range []bool{false, true} {
		t.Run(fmt.Sprint(lostAck), func(t *testing.T) {
			var nonceCalls, submits, queries atomic.Int32
			r := signedFixture(t)
			c, ctx := transactionTLS(t, func(w http.ResponseWriter, request *http.Request) {
				method, p := rpcMethod(t, request)
				switch method {
				case "getnoncebyaddr":
					nonceCalls.Add(1)
					if p["address"] != "synthetic-address-from-public-key" {
						t.Error("wrong address")
					}
					reply(w, `{"nonce":0,"nonceInTxPool":0}`)
				case "sendrawtransaction":
					submits.Add(1)
					if p["tx"] != r.Transaction {
						t.Error("wrong raw tx")
					}
					if lostAck {
						w.WriteHeader(502)
					} else {
						reply(w, fmt.Sprintf("%q", r.Digest))
					}
				case "gettransaction":
					queries.Add(1)
					if p["hash"] != r.Digest {
						t.Error("wrong query hash")
					}
					reply(w, `{"height":12345,"confirmations":999999,"hash":"pretend-confirmed"}`)
				default:
					t.Error("unapproved method", method)
				}
			})
			p, e := c.execute(ctx, usagetx.Preflight, preflight(r))
			if e != nil || p.Nonce == nil || *p.Nonce != 0 {
				t.Fatal("nonce failed", e)
			}
			deadline := c.reserved.deadline
			if _, e = c.execute(ctx, usagetx.Preflight, preflight(r)); e != nil || c.reserved.deadline != deadline || nonceCalls.Load() != 1 {
				t.Fatal("preflight replay made RPC or renewed deadline")
			}
			want := "accepted"
			if lostAck {
				want = "unknown"
			}
			for range 2 {
				result, e := c.execute(ctx, usagetx.Submit, r)
				if e != nil || result.State != want {
					t.Fatal("wrong submission result", result, e)
				}
			}
			if submits.Load() != 1 {
				t.Fatal("duplicate submit")
			}
			if _, e = c.execute(ctx, usagetx.Preflight, preflight(r)); e == nil || nonceCalls.Load() != 1 {
				t.Fatal("consumed nonce reallocated")
			}
			q := query(r)
			q.OperationID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
			q.ParticipationGeneration = 2
			result, e := c.execute(ctx, usagetx.Query, q)
			if e != nil || result.State != "unknown" || queries.Load() != 1 || !c.reserved.consumed {
				t.Fatal("untrusted query confirmed or cleared pending")
			}
			bad := q
			bad.Digest = strings.Repeat("0", 64)
			if _, e = c.execute(ctx, usagetx.Query, bad); e == nil || queries.Load() != 1 {
				t.Fatal("different pending digest queried")
			}
		})
	}
}

func TestTransactionInvalidBindingAndBytesMakeNoRPC(t *testing.T) {
	var calls atomic.Int32
	c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) { calls.Add(1); reply(w, `{"nonce":0,"nonceInTxPool":0}`) })
	r := signedFixture(t)
	if _, e := c.execute(ctx, usagetx.Submit, r); e == nil || calls.Load() != 0 {
		t.Fatal("no preflight accepted")
	}
	if _, e := c.execute(ctx, usagetx.Preflight, preflight(r)); e != nil {
		t.Fatal(e)
	}
	for name, edit := range map[string]func(*usagetx.Request){
		"operation":  func(r *usagetx.Request) { r.OperationID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee" },
		"generation": func(r *usagetx.Request) { r.ParticipationGeneration++ },
		"key":        func(r *usagetx.Request) { r.PublicKey = strings.Repeat("1", 64) },
		"digest":     func(r *usagetx.Request) { r.Digest = strings.Repeat("0", 64) },
		"signature": func(r *usagetx.Request) {
			b, _ := hex.DecodeString(r.Transaction)
			b[len(b)-1] ^= 1
			r.Transaction = hex.EncodeToString(b)
		},
		"extra-fee-field": func(r *usagetx.Request) { r.Transaction += "1801" },
	} {
		t.Run(name, func(t *testing.T) {
			bad := r
			edit(&bad)
			if _, e := c.execute(ctx, usagetx.Submit, bad); e == nil || calls.Load() != 1 || c.reserved.consumed {
				t.Fatal("invalid request reached RPC or consumed reservation")
			}
		})
	}
	other := preflight(r)
	other.OperationID = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
	if _, e := c.execute(ctx, usagetx.Preflight, other); e == nil || calls.Load() != 1 {
		t.Fatal("outstanding reservation replaced")
	}
	c.reserved.deadline = time.Now().Add(-time.Millisecond)
	if _, e := c.execute(ctx, usagetx.Submit, r); e == nil || calls.Load() != 1 {
		t.Fatal("expired reservation submitted")
	}
}

func TestTransactionNonceParserRejectsUnknownAndNeverInventsZero(t *testing.T) {
	for _, result := range []string{`{}`, `null`, `0`, `{"nonce":0}`, `{"nonce":0,"nonceInTxPool":null}`, `{"nonce":-1,"nonceInTxPool":0}`, `{"nonce":0.5,"nonceInTxPool":0}`, `{"nonce":18446744073709551616,"nonceInTxPool":0}`, `{"nonce":0,"nonce":1,"nonceInTxPool":0}`, `{"nonce":0,"nonceInTxPool":0,"address":"other"}`, `{"nonce":0,"nonceInTxPool":0,"currentHeight":-1}`, `{"nonce":0,"nonceInTxPool":0,"currentHeight":4294967296}`, `{"nonce":0,"nonceInTxPool":0,"currentHeight":"10027449"}`} {
		t.Run(result, func(t *testing.T) {
			var calls atomic.Int32
			c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) { calls.Add(1); reply(w, result) })
			if _, e := c.execute(ctx, usagetx.Preflight, preflight(signedFixture(t))); e == nil || calls.Load() != 1 || c.reserved != nil {
				t.Fatal("invalid nonce accepted")
			}
		})
	}
	for _, result := range []string{`{"nonce":0,"nonceInTxPool":128}`, `{"nonce":128,"nonceInTxPool":0}`, `{"currentHeight":10027449,"nonce":128,"nonceInTxPool":128}`, `{"nonce":18446744073709551615,"nonceInTxPool":0}`} {
		t.Run(result, func(t *testing.T) {
			c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) { reply(w, result) })
			v, e := c.execute(ctx, usagetx.Preflight, preflight(signedFixture(t)))
			want := uint64(128)
			if strings.Contains(result, "184467") {
				want = ^uint64(0)
			}
			if e != nil || v.Nonce == nil || *v.Nonce != want {
				t.Fatal("nonce truncated", v, e)
			}
		})
	}
}

func TestTransactionHTTPBoundsTLSAndEnvelope(t *testing.T) {
	for _, kind := range []string{"redirect", "http-error", "large-length", "large-chunked", "wrong-id", "duplicate-id", "error-null", "trailing", "wrong-ca", "no-deadline", "cancelled"} {
		t.Run(kind, func(t *testing.T) {
			var calls atomic.Int32
			c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) {
				calls.Add(1)
				switch kind {
				case "redirect":
					http.Redirect(w, r, "/other", http.StatusTemporaryRedirect)
				case "http-error":
					w.WriteHeader(503)
				case "large-length":
					w.Header().Set("Content-Length", "2049")
					io.WriteString(w, strings.Repeat(" ", 2049))
				case "large-chunked":
					w.(http.Flusher).Flush()
					io.WriteString(w, strings.Repeat(" ", 2049))
				case "wrong-id":
					io.WriteString(w, `{"id":"wrong","result":{"nonce":0,"nonceInTxPool":0}}`)
				case "duplicate-id":
					fmt.Fprintf(w, `{"id":%q,"id":%q,"result":0}`, transactionRPCID, transactionRPCID)
				case "error-null":
					fmt.Fprintf(w, `{"id":%q,"error":null,"result":0}`, transactionRPCID)
				case "trailing":
					reply(w, `{"nonce":0,"nonceInTxPool":0}`)
					io.WriteString(w, ` {}`)
				default:
					reply(w, `{"nonce":0,"nonceInTxPool":0}`)
				}
			})
			if kind == "wrong-ca" {
				c.client.Transport.(*http.Transport).TLSClientConfig = nil
			}
			if kind == "no-deadline" {
				ctx = context.Background()
			}
			if kind == "cancelled" {
				cancelled, cancel := context.WithCancel(ctx)
				cancel()
				ctx = cancelled
			}
			if _, e := c.execute(ctx, usagetx.Preflight, preflight(signedFixture(t))); e == nil || c.reserved != nil || calls.Load() > 1 {
				t.Fatal("unbounded/insecure/retried RPC")
			}
			if (kind == "wrong-ca" || kind == "no-deadline" || kind == "cancelled") && calls.Load() != 0 {
				t.Fatal("rejected request reached application")
			}
		})
	}
}

func TestTransactionLeaseCancellationDeadlineAndBusy(t *testing.T) {
	for _, kind := range []string{"lease-cancel", "deadline", "late-accepted"} {
		t.Run(kind, func(t *testing.T) {
			started := make(chan struct{})
			release := make(chan struct{})
			var calls atomic.Int32
			r := signedFixture(t)
			c, ctx := transactionTLS(t, func(w http.ResponseWriter, request *http.Request) {
				method, _ := rpcMethod(t, request)
				calls.Add(1)
				if method == "getnoncebyaddr" {
					reply(w, `{"nonce":0,"nonceInTxPool":0}`)
					return
				}
				close(started)
				select {
				case <-release:
					reply(w, fmt.Sprintf("%q", r.Digest))
				case <-request.Context().Done():
				}
			})
			if _, e := c.execute(ctx, usagetx.Preflight, preflight(r)); e != nil {
				t.Fatal(e)
			}
			short, cancel := context.WithTimeout(ctx, 100*time.Millisecond)
			defer cancel()
			done := make(chan transactionReply, 1)
			go func() { v, _ := c.execute(short, usagetx.Submit, r); done <- v }()
			select {
			case <-started:
			case <-time.After(time.Second):
				t.Fatal("submit did not start")
			}
			if _, e := c.execute(ctx, usagetx.Preflight, preflight(r)); e == nil {
				t.Fatal("concurrent operation accepted")
			}
			if kind != "deadline" {
				c.lease.cancel()
			}
			if kind == "late-accepted" {
				close(release)
			}
			select {
			case v := <-done:
				if v.State != "unknown" {
					t.Fatal("revoked or expired submission accepted", v)
				}
			case <-time.After(time.Second):
				t.Fatal("IO did not cancel")
			}
			if kind != "late-accepted" {
				close(release)
			}
			if calls.Load() != 2 || !c.reserved.consumed {
				t.Fatal("consumed nonce lost or repeated")
			}
		})
	}
}

func TestTransactionRestartQueryCannotSubmitOrConfirm(t *testing.T) {
	var calls atomic.Int32
	c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) {
		method, _ := rpcMethod(t, r)
		calls.Add(1)
		if method != "gettransaction" {
			t.Error("restart queried nonce or submitted")
		}
		reply(w, `{"height":1000000,"confirmations":1000000}`)
	})
	r := signedFixture(t)
	value, e := c.execute(ctx, usagetx.Query, query(r))
	if e != nil || value.State != "unknown" || calls.Load() != 1 || c.reserved != nil {
		t.Fatal("restart invented confirmation or nonce")
	}
	if _, e := c.execute(ctx, usagetx.Submit, r); e == nil || calls.Load() != 1 {
		t.Fatal("restart resubmitted without original reservation")
	}
}

func TestTransactionNoSeedFallbackOrUnboundedQuery(t *testing.T) {
	var first, second atomic.Int32
	fallback := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { second.Add(1) }))
	defer fallback.Close()
	c, ctx := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) {
		first.Add(1)
		method, _ := rpcMethod(t, r)
		if method == "gettransaction" {
			w.(http.Flusher).Flush()
			io.WriteString(w, strings.Repeat(" ", 16*1024+1))
		} else {
			w.WriteHeader(503)
		}
	})
	c.lease.launch.seeds = append(c.lease.launch.seeds, fallback.URL)
	r := signedFixture(t)
	if _, err := c.execute(ctx, usagetx.Preflight, preflight(r)); err == nil || first.Load() != 1 || second.Load() != 0 {
		t.Fatal("failed first seed silently fell back")
	}
	value, err := c.execute(ctx, usagetx.Query, query(r))
	if err != nil || value.State != "unknown" || first.Load() != 2 || second.Load() != 0 {
		t.Fatal("oversized query confirmed or retried")
	}
	if _, err := c.rpc(ctx, "getsubscribers", map[string]any{"topic": "other"}, 2048); err == nil || first.Load() != 2 {
		t.Fatal("unapproved RPC method reached transport")
	}
}
