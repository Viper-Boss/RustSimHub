// SPDX-License-Identifier: AGPL-3.0-only
package control

import (
	"context"
	"crypto/x509"
	"encoding/json"
	"encoding/pem"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"
)

const operationID = "11111111-1111-4111-8111-111111111111"
const callID = "22222222-2222-4222-8222-222222222222"

func fixture(t *testing.T, handler http.HandlerFunc) (*Client, *httptest.Server) {
	t.Helper()
	s := httptest.NewTLSServer(handler)
	t.Cleanup(s.Close)
	ca := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: s.Certificate().Raw})
	c, e := New(s.URL, strings.Repeat("a", 64), ca)
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(c.Close)
	return c, s
}
func receipt(state string, lease bool) map[string]any {
	var l any
	var call any
	if lease {
		call = callID
		l = Lease{callID, strings.Repeat("b", 64), 1, 2, 30}
	}
	return map[string]any{"operation": map[string]any{"id": operationID, "call_id": call, "line": 1, "state": state}, "lease": l}
}
func reply(w http.ResponseWriter, r *http.Request, result any) {
	var request struct {
		ID string `json:"id"`
	}
	json.NewDecoder(r.Body).Decode(&request)
	json.NewEncoder(w).Encode(map[string]any{"protocol": 2, "id": request.ID, "result": result})
}
func TestFiniteCommandsMatchCommittedActualWireVectors(t *testing.T) {
	var captured []json.RawMessage
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != "POST" || r.URL.Path != "/api/v2" || r.Header.Get("Idempotency-Key") != "" {
			t.Error("unexpected transport")
		}
		var req struct {
			Protocol int                        `json:"protocol"`
			ID       string                     `json:"id"`
			Command  map[string]json.RawMessage `json:"command"`
		}
		raw, _ := io.ReadAll(io.LimitReader(r.Body, MaxReply+1))
		if StrictJSON(raw, &req) != nil {
			t.Error("invalid request")
		}
		captured = append(captured, json.RawMessage(raw))
		var op string
		json.Unmarshal(req.Command["op"], &op)
		var result any = receipt("prepared", false)
		switch op {
		case "submit_call":
			result = receipt("unknown", true)
		case "call_operation":
			result = receipt("unknown", true)
		case "cancel_prepared_call":
			result = receipt("cancelled", false)
		case "renew_call":
			result = map[string]any{"lease": Lease{callID, strings.Repeat("b", 64), 1, 2, 30}}
		}
		json.NewEncoder(w).Encode(map[string]any{"protocol": 2, "id": req.ID, "result": result})
	})
	ctx := context.Background()
	p, e := c.Prepare(ctx, "prepare", 1, "synthetic-attempt-1", "dial", nil, "10010")
	if e != nil || p.Operation.ID != operationID {
		t.Fatal(e)
	}
	r, e := c.Submit(ctx, "submit", operationID, nil, false)
	if e != nil || r.Operation.State != "unknown" {
		t.Fatal(e)
	}
	if _, e = c.Operation(ctx, "query", operationID); e != nil {
		t.Fatal(e)
	}
	if _, e = c.Cancel(ctx, "cancel", operationID); e != nil {
		t.Fatal(e)
	}
	if _, e = c.Renew(ctx, "renew", *r.Lease); e != nil {
		t.Fatal(e)
	}
	data, e := os.ReadFile("request-vectors.json")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []json.RawMessage
	if StrictJSON(data, &vectors) != nil {
		t.Fatal("vectors")
	}
	if len(vectors) != len(captured) {
		t.Fatal("vector count")
	}
	for i := range vectors {
		var a, b any
		json.Unmarshal(vectors[i], &a)
		json.Unmarshal(captured[i], &b)
		if !reflect.DeepEqual(a, b) {
			t.Fatalf("wire vector %d mismatch", i)
		}
	}
}
func TestCoreDenialAndFiniteCommandsNeverUpgradeSession(t *testing.T) {
	var count atomic.Int32
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) {
		count.Add(1)
		reply(w, r, map[string]any{"ok": false, "error": "denied"})
	})
	_, e := c.Prepare(context.Background(), "p", 1, "one", "dial", nil, "10010")
	var failure *Failure
	if !errors.As(e, &failure) || failure.Kind != "core_denied" || count.Load() != 1 {
		t.Fatal("denial must be returned without retry")
	}
	if !failure.Unknown || failure.RecoveryID != "one" {
		t.Fatal("generic Core error cannot prove no side effect")
	}
	for _, raw := range []string{`{"op":"issue_media","id":"p"}`, `{"op":"pair_proof","id":"p"}`, `{"op":"submit_call","id":"p","operation_id":"` + operationID + `","token":"legacy"}`} {
		if _, e := Run(context.Background(), c, []byte(raw)); e == nil {
			t.Fatal("unapproved operation")
		}
	}
	if count.Load() != 1 {
		t.Fatal("bad operation reached transport")
	}
}
func TestDropAfterMutationPreservesUnknownAndDoesNotRetry(t *testing.T) {
	var count atomic.Int32
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) {
		count.Add(1)
		conn, _, _ := w.(http.Hijacker).Hijack()
		conn.Close()
	})
	_, e := c.Submit(context.Background(), "one", operationID, nil, false)
	var f *Failure
	if !errors.As(e, &f) || !f.Unknown || f.RecoveryID != operationID || count.Load() != 1 {
		t.Fatal("lost unknown result")
	}
}
func TestBadRepliesNeverPublishPartialReceipt(t *testing.T) {
	bodies := []string{
		`{"protocol":2,"id":"wrong","result":{}}`,
		`{"protocol":3,"id":"query","result":{}}`,
		`{"protocol":2,"id":"query","id":"query","result":{}}`,
		`{"protocol":2,"id":"query","result":{"operation":{"id":"` + operationID + `","line":1,"state":"submitted","unexpected":true},"lease":null}}`,
		`{"protocol":2,"id":"query","result":{"operation":{"id":"33333333-3333-4333-8333-333333333333","call_id":null,"line":1,"state":"submitted"},"lease":null}}`,
		`{"protocol":2,"id":"query","result":{"operation":{"id":"` + operationID + `","call_id":null,"line":1,"state":"success"},"lease":null}}`,
		strings.Repeat("x", MaxReply+1),
	}
	for i, body := range bodies {
		t.Run(string(rune('a'+i)), func(t *testing.T) {
			c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) { w.Write([]byte(body)) })
			r, e := c.Operation(context.Background(), "query", operationID)
			if e == nil || r != nil {
				t.Fatal("partial or mismatched receipt")
			}
		})
	}
}
func TestWrongLeaseBootGenerationAndCallAreRejected(t *testing.T) {
	for _, field := range []string{"boot", "generation", "call"} {
		t.Run(field, func(t *testing.T) {
			lease := Lease{callID, strings.Repeat("b", 64), 1, 2, 30}
			bad := lease
			switch field {
			case "boot":
				bad.Boot = strings.Repeat("c", 64)
			case "generation":
				bad.Generation++
			case "call":
				bad.CallID = operationID
			}
			c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) { reply(w, r, map[string]any{"lease": bad}) })
			if r, e := c.Renew(context.Background(), "renew", lease); e == nil || r != nil {
				t.Fatal("mismatched lease")
			}
		})
	}
}
func TestTLSWrongCAAndRedirectDoNotReachDestination(t *testing.T) {
	var reached atomic.Int32
	_, destination := fixture(t, func(w http.ResponseWriter, r *http.Request) { reached.Add(1) })
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, destination.URL+"/api/v2", 307) })
	if _, e := c.Operation(context.Background(), "q", operationID); e == nil {
		t.Fatal("redirect accepted")
	}
	if reached.Load() != 0 {
		t.Fatal("redirect replay")
	}
	// Trust the first server only while connecting to a different certificate.
	c.origin = destination.URL
	c.http.Transport.(*http.Transport).TLSClientConfig.RootCAs = x509.NewCertPool()
	if _, e := c.Operation(context.Background(), "q", operationID); e == nil {
		t.Fatal("untrusted CA accepted")
	}
	if reached.Load() != 0 {
		t.Fatal("TLS pin failed")
	}
}
func TestInvalidCommandsRejectedBeforeNetwork(t *testing.T) {
	var count atomic.Int32
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) { count.Add(1) })
	for _, v := range []struct {
		line    uint64
		action  string
		call    *string
		payload string
	}{{0, "dial", nil, "10010"}, {1, "shell", nil, "10010"}, {1, "dial", nil, "10010\n"}, {1, "hangup", nil, ""}, {1, "dial", &callIDCopy, "10010"}, {1, "dtmf", &callIDCopy, "x"}} {
		if _, e := c.Prepare(context.Background(), "p", v.line, "one", v.action, v.call, v.payload); e == nil {
			t.Fatal("bad prepare")
		}
	}
	if _, e := c.Submit(context.Background(), "s", "bad-id", nil, false); e == nil {
		t.Fatal("bad ID")
	}
	if _, e := c.Renew(context.Background(), "r", Lease{}); e == nil {
		t.Fatal("bad lease")
	}
	if count.Load() != 0 {
		t.Fatal("invalid command sent")
	}
}

var callIDCopy = callID

func TestStrictJSONAndOriginConstraints(t *testing.T) {
	for _, s := range []string{`{"x":1,"x":2}`, `{"x":{"a":1,"a":2}}`, `{"x":NaN}`, `{} {}`, strings.Repeat("[", 40) + strings.Repeat("]", 40)} {
		var x any
		if StrictJSON([]byte(s), &x) == nil {
			t.Fatal("bad JSON accepted")
		}
	}
	for _, u := range []string{"http://localhost", "https://u:p@example.invalid", "https://example.invalid/path", "https://example.invalid?q=x"} {
		if _, e := New(u, strings.Repeat("a", 64), []byte("bad")); e == nil {
			t.Fatal("bad origin")
		}
	}
}
func TestCancelledContextAndHTTPFailureKeepMutationUncertain(t *testing.T) {
	c, _ := fixture(t, func(w http.ResponseWriter, r *http.Request) { http.Error(w, "never print private remote text", 503) })
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	_, e := c.Submit(ctx, "s", operationID, nil, false)
	var f *Failure
	if !errors.As(e, &f) || !f.Unknown {
		t.Fatal("cancel must not imply rejected")
	}
	_, e = c.Submit(context.Background(), "s", operationID, nil, false)
	if !errors.As(e, &f) || !f.Unknown || strings.Contains(e.Error(), "private") {
		t.Fatal("HTTP response leaked or certainty invented")
	}
}
func TestPrivateFilesAndFiniteCLI(t *testing.T) {
	dir := t.TempDir()
	os.Chmod(dir, 0700)
	c, s := fixture(t, func(w http.ResponseWriter, r *http.Request) { reply(w, r, receipt("prepared", false)) })
	_ = c
	ca := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: s.Certificate().Raw})
	caPath, tokenPath, configPath := filepath.Join(dir, "ca"), filepath.Join(dir, "token"), filepath.Join(dir, "config")
	os.WriteFile(caPath, ca, 0600)
	os.WriteFile(tokenPath, []byte(strings.Repeat("a", 64)), 0600)
	cfg, _ := json.Marshal(map[string]string{"url": s.URL, "ca_file": caPath, "token_file": tokenPath})
	os.WriteFile(configPath, cfg, 0600)
	client, e := Load(configPath)
	if e != nil {
		t.Fatal(e)
	}
	defer client.Close()
	raw := []byte(`{"op":"prepare_call","id":"prepare","line_epoch":1,"idempotency_key":"synthetic-attempt-1","action":"dial","call_id":null,"payload":"10010"}`)
	if _, e := Run(context.Background(), client, raw); e != nil {
		t.Fatal(e)
	}
	os.Chmod(tokenPath, 0644)
	if _, e = Load(configPath); e == nil {
		t.Fatal("public token accepted")
	}
	os.Chmod(tokenPath, 0600)
	os.Rename(tokenPath, tokenPath+"-real")
	os.Symlink(tokenPath+"-real", tokenPath)
	if _, e = Load(configPath); e == nil {
		t.Fatal("symlink token accepted")
	}
}
