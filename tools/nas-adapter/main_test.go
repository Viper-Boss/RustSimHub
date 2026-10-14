// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"os"
	"testing"
)

func TestOriginalGoSigningVectors(t *testing.T) {
	var fixture struct {
		Signing []struct {
			Method    string `json:"method"`
			Path      string `json:"path"`
			Timestamp string `json:"timestamp"`
			Nonce     string `json:"nonce"`
			Body      string `json:"body_hex"`
			Canonical string `json:"canonical_hex"`
			Public    string `json:"public_hex"`
			Signature string `json:"signature"`
		} `json:"signing"`
	}
	data, e := os.ReadFile(os.Getenv("RSH_GOLDEN_FILE"))
	if e != nil {
		t.Fatal(e)
	}
	if json.Unmarshal(data, &fixture) != nil || len(fixture.Signing) != 5 {
		t.Fatal("invalid fixture")
	}
	for _, v := range fixture.Signing {
		body, e := hex.DecodeString(v.Body)
		if e != nil {
			t.Fatal(e)
		}
		original := canonical(v.Method, v.Path, v.Timestamp, v.Nonce, body)
		if hex.EncodeToString(original) != v.Canonical {
			t.Fatal("legacy canonical mismatch")
		}
		public, _ := hex.DecodeString(v.Public)
		signature, _ := base64.StdEncoding.DecodeString(v.Signature)
		if !ed25519.Verify(public, original, signature) {
			t.Fatal("legacy signature mismatch")
		}
	}
}
func TestDeviceProofSeparatesDeviceAndBinding(t *testing.T) {
	device := bytes.Repeat([]byte{77}, 32)
	epoch := bytes.Repeat([]byte{11}, 32)
	request := canonical("POST", "/v1/session", "2026-10-14T02:00:00Z", "synthetic-nonce", []byte(`{"core_id":"synthetic"}`))
	seed := bytes.Repeat([]byte{77}, 32)
	key := ed25519.NewKeyFromSeed(seed)
	original := bound(device, epoch, request)
	sig := ed25519.Sign(key, original)
	if !ed25519.Verify(key.Public().(ed25519.PublicKey), original, sig) {
		t.Fatal("invalid proof")
	}
	device[0]++
	if ed25519.Verify(key.Public().(ed25519.PublicKey), bound(device, epoch, request), sig) {
		t.Fatal("cross-device accepted")
	}
	device[0]--
	epoch[0]++
	if ed25519.Verify(key.Public().(ed25519.PublicKey), bound(device, epoch, request), sig) {
		t.Fatal("cross-binding accepted")
	}
}
