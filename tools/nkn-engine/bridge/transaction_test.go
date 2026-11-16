// SPDX-License-Identifier: AGPL-3.0-only
package bridge

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

type transactionSource struct {
	calls int
	forge bool
}

func (*transactionSource) SDKLinked() bool                              { return true }
func (*transactionSource) Read(context.Context, uint32) (Status, error) { return Status{}, ErrFrame }
func (s *transactionSource) Transaction(ctx context.Context, op usagetx.Operation, r usagetx.Request) (usagetx.Reply, error) {
	s.calls++
	nonce := uint64(0)
	v, e := usagetx.NewReply(op, r, "ready", &nonce)
	if s.forge {
		v.ParticipationGeneration++
	}
	return v, e
}

func TestOnlyExplicitOwnedBridgeCanCallTransactionSource(t *testing.T) {
	tx := usagetx.Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: strings.Repeat("12", 32)}
	raw, _ := json.Marshal(protocol.Request{Protocol: 1, ID: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee", Op: protocol.UsageNonce, Transaction: &tx})
	for _, private := range []bool{false, true} {
		var out bytes.Buffer
		s := &transactionSource{}
		serve := ServeOwnedWithSource
		if private {
			serve = ServeOwnedTransactions
		}
		if e := serve(context.Background(), bytes.NewReader(frame(string(raw))), &out, nil, nil, s); e != nil {
			t.Fatal(e)
		}
		responses := decode(t, out.Bytes())
		if len(responses) != 1 {
			t.Fatal("wrong response count")
		}
		if private {
			if s.calls != 1 || responses[0].Transaction == nil || responses[0].Transaction.Nonce == nil || *responses[0].Transaction.Nonce != 0 {
				t.Fatal("real zero or private dispatch lost")
			}
		} else if s.calls != 0 || responses[0].Transaction != nil || responses[0].Error == "" {
			t.Fatal("interface alone granted transaction capability")
		}
	}
	var out bytes.Buffer
	s := &transactionSource{forge: true}
	if e := ServeOwnedTransactions(context.Background(), bytes.NewReader(frame(string(raw))), &out, nil, nil, s); e != nil {
		t.Fatal(e)
	}
	r := decode(t, out.Bytes())[0]
	if r.Transaction != nil || r.Error != "transaction_unavailable" {
		t.Fatal("source forged a cross-generation reply")
	}
}
