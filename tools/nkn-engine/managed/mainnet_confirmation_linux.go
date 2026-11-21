// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"io"
	"net"
	"net/http"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

const (
	mainnetIndexHost          = "openapi.nkn.org"
	mainnetIndexPath          = "https://" + mainnetIndexHost + "/api/v1/transactions/"
	mainnetSuccessors         = 6
	maxIndexedTransaction     = 32 * 1024
	maxBlockTransactionHashes = 256 * 1024
)

func newMainnetIndex(dial *scopedDialer) *http.Client {
	client := newUsageHTTP(dial)
	transport := client.Transport.(*http.Transport)
	transport.DialContext = func(ctx context.Context, network, address string) (net.Conn, error) {
		if address != mainnetIndexHost+":443" {
			return nil, ErrLaunch
		}
		return dial.dial(ctx, network, address)
	}
	return client
}

func readIndex(ctx context.Context, client *http.Client, digest string) (uint32, bool) {
	if !isChainHash(digest) {
		return 0, false
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, mainnetIndexPath+digest, nil)
	if err != nil {
		return 0, false
	}
	req.Header.Set("Accept", "application/json")
	resp, err := client.Do(req)
	if err != nil {
		return 0, false
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK || resp.ContentLength > maxIndexedTransaction {
		return 0, false
	}
	raw, err := io.ReadAll(io.LimitReader(resp.Body, maxIndexedTransaction+1))
	if err != nil || len(raw) > maxIndexedTransaction {
		return 0, false
	}
	// This origin only supplies a height hint. Its transaction fields can
	// never confirm a check-in; a separate wallet API must prove inclusion.
	var hint struct {
		Hash   string `json:"hash"`
		Height uint32 `json:"block_height"`
	}
	if json.Unmarshal(raw, &hint) != nil || hint.Hash != digest || hint.Height == 0 {
		return 0, false
	}
	return hint.Height, true
}

func isChainHash(s string) bool {
	if len(s) != 64 {
		return false
	}
	raw, err := hex.DecodeString(s)
	return err == nil && hex.EncodeToString(raw) == s
}

func mainnetBlockHash(raw json.RawMessage, height uint32, digest string) (string, bool) {
	fields, err := usagetx.Fields(raw, maxBlockTransactionHashes)
	if err != nil || len(fields) != 3 || fields["Height"] == nil || fields["Hash"] == nil || fields["Transactions"] == nil {
		return "", false
	}
	var block struct {
		Hash         string
		Height       uint32
		Transactions []string
	}
	if json.Unmarshal(raw, &block) != nil || block.Height != height || !isChainHash(block.Hash) {
		return "", false
	}
	found := 0
	for _, tx := range block.Transactions {
		if !isChainHash(tx) {
			return "", false
		}
		if tx == digest {
			found++
		}
	}
	return block.Hash, found == 1
}

type blockRPC func(context.Context, string, map[string]any, int) (json.RawMessage, error)

func verifyMainnetInclusion(ctx context.Context, rpc blockRPC, height uint32, digest string) bool {
	params := map[string]any{"height": height}
	first, err := rpc(ctx, "getblocktxsbyheight", params, maxBlockTransactionHashes)
	if err != nil {
		return false
	}
	hash, ok := mainnetBlockHash(first, height, digest)
	if !ok {
		return false
	}
	rawTip, err := rpc(ctx, "getlatestblockheight", map[string]any{}, 2048)
	if err != nil {
		return false
	}
	var tip uint32
	if json.Unmarshal(rawTip, &tip) != nil || tip < height || tip-height < mainnetSuccessors {
		return false
	}
	// Reject a branch change observed during the read. If any observation
	// fails or disagrees, the original pending digest remains query-only.
	last, err := rpc(ctx, "getblocktxsbyheight", params, maxBlockTransactionHashes)
	observed, ok := mainnetBlockHash(last, height, digest)
	return err == nil && ok && hash == observed && ctx.Err() == nil
}

func (c *usageTransactions) mainnetConfirmed(ctx context.Context, digest string) bool {
	if c.index == nil || !c.live(ctx) || c.lease.launch.seeds[0] != protocol.MainnetWalletRPC {
		return false
	}
	height, ok := readIndex(ctx, c.index, digest)
	return ok && verifyMainnetInclusion(ctx, c.rpc, height, digest) && c.live(ctx)
}
