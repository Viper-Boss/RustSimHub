// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"
)

func newHTTPClient(roots *x509.CertPool) *http.Client {
	transport := &http.Transport{Proxy: nil, TLSClientConfig: &tls.Config{RootCAs: roots, MinVersion: tls.VersionTLS13}, DisableKeepAlives: true, MaxConnsPerHost: 1, TLSHandshakeTimeout: 5 * time.Second, ResponseHeaderTimeout: 10 * time.Second}
	return &http.Client{Transport: transport, Timeout: 15 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return errors.New("redirect_denied") }}
}

func httpFailure(status int, raw []byte) error {
	kind := "http_error"
	switch status {
	case 401:
		kind = "unauthorized"
	case 403:
		kind = "forbidden"
	case 429:
		kind = "busy"
	case 501:
		kind = "not_implemented"
	case 502, 503, 504:
		kind = "backend_unavailable"
	}
	var failure struct {
		Code            string `json:"code"`
		Retryable       bool   `json:"retryable"`
		AdapterRequired bool   `json:"adapter_required"`
	}
	if strictJSON(raw, &failure) == nil && status == 501 && failure.Code == "legacy_disabled" {
		kind = "compat_disabled"
	}
	return &Failure{Kind: kind, HTTP: status}
}

// One bounded request. POST has no idempotency header or replayable GetBody;
// neither this function nor the CLI performs automatic retries.
func exchange(ctx context.Context, client *http.Client, origin, path string, headers map[string]string, body []byte) ([]byte, error) {
	if path != "/v1/session" && path != "/compat/v1/read" {
		return nil, &Failure{Kind: "invalid_query"}
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimSuffix(origin, "/")+path, io.NopCloser(bytes.NewReader(body)))
	if err != nil {
		return nil, &Failure{Kind: "invalid_query"}
	}
	request.ContentLength = int64(len(body))
	request.Header.Set("Content-Type", "application/json")
	for key, value := range headers {
		request.Header.Set(key, value)
	}
	response, err := client.Do(request)
	if err != nil {
		return nil, &Failure{Kind: "transport_failed"}
	}
	defer response.Body.Close()
	data, err := io.ReadAll(io.LimitReader(response.Body, maxReply+1))
	if err != nil || len(data) > maxReply {
		if response.StatusCode != http.StatusOK {
			return nil, httpFailure(response.StatusCode, nil)
		}
		return nil, invalidReply()
	}
	if response.StatusCode != http.StatusOK {
		return nil, httpFailure(response.StatusCode, data)
	}
	return data, nil
}

func readOnce(ctx context.Context, client *http.Client, origin, token, requestID string, q Query) (*ReadReply, error) {
	if err := q.validate(); err != nil {
		return nil, err
	}
	if !hexToken(token) {
		return nil, &Failure{Kind: "invalid_query"}
	}
	body, err := json.Marshal(q)
	if err != nil {
		return nil, &Failure{Kind: "invalid_query"}
	}
	raw, err := exchange(ctx, client, origin, "/compat/v1/read", map[string]string{
		"Authorization": "Bearer " + token, "X-RSH-Legacy-Compat": "1", "X-RSH-Request-Id": requestID,
	}, body)
	if err != nil {
		return nil, err
	}
	return decodeRead(q, raw)
}
