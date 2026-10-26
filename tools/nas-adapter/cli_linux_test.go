// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"encoding/pem"
	"errors"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
)

// Exercises the real CLI entry and Linux private-file checks using temporary
// synthetic keys/config and a formal-contract loopback TLS fixture, never a NAS.
func TestCLISyntheticConfigAndRead(t *testing.T) {
	seed := bytes.Repeat([]byte{42}, 32)
	key := ed25519.NewKeyFromSeed(seed)
	device := bytes.Repeat([]byte{43}, 32)
	epoch := bytes.Repeat([]byte{44}, 32)
	var count atomic.Int32
	var reject atomic.Bool
	s, _ := tlsServer(t, func(w http.ResponseWriter, r *http.Request) {
		count.Add(1)
		if r.Method != "POST" || r.Header.Get("X-RSH-Legacy-Compat") != "1" {
			t.Error("invalid CLI request")
		}
		body, _ := io.ReadAll(r.Body)
		if r.URL.Path == "/v1/session" {
			canonical := canonical("POST", r.URL.Path, r.Header.Get("X-NSH-Timestamp"), r.Header.Get("X-NSH-Nonce"), body)
			sig, _ := base64.StdEncoding.DecodeString(r.Header.Get("X-NSH-Signature"))
			proof, _ := base64.StdEncoding.DecodeString(r.Header.Get("X-RSH-Device-Proof"))
			if !ed25519.Verify(key.Public().(ed25519.PublicKey), canonical, sig) || !ed25519.Verify(key.Public().(ed25519.PublicKey), bound(device, epoch, canonical), proof) {
				t.Error("signature/device binding changed")
			}
			if r.Header.Get("X-RSH-Binding-Epoch") != hex.EncodeToString(epoch) || string(body) != `{"core_id":"synthetic-nas"}` {
				t.Error("binding changed")
			}
			io.WriteString(w, `{"token":"`+testToken+`","expires_at":"2026-10-26T14:00:00Z"}`)
			return
		}
		if r.URL.Path != "/compat/v1/read" || r.Header.Get("Authorization") != "Bearer "+testToken {
			t.Error("wrong read path/authority")
		}
		if reject.Load() {
			w.WriteHeader(403)
			io.WriteString(w, `{"code":"permission_denied","retryable":false,"adapter_required":true}`)
			return
		}
		var q Query
		if strictJSON(body, &q) != nil {
			t.Error("bad query")
			return
		}
		summary := SmsSummary{ID: testID, Line: 9}
		switch q.Entity {
		case "status":
			w.Write(envelope(LineView{Line: 9, Status: "ready"}))
		case "sms":
			w.Write(envelope(SmsPage{Messages: []SmsSummary{summary}, Cursor: Cursor{testEpoch, "1"}}))
		case "sms_detail":
			w.Write(envelope(SmsDetail{Summary: summary, Text: "synthetic", Data: []uint16{}}))
		case "calls":
			w.Write(envelope(CallPage{Calls: []CallSummary{}, Cursor: Cursor{testEpoch, "0"}}))
		default:
			t.Error("unsupported entity sent")
		}
	})
	dir := t.TempDir()
	if err := os.Chmod(dir, 0700); err != nil {
		t.Fatal(err)
	}
	dir, _ = filepath.EvalSymlinks(dir)
	write := func(name string, raw []byte) string {
		path := filepath.Join(dir, name)
		if err := os.WriteFile(path, raw, 0600); err != nil {
			t.Fatal(err)
		}
		return path
	}
	ca := write("ca.pem", pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: s.Certificate().Raw}))
	seedPath := write("nas-seed", seed)
	cfg, _ := json.Marshal(config{URL: s.URL, CAFile: ca, DevicePublicKey: hex.EncodeToString(device), CoreID: "synthetic-nas", BindingEpoch: hex.EncodeToString(epoch), KeyFile: seedPath})
	configPath := write("config.json", cfg)
	var output bytes.Buffer
	if err := runCLI([]string{"nas-adapter", "session", configPath}, strings.NewReader(""), &output); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), testToken) {
		t.Fatal("no synthetic session")
	}
	for _, entity := range []string{"status", "sms", "sms_detail", "calls"} {
		q := Query{Entity: entity, Limit: 1}
		if entity == "sms_detail" {
			q.RecordID = strptr(testID)
		}
		input, _ := json.Marshal(struct {
			Token string `json:"token"`
			Query Query  `json:"query"`
		}{testToken, q})
		output.Reset()
		if err := runCLI([]string{"nas-adapter", "read", configPath}, bytes.NewReader(input), &output); err != nil {
			t.Fatal(err)
		}
		if strings.Contains(output.String(), testToken) || !strings.Contains(output.String(), `"adapter_api":1`) {
			t.Fatal("read leaked token or lost contract")
		}
	}
	if count.Load() != 5 {
		t.Fatal("unexpected retry/request")
	}
	// Also build and invoke the actual executable, including main's stdin,
	// stdout/stderr and exit-code behavior, using the same synthetic config.
	binary := filepath.Join(dir, "nas-adapter")
	build := exec.Command("go", "build", "-o", binary, ".")
	if raw, err := build.CombinedOutput(); err != nil {
		t.Fatalf("build CLI: %v\n%s", err, raw)
	}
	command := exec.Command(binary, "read", configPath)
	command.Stdin = strings.NewReader(`{"token":"` + testToken + `","query":{"entity":"status","limit":1}}`)
	raw, err := command.Output()
	if err != nil || !bytes.Contains(raw, []byte(`"status":"ready"`)) || bytes.Contains(raw, []byte(testToken)) || count.Load() != 6 {
		t.Fatalf("executable read failed: %v", err)
	}
	reject.Store(true)
	output.Reset()
	err = runCLI([]string{"nas-adapter", "read", configPath}, strings.NewReader(`{"token":"`+testToken+`","query":{"entity":"status","limit":1}}`), &output)
	requireFailure(t, err, "forbidden")
	if output.Len() != 0 || count.Load() != 7 {
		t.Fatal("failure published output/retried")
	}
	if err := os.Chmod(configPath, 0644); err != nil {
		t.Fatal(err)
	}
	if err := runCLI([]string{"nas-adapter", "read", configPath}, strings.NewReader(""), &output); err == nil || count.Load() != 7 {
		t.Fatal("public config accepted")
	}
	command = exec.Command(binary, "read", configPath)
	raw, err = command.Output()
	var exit *exec.ExitError
	if !errors.As(err, &exit) || exit.ExitCode() != 1 || len(raw) != 0 || bytes.Contains(exit.Stderr, []byte(testToken)) || count.Load() != 7 {
		t.Fatal("executable did not reject public config without leaking/retrying")
	}
}
