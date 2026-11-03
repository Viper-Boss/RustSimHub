// SPDX-License-Identifier: AGPL-3.0-only
// Package control is the separate NasControlClientV1 candidate. It never creates
// sessions, reads legacy NAS keys, upgrades scopes, or retries a request.
package control

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"
)

const MaxReply = 65536

var uuid = regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`)
var hex64 = regexp.MustCompile(`^[0-9a-f]{64}$`)
var invalid = errors.New("invalid_control_contract")

// Failure contains only finite diagnostics and the caller's recovery identifier.
// Unknown means query the operation; it is never permission to send again.
type Failure struct {
	Kind       string
	Unknown    bool
	RecoveryID string
}

func (f *Failure) Error() string { return f.Kind }

type Lease struct {
	CallID     string `json:"call_id"`
	Boot       string `json:"boot"`
	Line       uint64 `json:"line"`
	Generation uint64 `json:"generation"`
	Expires    uint32 `json:"expires_in_seconds"`
}

func (l *Lease) valid() bool {
	return l != nil && uuid.MatchString(l.CallID) && hex64.MatchString(l.Boot) && l.Line > 0 && l.Generation > 0 && l.Expires <= 30
}

type Operation struct {
	ID     string  `json:"id"`
	CallID *string `json:"call_id"`
	Line   uint64  `json:"line"`
	State  string  `json:"state"`
}
type Receipt struct {
	Operation Operation `json:"operation"`
	Lease     *Lease    `json:"lease"`
}
type Client struct {
	origin string
	token  string
	http   *http.Client
}

// New requires a separately provisioned normal client token and explicit CA.
// Core remains the authority for principal kind, scopes, line and live leases.
func New(origin, token string, caPEM []byte) (*Client, error) {
	u, err := url.Parse(origin)
	if err != nil || u.Scheme != "https" || u.Host == "" || u.Hostname() == "" || u.User != nil || u.RawQuery != "" || u.ForceQuery || u.RawPath != "" || u.Fragment != "" || (u.Path != "" && u.Path != "/") || !hex64.MatchString(token) {
		return nil, invalid
	}
	roots := x509.NewCertPool()
	if !roots.AppendCertsFromPEM(caPEM) {
		return nil, invalid
	}
	transport := &http.Transport{Proxy: nil, TLSClientConfig: &tls.Config{RootCAs: roots, MinVersion: tls.VersionTLS13}, DisableKeepAlives: true, MaxConnsPerHost: 1, TLSHandshakeTimeout: 5 * time.Second, ResponseHeaderTimeout: 10 * time.Second}
	return &Client{strings.TrimSuffix(origin, "/"), token, &http.Client{Transport: transport, Timeout: 15 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return errors.New("redirect_denied") }}}, nil
}
func (c *Client) Close() { c.http.CloseIdleConnections() }

// StrictJSON rejects duplicate keys, trailing values, unknown fields and deep
// nesting before decoding. Exported for the finite CLI and shared vectors.
func StrictJSON(raw []byte, out any) error {
	if len(raw) == 0 || len(raw) > MaxReply || !utf8.Valid(raw) {
		return invalid
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()
	var walk func(int) error
	walk = func(depth int) error {
		if depth > 32 {
			return invalid
		}
		token, err := d.Token()
		if err != nil {
			return invalid
		}
		if delim, ok := token.(json.Delim); ok {
			switch delim {
			case '{':
				seen := map[string]bool{}
				for d.More() {
					k, e := d.Token()
					s, ok := k.(string)
					if e != nil || !ok || seen[s] {
						return invalid
					}
					seen[s] = true
					if walk(depth+1) != nil {
						return invalid
					}
				}
				end, e := d.Token()
				if e != nil || end != json.Delim('}') {
					return invalid
				}
			case '[':
				for d.More() {
					if walk(depth+1) != nil {
						return invalid
					}
				}
				end, e := d.Token()
				if e != nil || end != json.Delim(']') {
					return invalid
				}
			default:
				return invalid
			}
		}
		return nil
	}
	if walk(0) != nil {
		return invalid
	}
	if _, err := d.Token(); err != io.EOF {
		return invalid
	}
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if decoder.Decode(out) != nil {
		return invalid
	}
	return nil
}

func (c *Client) exchange(ctx context.Context, id string, command map[string]any, recovery string, mutation bool, out any) error {
	if len(id) == 0 || len(id) > 128 || strings.ContainsAny(id, "\r\n\x00") {
		return invalid
	}
	command["token"] = c.token
	body, err := json.Marshal(map[string]any{"protocol": 2, "id": id, "command": command})
	if err != nil || len(body) > MaxReply {
		return invalid
	}
	// Non-replayable body: no GetBody, Idempotency-Key header, redirects or retry.
	req, err := http.NewRequestWithContext(ctx, "POST", c.origin+"/api/v2", io.NopCloser(bytes.NewReader(body)))
	if err != nil {
		return invalid
	}
	req.ContentLength = int64(len(body))
	req.Header.Set("Content-Type", "application/json")
	response, err := c.http.Do(req)
	if err != nil {
		return &Failure{"transport_failed", mutation, recovery}
	}
	defer response.Body.Close()
	if response.StatusCode != 200 {
		return &Failure{"http_denied", mutation, recovery}
	}
	raw, err := io.ReadAll(io.LimitReader(response.Body, MaxReply+1))
	if err != nil || len(raw) > MaxReply {
		return &Failure{"invalid_reply", mutation, recovery}
	}
	var envelope struct {
		Protocol uint16          `json:"protocol"`
		ID       string          `json:"id"`
		Result   json.RawMessage `json:"result"`
	}
	if StrictJSON(raw, &envelope) != nil || envelope.Protocol != 2 || envelope.ID != id {
		return &Failure{"invalid_reply", mutation, recovery}
	}
	var denied struct {
		Ok    bool   `json:"ok"`
		Error string `json:"error"`
	}
	if StrictJSON(envelope.Result, &denied) == nil && denied.Error != "" {
		return &Failure{"core_denied", mutation, recovery}
	}
	if StrictJSON(envelope.Result, out) != nil {
		return &Failure{"invalid_reply", mutation, recovery}
	}
	return nil
}

func validateReceipt(r *Receipt, operationID string, line uint64) error {
	op := r.Operation
	if !uuid.MatchString(op.ID) || (operationID != "" && op.ID != operationID) || op.Line == 0 || (line != 0 && op.Line != line) || (op.CallID != nil && !uuid.MatchString(*op.CallID)) {
		return invalid
	}
	switch op.State {
	case "prepared", "submitting", "submitted", "unknown", "cancelled":
	default:
		return invalid
	}
	if r.Lease != nil && (!r.Lease.valid() || op.CallID == nil || r.Lease.CallID != *op.CallID || r.Lease.Line != op.Line) {
		return invalid
	}
	return nil
}
func (c *Client) operation(ctx context.Context, id, op, operationID string, lease *Lease, urgent bool) (*Receipt, error) {
	if !uuid.MatchString(operationID) {
		return nil, invalid
	}
	command := map[string]any{"op": op, "operation_id": operationID}
	mutation := op != "call_operation"
	if op == "submit_call" {
		if lease != nil && !lease.valid() {
			return nil, invalid
		}
		command["lease"] = lease
		command["urgent"] = urgent
	}
	var r Receipt
	if err := c.exchange(ctx, id, command, operationID, mutation, &r); err != nil {
		return nil, err
	}
	if validateReceipt(&r, operationID, 0) != nil {
		return nil, &Failure{"invalid_reply", mutation, operationID}
	}
	return &r, nil
}
func (c *Client) Prepare(ctx context.Context, id string, line uint64, key, action string, callID *string, payload string) (*Receipt, error) {
	if line == 0 || line > 1<<63-1 || len(key) == 0 || len(key) > 128 || strings.HasPrefix(key, "__") || strings.ContainsAny(key, "\r\n\x00") || len(payload) > 32 {
		return nil, invalid
	}
	switch action {
	case "dial":
		if callID != nil || len(payload) < 3 || !regexp.MustCompile(`^\+?[0-9]+$`).MatchString(payload) {
			return nil, invalid
		}
	case "accept", "hangup":
		if callID == nil || !uuid.MatchString(*callID) || payload != "" {
			return nil, invalid
		}
	case "dtmf":
		if callID == nil || !uuid.MatchString(*callID) || !regexp.MustCompile(`^[0-9*#ABCD]$`).MatchString(payload) {
			return nil, invalid
		}
	default:
		return nil, invalid
	}
	var r Receipt
	err := c.exchange(ctx, id, map[string]any{"op": "prepare_call", "line_epoch": line, "idempotency_key": key, "action": action, "call_id": callID, "payload": payload}, key, true, &r)
	if err != nil {
		return nil, err
	}
	if validateReceipt(&r, "", line) != nil || r.Lease != nil || (callID != nil && (r.Operation.CallID == nil || *r.Operation.CallID != *callID)) {
		return nil, &Failure{"invalid_reply", true, key}
	}
	return &r, nil
}
func (c *Client) Submit(ctx context.Context, id, operationID string, lease *Lease, urgent bool) (*Receipt, error) {
	return c.operation(ctx, id, "submit_call", operationID, lease, urgent)
}
func (c *Client) Operation(ctx context.Context, id, operationID string) (*Receipt, error) {
	return c.operation(ctx, id, "call_operation", operationID, nil, false)
}
func (c *Client) Cancel(ctx context.Context, id, operationID string) (*Receipt, error) {
	return c.operation(ctx, id, "cancel_prepared_call", operationID, nil, false)
}
func (c *Client) Renew(ctx context.Context, id string, lease Lease) (*Lease, error) {
	if !lease.valid() {
		return nil, invalid
	}
	var r struct {
		Lease *Lease `json:"lease"`
	}
	if err := c.exchange(ctx, id, map[string]any{"op": "renew_call", "lease": lease}, lease.CallID, true, &r); err != nil {
		return nil, err
	}
	if !r.Lease.valid() || r.Lease.CallID != lease.CallID || r.Lease.Boot != lease.Boot || r.Lease.Line != lease.Line || r.Lease.Generation != lease.Generation {
		return nil, &Failure{"invalid_reply", true, lease.CallID}
	}
	return r.Lease, nil
}
