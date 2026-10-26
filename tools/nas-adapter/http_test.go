// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
)

func tlsServer(t *testing.T, handler http.HandlerFunc) (*httptest.Server, *http.Client) {
	t.Helper()
	s := httptest.NewUnstartedServer(handler)
	s.Config.ErrorLog = log.New(io.Discard, "", 0)
	s.TLS = &tls.Config{MinVersion: tls.VersionTLS13}
	s.StartTLS()
	t.Cleanup(s.Close)
	roots := x509.NewCertPool()
	roots.AddCert(s.Certificate())
	c := newHTTPClient(roots)
	t.Cleanup(c.CloseIdleConnections)
	return s, c
}

func TestTLSReadUsesOnlyFormalContractAndDoesNotRetry(t *testing.T) {
	var requests atomic.Int32
	s, client := tlsServer(t, func(w http.ResponseWriter, r *http.Request) {
		requests.Add(1)
		if r.Method != "POST" || r.URL.Path != "/compat/v1/read" || r.URL.RawQuery != "" || r.Header.Get("Authorization") != "Bearer "+testToken || r.Header.Get("X-RSH-Legacy-Compat") != "1" || r.Header.Get("X-RSH-Request-Id") != "synthetic-request" {
			t.Error("read authority/path changed")
		}
		var q Query
		if json.NewDecoder(r.Body).Decode(&q) != nil || q.Entity != "status" {
			t.Error("bad request")
		}
		if requests.Load() == 1 {
			w.WriteHeader(503)
			io.WriteString(w, `{"code":"core_unavailable","retryable":false,"adapter_required":true}`)
			return
		}
		w.Write(envelope(LineView{Line: 7, Status: "observing"}))
	})
	// A configured proxy must not receive private NAS credentials.
	t.Setenv("HTTPS_PROXY", "http://127.0.0.1:1")
	q := Query{Entity: "status", Limit: 1}
	reply, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", q)
	requireFailure(t, err, "backend_unavailable")
	if reply != nil || requests.Load() != 1 {
		t.Fatal("automatic retry or partial reply")
	}
	reply, err = readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", q)
	if err != nil || reply.Data.(LineView).Line != 7 || requests.Load() != 2 {
		t.Fatalf("explicit retry failed: %v", err)
	}
}

func TestHTTPFailuresRemainDistinctAndNeverPrintRemoteStrings(t *testing.T) {
	for _, tc := range []struct {
		status     int
		code, kind string
	}{
		{400, "invalid_request", "http_error"}, {401, "permission_denied", "unauthorized"},
		{403, "permission_denied", "forbidden"}, {429, "legacy_busy", "busy"},
		{501, "not_supported", "not_implemented"}, {501, "legacy_disabled", "compat_disabled"},
		{503, "core_unavailable", "backend_unavailable"},
	} {
		t.Run(tc.kind, func(t *testing.T) {
			s, client := tlsServer(t, func(w http.ResponseWriter, r *http.Request) {
				w.WriteHeader(tc.status)
				json.NewEncoder(w).Encode(struct {
					Code            string `json:"code"`
					Retryable       bool   `json:"retryable"`
					AdapterRequired bool   `json:"adapter_required"`
				}{tc.code, false, true})
			})
			_, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "status", Limit: 1})
			requireFailure(t, err, tc.kind)
			if err.(*Failure).HTTP != tc.status {
				t.Fatal("HTTP status lost")
			}
		})
	}
	err := httpFailure(401, []byte(`{"code":"<script>secret\nforged log</script>","retryable":false,"adapter_required":true}`))
	if strings.Contains(err.Error(), "secret") || strings.Contains(err.Error(), "script") {
		t.Fatal("remote failure text leaked")
	}
	requireFailure(t, httpFailure(403, []byte(`garbage`)), "forbidden")
}

func TestRedirectAndInvalidCertificatesDoNotReachTarget(t *testing.T) {
	var destination atomic.Int32
	target, _ := tlsServer(t, func(w http.ResponseWriter, r *http.Request) { destination.Add(1) })
	var source atomic.Int32
	s, client := tlsServer(t, func(w http.ResponseWriter, r *http.Request) { source.Add(1); http.Redirect(w, r, target.URL, 307) })
	_, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "status", Limit: 1})
	// 307 cannot replay this request body (GetBody is deliberately absent), so
	// net/http returns the original HTTP response without following it.
	requireFailure(t, err, "http_error")
	if err.(*Failure).HTTP != 307 {
		t.Fatal("redirect status lost")
	}
	if source.Load() != 1 || destination.Load() != 0 {
		t.Fatal("followed redirect")
	}
	untrusted := newHTTPClient(x509.NewCertPool())
	defer untrusted.CloseIdleConnections()
	_, err = readOnce(context.Background(), untrusted, s.URL, testToken, "synthetic-request", Query{Entity: "status", Limit: 1})
	requireFailure(t, err, "transport_failed")
	if source.Load() != 1 {
		t.Fatal("invalid certificate reached handler")
	}
}

func TestBadAndOversizedHTTPReplyNeverPublishesPage(t *testing.T) {
	for _, body := range [][]byte{[]byte(`{"data":`), bytes.Repeat([]byte("x"), maxReply+1)} {
		s, client := tlsServer(t, func(w http.ResponseWriter, r *http.Request) { w.Write(body) })
		reply, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "sms", Limit: 1})
		requireFailure(t, err, "invalid_reply")
		if reply != nil {
			t.Fatal("published malformed page")
		}
	}
}

func TestTLS12IsRejectedAndInvalidInputNeverSendsRequest(t *testing.T) {
	var count atomic.Int32
	s := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { count.Add(1) }))
	s.Config.ErrorLog = log.New(io.Discard, "", 0)
	s.TLS = &tls.Config{MaxVersion: tls.VersionTLS12}
	s.StartTLS()
	defer s.Close()
	roots := x509.NewCertPool()
	roots.AddCert(s.Certificate())
	client := newHTTPClient(roots)
	defer client.CloseIdleConnections()
	_, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "status", Limit: 1})
	requireFailure(t, err, "transport_failed")
	_, err = readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "send_sms", Limit: 1})
	requireFailure(t, err, "invalid_query")
	if count.Load() != 0 {
		t.Fatal("unsupported request sent")
	}
}

func TestRedirect302IsDeniedByClientPolicy(t *testing.T) {
	var count atomic.Int32
	target, _ := tlsServer(t, func(w http.ResponseWriter, r *http.Request) { count.Add(1) })
	s, client := tlsServer(t, func(w http.ResponseWriter, r *http.Request) { http.Redirect(w, r, target.URL, 302) })
	_, err := readOnce(context.Background(), client, s.URL, testToken, "synthetic-request", Query{Entity: "status", Limit: 1})
	requireFailure(t, err, "transport_failed")
	if count.Load() != 0 {
		t.Fatal("followed redirect")
	}
}
