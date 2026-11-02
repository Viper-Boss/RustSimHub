// SPDX-License-Identifier: AGPL-3.0-only
// Independent Go standard-library reference for the Rust compatibility vector.
// Algorithm source: nknguard c32952d7 pkg/membership and pkg/identity, AGPL-3.0.
package main

import (
	"bytes"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base32"
	"encoding/hex"
	"encoding/json"
	"os"
)

func main() {
	e := base32.NewEncoding("abcdefghijklmnopqrstuvwxyz234567").WithPadding(base32.NoPadding)
	network := "nkgnet_" + e.EncodeToString(bytes.Repeat([]byte{5}, 16))
	secret := bytes.Repeat([]byte{7}, 32)
	root, _ := hex.DecodeString("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
	m := hmac.New(sha256.New, secret)
	m.Write([]byte("nknguard/v1/membership-proof"))
	m.Write([]byte{0})
	m.Write([]byte(network))
	m.Write([]byte{1})
	derived := m.Sum(nil)
	hash := sha256.Sum256(root)
	device := "nkg_" + e.EncodeToString(hash[:10])
	m = hmac.New(sha256.New, derived)
	m.Write([]byte(network))
	m.Write([]byte{0})
	m.Write([]byte(device))
	m.Write([]byte{0})
	m.Write(root)
	json.NewEncoder(os.Stdout).Encode(map[string]string{"network": network, "device": device, "proof": hex.EncodeToString(m.Sum(nil))})
}
