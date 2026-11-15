// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
)

// One explicit read, one fixed installed seed, no retries, proxy, redirects,
// subscriber list, identity, signing or chain writes. The existing scoped dialer
// and Root UID nft policy still fence DNS, destinations and egress interface.
func usageCount(ctx context.Context, op protocol.Operation, client *http.Client, endpoint string) (bridge.UsageCount, error) {
	window := protocol.UsageWindow(op)
	if window == "" || ctx.Err() != nil {
		return bridge.UsageCount{}, ErrLaunch
	}
	body, _ := json.Marshal(map[string]any{"id": "rustsimhub-usage-v1", "method": "getsubscriberscount", "params": map[string]string{"topic": "rustsimhub-usage-v1-" + window, "subscriberHashPrefix": ""}})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(body))
	if err != nil {
		return bridge.UsageCount{}, ErrLaunch
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := client.Do(req)
	if err != nil {
		return bridge.UsageCount{}, ErrLaunch
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK || resp.ContentLength > 2048 {
		return bridge.UsageCount{}, ErrLaunch
	}
	raw, err := io.ReadAll(io.LimitReader(resp.Body, 2049))
	if err != nil || len(raw) > 2048 || ctx.Err() != nil {
		return bridge.UsageCount{}, ErrLaunch
	}
	// Token-based decoding rejects duplicate result/error/id keys. Missing or
	// null result, strings, negative/fractional/overflow counts never become zero.
	dec := json.NewDecoder(bytes.NewReader(raw))
	first, err := dec.Token()
	if err != nil || first != json.Delim('{') {
		return bridge.UsageCount{}, ErrLaunch
	}
	seen := map[string]bool{}
	var count *uint64
	validID := false
	for dec.More() {
		key, err := dec.Token()
		name, ok := key.(string)
		if err != nil || !ok || seen[name] {
			return bridge.UsageCount{}, ErrLaunch
		}
		seen[name] = true
		switch name {
		case "id":
			var id string
			if dec.Decode(&id) != nil || id != "rustsimhub-usage-v1" {
				return bridge.UsageCount{}, ErrLaunch
			}
			validID = true
		case "result":
			if dec.Decode(&count) != nil {
				return bridge.UsageCount{}, ErrLaunch
			}
		case "jsonrpc":
			var version string
			if dec.Decode(&version) != nil || version != "2.0" {
				return bridge.UsageCount{}, ErrLaunch
			}
		default:
			return bridge.UsageCount{}, ErrLaunch
		}
	}
	end, err := dec.Token()
	var extra any
	if err != nil || end != json.Delim('}') || dec.Decode(&extra) != io.EOF || !validID || count == nil || *count > 1000000000 {
		return bridge.UsageCount{}, ErrLaunch
	}
	return bridge.UsageCount{Window: window, Count: *count}, nil
}

func newUsageHTTP(dial *scopedDialer) *http.Client {
	return &http.Client{Timeout: time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return ErrLaunch },
		Transport: &http.Transport{Proxy: nil, DialContext: dial.httpDial, DisableKeepAlives: true, TLSHandshakeTimeout: time.Second, ResponseHeaderTimeout: time.Second, MaxResponseHeaderBytes: 4096}}
}
