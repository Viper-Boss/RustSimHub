// SPDX-License-Identifier: AGPL-3.0-only
package protocol

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

func TestOwnedTransactionsCannotEnterPublicOrLegacyParser(t *testing.T) {
	tx := usagetx.Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: strings.Repeat("12", 32)}
	raw, _ := json.Marshal(Request{Protocol: 1, ID: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", Op: UsageNonce, Transaction: &tx})
	if _, e := Parse(raw); e == nil {
		t.Fatal("public parser admitted transaction")
	}
	r, e := ParseOwned(raw)
	if e != nil || r.Transaction == nil || *r.Transaction != tx {
		t.Fatal("private bounded request refused", e)
	}
	for _, bad := range [][]byte{
		[]byte(strings.Replace(string(raw), `"op":"usage_nonce"`, `"op":"usage_nonce","op":"usage_nonce"`, 1)),
		[]byte(strings.Replace(string(raw), `"participation_generation":1`, `"participation_generation":null`, 1)),
		[]byte(strings.Replace(string(raw), `"op":"usage_nonce"`, `"op":"status"`, 1)),
		[]byte(strings.Replace(string(raw), `"version":1`, `"version":1,"method":"getnoncebyaddr"`, 1)),
		append(raw, []byte(" {}")...), []byte(strings.Repeat(" ", MaxOwnedRequestBytes+1)),
	} {
		if _, e := ParseOwned(bad); e == nil {
			t.Fatal("invalid owned request accepted")
		}
	}
	legacy := []byte(`{"protocol":1,"id":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee","op":"status"}`)
	if r, e := ParseOwned(legacy); e != nil || r.Op != Status || r.Transaction != nil {
		t.Fatal("legacy request compatibility lost")
	}
}
