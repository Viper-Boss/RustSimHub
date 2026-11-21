// SPDX-License-Identifier: AGPL-3.0-only
//go:build linux && nkn_sdk_candidate

package managed

import (
	"crypto/sha256"
	"testing"

	nkn "github.com/nknorg/nkn-sdk-go"
)

func TestStatisticsAddressMatchesFixedSDKAccount(t *testing.T) {
	entropy := make([]byte, 32)
	for i := range entropy {
		entropy[i] = byte(i)
	}
	seed := sha256.Sum256(append([]byte("RustSimHub/usage-stats/ed25519-seed/v1\x00"), entropy...))
	account, err := nkn.NewAccount(seed[:])
	if err != nil {
		t.Fatal(err)
	}
	wallet, err := nkn.NewWallet(account, nil)
	if err != nil {
		t.Fatal(err)
	}
	var public [32]byte
	copy(public[:], wallet.PubKey())
	address, err := statisticsAddress(public)
	if err != nil || address != wallet.Address() {
		t.Fatal("address differs from actual fixed SDK", address, err)
	}
	if _, err := statisticsAddress([32]byte{}); err == nil {
		t.Fatal("zero identity accepted")
	}
}
