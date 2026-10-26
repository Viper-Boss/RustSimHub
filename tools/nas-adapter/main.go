// SPDX-License-Identifier: AGPL-3.0-only
// Minimal NAS bridge client. The legacy signing format is protocol data; no owner/device keys are imported.
package main

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"strings"
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

func canonical(method, path, stamp, nonce string, body []byte) []byte {
	sum := sha256.Sum256(body)
	return []byte(strings.Join([]string{strings.ToUpper(method), path, stamp, nonce, base64.StdEncoding.EncodeToString(sum[:])}, "\n"))
}
func bound(device, epoch, request []byte) []byte {
	proof := append([]byte("RustSimHub.LegacyDeviceProof.v1\x00"), device...)
	proof = append(proof, epoch...)
	return append(proof, request...)
}
func runCLI(args []string, input io.Reader, output io.Writer) error {
	if len(args) != 3 || (args[1] != "session" && args[1] != "read") {
		return errors.New("usage_session_or_read_private_config")
	}
	raw, e := private(args[2], 8192)
	if e != nil {
		return e
	}
	var cfg config
	if e = strictJSON(raw, &cfg); e != nil {
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
	if e != nil || len(epoch) != 32 || !hexToken(cfg.BindingEpoch) {
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
	client := newHTTPClient(roots)
	defer client.CloseIdleConnections()
	nonceBytes := make([]byte, 32)
	if _, e = rand.Read(nonceBytes); e != nil {
		return errors.New("entropy_unavailable")
	}
	nonce := hex.EncodeToString(nonceBytes)
	var path string
	var body []byte
	headers := map[string]string{"X-RSH-Legacy-Compat": "1", "X-RSH-Request-Id": nonce}
	if args[1] == "session" {
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
		rawInput, err := io.ReadAll(io.LimitReader(input, 8193))
		if err != nil {
			return &Failure{Kind: "invalid_query"}
		}
		token, query, err := readInput(rawInput)
		if err != nil {
			return err
		}
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		reply, err := readOnce(ctx, client, cfg.URL, token, nonce, query)
		if err != nil {
			return err
		}
		return json.NewEncoder(output).Encode(reply)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	data, e := exchange(ctx, client, cfg.URL, path, headers, body)
	if e != nil {
		return e
	}
	// Session minting is the sole output allowed to contain the new bearer.
	var session struct {
		Token     string `json:"token"`
		ExpiresAt string `json:"expires_at"`
	}
	if strictJSON(data, &session) != nil || !hexToken(session.Token) {
		return invalidReply()
	}
	if _, e := time.Parse(time.RFC3339, session.ExpiresAt); e != nil {
		return invalidReply()
	}
	return json.NewEncoder(output).Encode(session)
}
func run() error { return runCLI(os.Args, os.Stdin, os.Stdout) }

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "nas-adapter:", err.Error())
		os.Exit(1)
	}
}
