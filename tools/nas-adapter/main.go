// SPDX-License-Identifier: AGPL-3.0-only
// Minimal NAS bridge client. The legacy signing format is protocol data; no owner/device keys are imported.
package main

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"crypto/tls"
	"crypto/x509"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"time"
)

type config struct {
	URL             string `json:"url"`
	CAFile          string `json:"ca_file"`
	DevicePublicKey string `json:"device_public_key"`
	CoreID          string `json:"core_id"`
	BindingEpoch    string `json:"binding_epoch"`
	KeyFile         string `json:"key_file"`
}

func private(path string, maximum int64) ([]byte, error) {
	if !filepath.IsAbs(path) {
		return nil, errors.New("private_file_required")
	}
	resolved, e := filepath.EvalSymlinks(path)
	if e != nil || resolved != path {
		return nil, errors.New("private_file_required")
	}
	parent, e := os.Stat(filepath.Dir(path))
	if e != nil || !parent.IsDir() || parent.Mode().Perm()&0077 != 0 {
		return nil, errors.New("private_file_required")
	}
	ps, ok := parent.Sys().(*syscall.Stat_t)
	if !ok || int(ps.Uid) != os.Geteuid() {
		return nil, errors.New("private_file_required")
	}
	fd, e := syscall.Open(path, syscall.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if e != nil {
		return nil, errors.New("private_file_required")
	}
	f := os.NewFile(uintptr(fd), path)
	defer f.Close()
	m, e := f.Stat()
	if e != nil || !m.Mode().IsRegular() || m.Mode().Perm()&0077 != 0 || m.Size() > maximum {
		return nil, errors.New("private_file_required")
	}
	st, ok := m.Sys().(*syscall.Stat_t)
	if !ok || int(st.Uid) != os.Geteuid() || st.Nlink != 1 {
		return nil, errors.New("private_file_required")
	}
	return io.ReadAll(io.LimitReader(f, maximum+1))
}
func canonical(method, path, stamp, nonce string, body []byte) []byte {
	sum := sha256.Sum256(body)
	return []byte(strings.Join([]string{strings.ToUpper(method), path, stamp, nonce, base64.StdEncoding.EncodeToString(sum[:])}, "\n"))
}
func bound(device, epoch, request []byte) []byte {
	proof := append([]byte("RustSimHub.LegacyDeviceProof.v1\x00"), device...)
	proof = append(proof, epoch...)
	return append(proof, request...)
}
func run() error {
	if len(os.Args) != 3 || (os.Args[1] != "session" && os.Args[1] != "read") {
		return errors.New("usage_session_or_read_private_config")
	}
	raw, e := private(os.Args[2], 8192)
	if e != nil {
		return e
	}
	var cfg config
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if e = decoder.Decode(&cfg); e != nil {
		return errors.New("invalid_config")
	}
	target, e := url.Parse(cfg.URL)
	if e != nil || target.Scheme != "https" || target.User != nil || target.RawQuery != "" || target.Fragment != "" || (target.Path != "" && target.Path != "/") {
		return errors.New("explicit_https_origin_required")
	}
	ip := net.ParseIP(target.Hostname())
	if ip == nil || !(ip.IsPrivate() || ip.IsLoopback() || ip.IsLinkLocalUnicast()) {
		return errors.New("explicit_local_address_required")
	}
	device, e := hex.DecodeString(cfg.DevicePublicKey)
	if e != nil || len(device) != 32 {
		return errors.New("invalid_device_pin")
	}
	epoch, e := hex.DecodeString(cfg.BindingEpoch)
	if e != nil || len(epoch) != 32 {
		return errors.New("invalid_binding")
	}
	if cfg.CoreID == "" || len(cfg.CoreID) > 128 || strings.ContainsAny(cfg.CoreID, "\r\n") {
		return errors.New("invalid_binding")
	}
	pem, e := os.ReadFile(cfg.CAFile)
	if e != nil {
		return errors.New("trusted_ca_required")
	}
	roots := x509.NewCertPool()
	if !roots.AppendCertsFromPEM(pem) {
		return errors.New("trusted_ca_required")
	}
	transport := &http.Transport{Proxy: nil, TLSClientConfig: &tls.Config{RootCAs: roots, MinVersion: tls.VersionTLS13}, DisableKeepAlives: true, MaxConnsPerHost: 1, TLSHandshakeTimeout: 5 * time.Second, ResponseHeaderTimeout: 10 * time.Second}
	client := &http.Client{Transport: transport, Timeout: 15 * time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return errors.New("redirect_denied") }}
	nonceBytes := make([]byte, 32)
	if _, e = rand.Read(nonceBytes); e != nil {
		return errors.New("entropy_unavailable")
	}
	nonce := hex.EncodeToString(nonceBytes)
	var path string
	var body []byte
	var token string
	headers := map[string]string{"X-RSH-Legacy-Compat": "1", "X-RSH-Request-Id": nonce}
	if os.Args[1] == "session" {
		seed, e := private(cfg.KeyFile, 32)
		if e != nil || len(seed) != 32 {
			return errors.New("private_nas_seed_required")
		}
		key := ed25519.NewKeyFromSeed(seed)
		for i := range seed {
			seed[i] = 0
		}
		defer func() {
			for i := range key {
				key[i] = 0
			}
		}()
		path = "/v1/session"
		body, _ = json.Marshal(struct {
			CoreID string `json:"core_id"`
		}{cfg.CoreID})
		stamp := time.Now().UTC().Format(time.RFC3339)
		original := canonical("POST", path, stamp, nonce, body)
		id := sha256.Sum256(device)
		headers["X-NSH-Core-Id"] = cfg.CoreID
		headers["X-NSH-Device-Id"] = fmt.Sprintf("NSH-410-%X", id[:3])
		headers["X-NSH-Timestamp"] = stamp
		headers["X-NSH-Nonce"] = nonce
		headers["X-NSH-Signature"] = base64.StdEncoding.EncodeToString(ed25519.Sign(key, original))
		headers["X-RSH-Device-Proof"] = base64.StdEncoding.EncodeToString(ed25519.Sign(key, bound(device, epoch, original)))
		headers["X-RSH-Binding-Epoch"] = cfg.BindingEpoch
	} else {
		path = "/compat/v1/read"
		input, e := io.ReadAll(io.LimitReader(os.Stdin, 8193))
		if e != nil || len(input) > 8192 {
			return errors.New("invalid_query")
		}
		var request struct {
			Token string          `json:"token"`
			Query json.RawMessage `json:"query"`
		}
		d := json.NewDecoder(bytes.NewReader(input))
		d.DisallowUnknownFields()
		if d.Decode(&request) != nil || len(request.Token) != 64 || len(request.Query) == 0 {
			return errors.New("invalid_query")
		}
		token = request.Token
		body = request.Query
		headers["Authorization"] = "Bearer " + token
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	request, e := http.NewRequestWithContext(ctx, http.MethodPost, strings.TrimSuffix(cfg.URL, "/")+path, io.NopCloser(bytes.NewReader(body)))
	if e != nil {
		return errors.New("invalid_request")
	}
	request.ContentLength = int64(len(body))
	request.Header.Set("Content-Type", "application/json")
	for k, v := range headers {
		request.Header.Set(k, v)
	}
	response, e := client.Do(request)
	if e != nil {
		return errors.New("transport_failed")
	}
	defer response.Body.Close()
	data, e := io.ReadAll(io.LimitReader(response.Body, 65537))
	if e != nil || len(data) > 65536 {
		return errors.New("invalid_reply")
	}
	if response.StatusCode != http.StatusOK {
		var failure struct {
			Code string `json:"code"`
		}
		if json.Unmarshal(data, &failure) == nil && failure.Code != "" {
			return errors.New(failure.Code)
		}
		return errors.New("request_rejected")
	}
	if !json.Valid(data) {
		return errors.New("invalid_reply")
	}
	_, e = os.Stdout.Write(append(data, '\n'))
	return e
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "nas-adapter:", err.Error())
		os.Exit(1)
	}
}
