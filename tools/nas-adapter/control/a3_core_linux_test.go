// SPDX-License-Identifier: AGPL-3.0-only
package control

import (
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"testing"
	"time"
)

type a3LostReply struct {
	next http.RoundTripper
	lose bool
}

func (r *a3LostReply) RoundTrip(req *http.Request) (*http.Response, error) {
	response, err := r.next.RoundTrip(req)
	if err == nil && r.lose {
		r.lose = false
		_, _ = io.Copy(io.Discard, response.Body)
		response.Body.Close()
		return nil, errors.New("synthetic loss after original Core reply")
	}
	return response, err
}

func TestA3ActualEdgeCoreAuthControlRevocationAndOriginalOperation(t *testing.T) {
	root := os.Getenv("RSH_A3_NAS_CORE")
	if root == "" {
		t.Skip("explicit isolated original Core and Edge required")
	}
	var endpoint struct {
		Origin string `json:"origin"`
	}
	var credentials struct {
		First, Second, Nas string
		Line               uint64
	}
	load := func(name string, out any) {
		raw, e := os.ReadFile(filepath.Join(root, name))
		if e != nil || json.Unmarshal(raw, out) != nil {
			t.Fatal("missing original fixture input", name, e)
		}
	}
	load("edge.json", &endpoint)
	load("credentials.json", &credentials)
	ca, e := os.ReadFile(filepath.Join(root, "tls/cert.pem"))
	if e != nil {
		t.Fatal(e)
	}
	newClient := func(token string) *Client {
		c, e := New(endpoint.Origin, token, ca)
		if e != nil {
			t.Fatal(e)
		}
		t.Cleanup(c.Close)
		return c
	}
	first, second, nas := newClient(credentials.First), newClient(credentials.Second), newClient(credentials.Nas)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	prepared, e := first.Prepare(ctx, "a3-prepare", credentials.Line, "a3-nas-original", "dial", nil, "+12025550123")
	if e != nil {
		t.Fatal("actual Edge/Core must accept explicitly approved normal client", e)
	}
	repeated, e := first.Prepare(ctx, "a3-prepare-repeat", credentials.Line, "a3-nas-original", "dial", nil, "+12025550123")
	if e != nil || repeated.Operation.ID != prepared.Operation.ID {
		t.Fatal("original operation not retained", e)
	}
	if _, e = nas.Prepare(ctx, "a3-nas-denied", credentials.Line, "a3-nas-cannot-control", "dial", nil, "+12025550123"); e == nil {
		t.Fatal("read-only NAS escalated control")
	}
	lost := &a3LostReply{next: first.http.Transport, lose: true}
	first.http.Transport = lost
	if _, e = first.Submit(ctx, "a3-lost-reply", prepared.Operation.ID, nil, false); e == nil {
		t.Fatal("lost original reply reported success")
	}
	var failure *Failure
	if !errors.As(e, &failure) || !failure.Unknown || failure.RecoveryID != prepared.Operation.ID {
		t.Fatal("missing original recovery identity", e)
	}
	recovered, e := first.Operation(ctx, "a3-query-original", prepared.Operation.ID)
	if e != nil || recovered.Operation.ID != prepared.Operation.ID || recovered.Operation.CallID == nil || recovered.Lease == nil {
		t.Fatal("original Core operation query failed", e)
	}
	if _, e = first.Submit(ctx, "a3-same-submit", prepared.Operation.ID, nil, false); e != nil {
		t.Fatal("original committed operation could not be queried by repeat", e)
	}
	competitor, e := second.Prepare(ctx, "a3-competitor", credentials.Line, "a3-foreign-tone", "dtmf", recovered.Operation.CallID, "5")
	if e == nil {
		result, err := second.Submit(ctx, "a3-competitor-submit", competitor.Operation.ID, recovered.Lease, false)
		if err == nil && result.Operation.State == "submitted" {
			t.Fatal("another client controlled first client's call")
		}
	}
	if e = os.WriteFile(filepath.Join(root, "revoke"), []byte("1"), 0600); e != nil {
		t.Fatal(e)
	}
	deadline := time.Now().Add(3 * time.Second)
	for {
		if _, e = os.Stat(filepath.Join(root, "revoked")); e == nil {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("actual Auth was not revoked")
		}
		time.Sleep(10 * time.Millisecond)
	}
	if _, e = first.Operation(ctx, "a3-revoked-query", prepared.Operation.ID); e == nil {
		t.Fatal("revoked original token remains valid")
	}
	if e = os.WriteFile(filepath.Join(root, "done"), []byte("1"), 0600); e != nil {
		t.Fatal(e)
	}
	t.Log("actual Go client -> original Edge TLS/headers/Unix -> original Core Auth/call actor; original-ID recovery, competing control and revocation verified")
}
