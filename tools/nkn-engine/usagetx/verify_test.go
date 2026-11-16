// SPDX-License-Identifier: AGPL-3.0-only
package usagetx

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"strings"
	"testing"
)

func fixedVectors(t *testing.T) []struct {
	Intent      Intent `json:"intent"`
	Nonce       uint64 `json:"nonce"`
	Transaction string `json:"transaction_hex"`
	Digest      string `json:"digest_hex"`
} {
	t.Helper()
	raw, err := os.ReadFile("../../nkn-build/usage-vectors/vectors.json")
	if err != nil {
		t.Fatal(err)
	}
	var v struct {
		Vectors []struct {
			Intent      Intent `json:"intent"`
			Nonce       uint64 `json:"nonce"`
			Transaction string `json:"transaction_hex"`
			Digest      string `json:"digest_hex"`
		} `json:"vectors"`
	}
	if json.Unmarshal(raw, &v) != nil || len(v.Vectors) != 15 {
		t.Fatal("missing fixed SDK vectors")
	}
	return v.Vectors
}

func TestFixedSDKBytesAndActualSignature(t *testing.T) {
	for _, v := range fixedVectors(t) {
		t.Run(fmt.Sprintf("%s/%d", v.Intent.Window, v.Nonce), func(t *testing.T) {
			r := Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: v.Intent.PublicKey, Intent: &v.Intent, Digest: v.Digest, Transaction: v.Transaction}
			raw, err := VerifySubmission(r, v.Nonce)
			if err != nil || hex.EncodeToString(raw) != v.Transaction {
				t.Fatal("rejected original fixed SDK transaction", err)
			}
			encoded, _ := json.Marshal(r)
			if _, err := Parse(Submit, encoded); err != nil || len(encoded) > MaxRequestBytes {
				t.Fatal("wire request does not fit", err)
			}
			for name, mutate := range map[string]func(*Request){
				"digest": func(r *Request) { r.Digest = strings.Repeat("0", 64) },
				"signature": func(r *Request) {
					b, _ := hex.DecodeString(r.Transaction)
					b[len(b)-1] ^= 1
					r.Transaction = hex.EncodeToString(b)
				},
				"payload": func(r *Request) {
					r.Transaction = strings.Replace(r.Transaction, hex.EncodeToString([]byte("rustsimhub")), hex.EncodeToString([]byte("Rustsimhub")), 1)
				},
				"extra-field":      func(r *Request) { r.Transaction += "1801" },
				"different-intent": func(r *Request) { in := *r.Intent; in.RenewalPeriod++; r.Intent = &in },
			} {
				t.Run(name, func(t *testing.T) {
					changed := r
					mutate(&changed)
					if _, e := VerifySubmission(changed, v.Nonce); e == nil {
						t.Fatal("tampering accepted")
					}
				})
			}
			if _, e := VerifySubmission(r, v.Nonce^1); e == nil {
				t.Fatal("wrong preflight nonce accepted")
			}
		})
	}
}

func TestClosedRequestAndPeriodBoundary(t *testing.T) {
	v := fixedVectors(t)[0]
	r := Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: v.Intent.PublicKey}
	encoded, _ := json.Marshal(r)
	if _, e := Parse(Preflight, encoded); e != nil {
		t.Fatal(e)
	}
	for _, bad := range []string{
		strings.Replace(string(encoded), `"version":1`, `"version":1,"version":1`, 1),
		strings.Replace(string(encoded), `"version":1`, `"version":null`, 1),
		strings.Replace(string(encoded), `"participation_generation":1`, `"participation_generation":0`, 1),
		strings.Replace(string(encoded), `"participation_generation":1`, `"participation_generation":1.5`, 1),
		string(encoded) + ` {}`, string(encoded[:len(encoded)-1]) + `,"url":"https://other/"}`,
		string(encoded[:len(encoded)-1]) + `,"method":"sendrawtransaction"}`,
		strings.Repeat(" ", MaxRequestBytes+1),
	} {
		if _, e := Parse(Preflight, []byte(bad)); e == nil {
			t.Fatal("open or invalid request accepted", bad)
		}
	}
	r.Intent = &v.Intent
	r.Digest = v.Digest
	r.Transaction = v.Transaction
	encoded, _ = json.Marshal(r)
	for _, bad := range []string{
		strings.Replace(string(encoded), `"window":"24h"`, `"window":"24h","window":"24h"`, 1),
		strings.Replace(string(encoded), `"renewal_period":20000`, `"renewal_period":null`, 1),
		strings.Replace(string(encoded), `"subscription_blocks":8640`, `"subscription_blocks":8641`, 1),
	} {
		if _, e := Parse(Submit, []byte(bad)); e == nil {
			t.Fatal("bad nested intent accepted")
		}
	}
	if _, e := Parse(Query, encoded); e == nil {
		t.Fatal("query smuggled transaction")
	}
	if _, e := Parse(Preflight, encoded); e == nil {
		t.Fatal("preflight smuggled transaction")
	}
	v.Intent.RenewalPeriod = math.MaxUint64/86400 - 1
	if v.Intent.Validate() != nil {
		t.Fatal("last valid original intent period rejected")
	}
	v.Intent.RenewalPeriod++
	if v.Intent.Validate() == nil {
		t.Fatal("overflowing renewal period accepted")
	}
}
