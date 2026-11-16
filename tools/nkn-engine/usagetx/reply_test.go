// SPDX-License-Identifier: AGPL-3.0-only
package usagetx

import (
	"bytes"
	"encoding/json"
	"testing"
)

func TestBoundReplyRejectsConfirmedAndCrossIntent(t *testing.T) {
	v := fixedVectors(t)[0]
	r := Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 7, PublicKey: v.Intent.PublicKey, Intent: &v.Intent, Digest: v.Digest, Transaction: v.Transaction}
	if _, e := VerifyCanonicalSubmission(r); e != nil {
		t.Fatal("Root canonical verifier refused original SDK", e)
	}
	for _, op := range []Operation{Preflight, Submit, Query} {
		request := r
		state := "unknown"
		var nonce *uint64
		if op == Preflight {
			request.Intent = nil
			request.Digest = ""
			request.Transaction = ""
			state = "ready"
			var zero uint64
			nonce = &zero
		} else if op == Query {
			request.Transaction = ""
		} else {
			state = "accepted"
		}
		reply, e := NewReply(op, request, state, nonce)
		if e != nil {
			t.Fatal(e)
		}
		raw, _ := json.Marshal(reply)
		if _, e := ParseReply(op, request, raw); e != nil {
			t.Fatal("valid bounded reply refused", e)
		}
		for _, bad := range [][]byte{
			bytes.Replace(raw, []byte(`"version":1`), []byte(`"version":1,"version":1`), 1),
			bytes.Replace(raw, []byte(`"participation_generation":7`), []byte(`"participation_generation":8`), 1),
			bytes.Replace(raw, []byte(`"status":"`+state+`"`), []byte(`"status":"confirmed"`), 1),
			append(raw, []byte(" {}")...),
		} {
			if _, e := ParseReply(op, request, bad); e == nil {
				t.Fatal("unbound or invented confirmation accepted")
			}
		}
		if op == Preflight {
			if _, e := ParseReply(op, request, bytes.Replace(raw, []byte(`"nonce":0`), []byte(`"nonce":null`), 1)); e == nil {
				t.Fatal("null nonce became zero")
			}
		}
		if op != Preflight {
			bad := request
			in := *bad.Intent
			in.RenewalPeriod++
			bad.Intent = &in
			if _, e := ParseReply(op, bad, raw); e == nil {
				t.Fatal("cross-intent reply accepted")
			}
		}
	}
	for _, v := range fixedVectors(t) {
		r.Intent = &v.Intent
		r.PublicKey = v.Intent.PublicKey
		r.Transaction = v.Transaction
		r.Digest = v.Digest
		if _, e := VerifyCanonicalSubmission(r); e != nil {
			t.Fatal("nonce width or canonical payload rejected", v.Nonce, e)
		}
		r.Transaction += "1801"
		if _, e := VerifyCanonicalSubmission(r); e == nil {
			t.Fatal("Root canonical verifier accepted extra field")
		}
	}
}
