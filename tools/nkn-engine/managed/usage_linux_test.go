// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
)

func TestUsageOneFiniteRPCAndRealZero(t *testing.T) {
	for _, op := range []protocol.Operation{protocol.Usage24h, protocol.Usage30d, protocol.Usage90d} {
		t.Run(string(op), func(t *testing.T) {
			calls := 0
			s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				var request struct {
					ID     string            `json:"id"`
					Method string            `json:"method"`
					Params map[string]string `json:"params"`
				}
				dec := json.NewDecoder(r.Body)
				dec.DisallowUnknownFields()
				if r.Method != "POST" || dec.Decode(&request) != nil || request.ID != "rustsimhub-usage-v1" || request.Method != "getsubscriberscount" || len(request.Params) != 2 || request.Params["topic"] != "rustsimhub-usage-v1-"+protocol.UsageWindow(op) || request.Params["subscriberHashPrefix"] != "" {
					t.Error("unexpected RPC")
				}
				io.WriteString(w, `{"jsonrpc":"2.0","id":"rustsimhub-usage-v1","result":0}`)
			}))
			defer s.Close()
			value, err := usageCount(context.Background(), op, s.Client(), s.URL)
			if err != nil || value.Count != 0 || value.Window != protocol.UsageWindow(op) || calls != 1 {
				t.Fatalf("count %v err %v calls %d", value, err, calls)
			}
		})
	}
}
func TestUsageBadRepliesNeverInventZero(t *testing.T) {
	for _, raw := range []string{
		`{}`, `{"id":"rustsimhub-usage-v1"}`, `{"id":"rustsimhub-usage-v1","result":null}`,
		`{"id":"rustsimhub-usage-v1","result":"3"}`, `{"id":"other","result":3}`,
		`{"id":"rustsimhub-usage-v1","result":-1}`, `{"id":"rustsimhub-usage-v1","result":1.5}`,
		`{"id":"rustsimhub-usage-v1","result":1000000001}`, `{"id":"rustsimhub-usage-v1","result":1,"result":2}`,
		`{"id":"rustsimhub-usage-v1","result":1,"error":null}`, `{"id":"rustsimhub-usage-v1","result":1} {}`,
		strings.Repeat(" ", 2049),
	} {
		t.Run(raw[:min(len(raw), 70)], func(t *testing.T) {
			calls := 0
			s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { calls++; io.WriteString(w, raw) }))
			defer s.Close()
			if _, err := usageCount(context.Background(), protocol.Usage24h, s.Client(), s.URL); err == nil {
				t.Fatal("invalid count accepted")
			}
			if calls != 1 {
				t.Fatal("automatic retry")
			}
		})
	}
}
func TestUsageCancellationAndNoRedirect(t *testing.T) {
	calls := 0
	s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { calls++; http.Redirect(w, r, "/other", http.StatusFound) }))
	defer s.Close()
	client := newUsageHTTP(nil)
	client.Transport = s.Client().Transport
	if _, err := usageCount(context.Background(), protocol.Usage24h, client, s.URL); err == nil || calls != 1 {
		t.Fatal("redirect accepted")
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := usageCount(ctx, protocol.Usage24h, client, s.URL); err == nil || calls != 1 {
		t.Fatal("cancelled request sent")
	}
	ctx, cancel = context.WithTimeout(context.Background(), 5*time.Millisecond)
	defer cancel()
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { <-r.Context().Done() }))
	defer slow.Close()
	start := time.Now()
	if _, err := usageCount(ctx, protocol.Usage24h, slow.Client(), slow.URL); err == nil || time.Since(start) > time.Second {
		t.Fatal("deadline not enforced")
	}
}
