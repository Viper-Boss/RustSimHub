// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/json"
	"errors"
	"strings"
	"testing"
)

func TestMainnetRequiresExactPendingDigestStableBlockAndDepth(t *testing.T) {
	digest := strings.Repeat("a", 64)
	makeBlock := func(hash string, height uint32, txs []string) json.RawMessage {
		b, _ := json.Marshal(map[string]any{"Hash": hash, "Height": height, "Transactions": txs})
		return b
	}
	first := makeBlock(strings.Repeat("c", 64), 42, []string{digest})
	for _, tt := range []struct {
		name                string
		initial, tip, final json.RawMessage
		want                bool
	}{
		{"included", first, json.RawMessage("48"), first, true},
		{"shallow", first, json.RawMessage("47"), first, false},
		{"reorganization", first, json.RawMessage("48"), makeBlock(strings.Repeat("d", 64), 42, []string{digest}), false},
		{"alien digest", makeBlock(strings.Repeat("c", 64), 42, []string{strings.Repeat("e", 64)}), json.RawMessage("48"), first, false},
		{"height mismatch", makeBlock(strings.Repeat("c", 64), 43, []string{digest}), json.RawMessage("48"), first, false},
		{"duplicate digest", makeBlock(strings.Repeat("c", 64), 42, []string{digest, digest}), json.RawMessage("48"), first, false},
		{"unavailable", first, json.RawMessage("48"), nil, false},
	} {
		t.Run(tt.name, func(t *testing.T) {
			blocks := 0
			rpc := func(_ context.Context, method string, params map[string]any, limit int) (json.RawMessage, error) {
				switch method {
				case "getblocktxsbyheight":
					if params["height"] != uint32(42) || limit != maxBlockTransactionHashes {
						t.Fatal("unbounded or mismatched lookup")
					}
					blocks++
					if blocks == 1 {
						return tt.initial, nil
					}
					if tt.final == nil {
						return nil, errors.New("offline")
					}
					return tt.final, nil
				case "getlatestblockheight":
					if len(params) != 0 || limit != 2048 {
						t.Fatal("invalid chain tip lookup")
					}
					return tt.tip, nil
				default:
					t.Fatal("unexpected RPC")
					return nil, errors.New("invalid")
				}
			}
			if got := verifyMainnetInclusion(context.Background(), rpc, 42, digest); got != tt.want {
				t.Fatalf("got %v, want %v", got, tt.want)
			}
		})
	}
}
