// SPDX-License-Identifier: AGPL-3.0-only

package main

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/nknorg/nkn/v2/pb"
	"github.com/nknorg/nkn/v2/transaction"
	"google.golang.org/protobuf/proto"
)

type fixture struct {
	t          *testing.T
	confirmed  uint64
	height     uint32
	lost       bool
	queued     *pb.Subscribe
	expires    map[string]uint32
	lastSigned string
	attempts   int
}

func (f *fixture) call(_ context.Context, method string, params map[string]string, out any) error {
	f.t.Helper()
	switch method {
	case "getnoncebyaddr":
		queued := f.confirmed
		if f.queued != nil {
			queued++
		}
		*out.(*nonceStatus) = nonceStatus{Confirmed: &f.confirmed, Queued: &queued, Height: f.height}
	case "getsubscription":
		return json.Unmarshal([]byte(`{"expiresAt":`+uintText(f.expires[params["topic"]])+`}`), out)
	case "sendrawtransaction":
		f.attempts++
		bytes, err := hex.DecodeString(params["tx"])
		if err != nil {
			f.t.Fatal(err)
		}
		var tx transaction.Transaction
		if err := tx.Unmarshal(bytes); err != nil {
			f.t.Fatal(err)
		}
		if tx.UnsignedTx.Fee != 0 || tx.UnsignedTx.Nonce != f.confirmed ||
			tx.UnsignedTx.Payload.Type != pb.PayloadType_SUBSCRIBE_TYPE {
			f.t.Fatal("expected a zero-fee subscription at the current nonce")
		}
		if err := tx.VerifySignature(); err != nil {
			f.t.Fatal(err)
		}
		var subscription pb.Subscribe
		if err := proto.Unmarshal(tx.UnsignedTx.Payload.Data, &subscription); err != nil {
			f.t.Fatal(err)
		}
		if subscription.Identifier != "" || len(subscription.Meta) != 0 || len(subscription.Subscriber) != 32 {
			f.t.Fatal("subscription contains unexpected fields")
		}
		if f.lost {
			f.lastSigned = params["tx"]
			f.lost = false
			return errors.New("response lost")
		}
		if f.lastSigned != "" && params["tx"] != f.lastSigned {
			f.t.Fatal("retry changed the reserved transaction")
		}
		f.lastSigned = ""
		f.queued = &subscription
		hash := tx.Hash()
		*out.(*string) = hex.EncodeToString(hash.ToArray())
	default:
		f.t.Fatal("unexpected RPC method", method)
	}
	return nil
}

func uintText(value uint32) string {
	b, _ := json.Marshal(value)
	return string(b)
}

func TestCheckInRetriesWithoutChangingTransactionAndRenewsEachWindow(t *testing.T) {
	dir := t.TempDir()
	now := time.Date(2026, 11, 22, 0, 0, 0, 0, time.UTC)
	chain := &fixture{t: t, height: 10000, lost: true, expires: make(map[string]uint32)}
	run := func() {
		t.Helper()
		if err := checkIn(context.Background(), chain, dir, now); err != nil {
			t.Fatal(err)
		}
	}
	if err := checkIn(context.Background(), chain, dir, now); err == nil {
		t.Fatal("expected lost response")
	}
	for i, window := range windows {
		run()
		if chain.queued == nil || chain.queued.Topic != window.topic || chain.queued.Duration != window.blocks {
			t.Fatalf("expected the fixed window %s", window.topic)
		}
		run()
		if chain.attempts != i+2 {
			t.Fatal("submitted again while a transaction is pending")
		}
		chain.height++
		chain.expires[window.topic] = chain.height + window.blocks
		chain.confirmed++
		chain.queued = nil
	}
	run()
	now = now.Add(30 * time.Minute)
	run()
	if chain.attempts != 4 {
		t.Fatal("submitted again after confirmation")
	}
	if mode, err := os.Stat(filepath.Join(dir, "seed")); err != nil || mode.Mode().Perm() != 0600 {
		t.Fatal("statistics identity permissions", err)
	}
	now = now.Add(24 * time.Hour)
	chain.height += blocksPerDay + 1
	run()
	if chain.queued == nil || chain.queued.Topic != windows[0].topic || chain.attempts != 5 {
		t.Fatal("did not renew after one day of blocks")
	}
}
