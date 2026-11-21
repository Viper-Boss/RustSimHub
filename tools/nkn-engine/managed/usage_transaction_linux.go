// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"sync"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

const transactionRPCID = "rustsimhub-statistics-transaction-v1"

type transactionReply struct {
	Nonce *uint64 `json:"nonce,omitempty"`
	State string  `json:"state,omitempty"`
}

// An ephemeral, one-use nonce reservation within ONE original Root Lease. This
// is not another CheckInMachine or persistent check-in state. In particular a
// consumed reservation cannot be forgotten on timeout or replaced by preflight.
type nonceReservation struct {
	id         string
	generation uint64
	publicKey  string
	nonce      uint64
	deadline   time.Time
	consumed   bool
	intent     usagetx.Intent
	digest     string
	state      string
}

// No exported constructor, endpoint argument, general RPC API, key/seed access,
// signing, Root stop, or shared-reference release. Not wired into runtime yet.
type usageTransactions struct {
	mu       sync.Mutex
	lease    *Lease
	client   *http.Client
	address  string
	reserved *nonceReservation
}

func newUsageTransactions(lease *Lease, dial *scopedDialer) (*usageTransactions, error) {
	if lease == nil || dial == nil || dial.lease != lease || lease.Check() != nil || len(lease.launch.seeds) == 0 {
		return nil, ErrLaunch
	}
	address, err := statisticsAddress(lease.launch.statisticsKey)
	if err != nil {
		return nil, ErrLaunch
	}
	return &usageTransactions{lease: lease, client: newUsageHTTP(dial), address: address}, nil
}

func (c *usageTransactions) live(ctx context.Context) bool {
	if c == nil || c.lease == nil || c.client == nil || c.address == "" || ctx == nil || ctx.Err() != nil {
		return false
	}
	d, ok := ctx.Deadline()
	return ok && time.Now().Before(d) && c.lease.Check() == nil && len(c.lease.launch.seeds) > 0
}

func sameReservation(t *nonceReservation, r usagetx.Request) bool {
	return t != nil && t.id == r.OperationID && t.generation == r.ParticipationGeneration && t.publicKey == r.PublicKey
}

func (c *usageTransactions) execute(ctx context.Context, op usagetx.Operation, r usagetx.Request) (transactionReply, error) {
	// Copy the validated nested intent; do not retain a pointer owned by caller.
	raw, err := json.Marshal(r)
	if err != nil {
		return transactionReply{}, ErrLaunch
	}
	r, err = usagetx.Parse(op, raw)
	if err != nil || !c.live(ctx) || r.PublicKey != hex.EncodeToString(c.lease.launch.statisticsKey[:]) || !c.mu.TryLock() {
		return transactionReply{}, ErrLaunch
	}
	defer c.mu.Unlock()
	if !c.live(ctx) {
		return transactionReply{}, ErrLaunch
	}
	switch op {
	case usagetx.Preflight:
		if old := c.reserved; old != nil {
			if old.consumed {
				return transactionReply{}, ErrLaunch
			}
			if time.Now().Before(old.deadline) {
				if !sameReservation(old, r) {
					return transactionReply{}, ErrLaunch
				}
				n := old.nonce
				return transactionReply{Nonce: &n}, nil
			}
		}
		deadline, _ := ctx.Deadline()
		deadline = minTime(deadline, time.Now().Add(10*time.Second))
		result, e := c.rpc(ctx, "getnoncebyaddr", map[string]string{"address": c.address}, 2048)
		if e != nil || !c.live(ctx) || !time.Now().Before(deadline) {
			return transactionReply{}, ErrLaunch
		}
		f, e := usagetx.Fields(result, 2048)
		if e != nil || len(f) != 2 || f["nonce"] == nil || f["nonceInTxPool"] == nil {
			return transactionReply{}, ErrLaunch
		}
		var chain, pool uint64
		if json.Unmarshal(f["nonce"], &chain) != nil || json.Unmarshal(f["nonceInTxPool"], &pool) != nil {
			return transactionReply{}, ErrLaunch
		}
		n := max(chain, pool)
		c.reserved = &nonceReservation{id: r.OperationID, generation: r.ParticipationGeneration, publicKey: r.PublicKey, nonce: n, deadline: deadline}
		return transactionReply{Nonce: &n}, nil
	case usagetx.Submit:
		t := c.reserved
		if !sameReservation(t, r) || !time.Now().Before(t.deadline) {
			return transactionReply{}, ErrLaunch
		}
		if _, e := usagetx.VerifySubmission(r, t.nonce); e != nil {
			return transactionReply{}, ErrLaunch
		}
		if t.consumed {
			if t.digest != r.Digest || t.intent != *r.Intent {
				return transactionReply{}, ErrLaunch
			}
			return transactionReply{State: t.state}, nil
		}
		// Consume BEFORE any write, including a failed HTTP attempt. Cancellation,
		// lost ack, invalid result or a later preflight cannot trigger resubmission.
		t.consumed, t.intent, t.digest, t.state = true, *r.Intent, r.Digest, "unknown"
		bounded, cancel := context.WithDeadline(ctx, t.deadline)
		defer cancel()
		result, e := c.rpc(bounded, "sendrawtransaction", map[string]string{"tx": r.Transaction}, 2048)
		var digest string
		if e == nil && c.live(bounded) && json.Unmarshal(result, &digest) == nil && digest == r.Digest {
			t.state = "accepted" // Node acceptance is NOT chain confirmation.
		}
		return transactionReply{State: t.state}, nil
	case usagetx.Query:
		if t := c.reserved; t != nil && t.consumed && (t.publicKey != r.PublicKey || t.digest != r.Digest || t.intent != *r.Intent) {
			return transactionReply{}, ErrLaunch
		}
		// Restart recovery may query the persisted ORIGINAL intent/digest without
		// preflight or resubmitting. Trust/depth policy is not approved: even a
		// node's plausible result cannot release a pending nonce or confirm it.
		_, _ = c.rpc(ctx, "gettransaction", map[string]string{"hash": r.Digest}, 16*1024)
		return transactionReply{State: "unknown"}, nil
	}
	return transactionReply{}, ErrLaunch
}

func minTime(a, b time.Time) time.Time {
	if a.Before(b) {
		return a
	}
	return b
}

// Exactly one request to the first installed seed, on the original scoped
// transport. No implicit fallback, batch, retry, subscriber enumeration or
// user-selected destination/method. Per-I/O 1s is inside the original deadline.
func (c *usageTransactions) rpc(ctx context.Context, method string, params map[string]string, limit int) (json.RawMessage, error) {
	if !c.live(ctx) {
		return nil, ErrLaunch
	}
	if len(params) != 1 {
		return nil, ErrLaunch
	}
	switch method {
	case "getnoncebyaddr":
		if params["address"] != c.address || limit != 2048 {
			return nil, ErrLaunch
		}
	case "sendrawtransaction":
		if params["tx"] == "" || len(params["tx"]) > usagetx.MaxTransactionBytes*2 || limit != 2048 {
			return nil, ErrLaunch
		}
	case "gettransaction":
		if len(params["hash"]) != 64 || limit != 16*1024 {
			return nil, ErrLaunch
		}
	default:
		return nil, ErrLaunch
	}
	bounded, cancel := context.WithTimeout(ctx, time.Second)
	stop := context.AfterFunc(c.lease.Context(), cancel)
	defer func() { stop(); cancel() }()
	body, _ := json.Marshal(struct {
		ID     string            `json:"id"`
		Method string            `json:"method"`
		Params map[string]string `json:"params"`
	}{transactionRPCID, method, params})
	req, e := http.NewRequestWithContext(bounded, http.MethodPost, c.lease.launch.seeds[0], bytes.NewReader(body))
	if e != nil || !c.live(bounded) {
		return nil, ErrLaunch
	}
	req.Header.Set("Content-Type", "application/json")
	client := *c.client
	client.CheckRedirect = func(*http.Request, []*http.Request) error { return ErrLaunch }
	resp, e := client.Do(req)
	if e != nil {
		return nil, ErrLaunch
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK || resp.ContentLength > int64(limit) {
		return nil, ErrLaunch
	}
	raw, e := io.ReadAll(io.LimitReader(resp.Body, int64(limit)+1))
	if e != nil || len(raw) > limit || !c.live(bounded) {
		return nil, ErrLaunch
	}
	f, e := usagetx.Fields(raw, limit)
	if e != nil || len(f) < 2 || len(f) > 3 || f["id"] == nil || f["result"] == nil {
		return nil, ErrLaunch
	}
	for key := range f {
		if key != "id" && key != "result" && key != "jsonrpc" {
			return nil, ErrLaunch
		}
	}
	var id string
	if json.Unmarshal(f["id"], &id) != nil || id != transactionRPCID {
		return nil, ErrLaunch
	}
	if version, ok := f["jsonrpc"]; ok {
		var v string
		if json.Unmarshal(version, &v) != nil || v != "2.0" {
			return nil, ErrLaunch
		}
	}
	return f["result"], nil
}
