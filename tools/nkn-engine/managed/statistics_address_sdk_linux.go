// SPDX-License-Identifier: AGPL-3.0-only
//go:build linux && nkn_sdk_candidate

package managed

import "github.com/nknorg/nkn/v2/common"

// Use the fixed upstream NKN address algorithm (including its unusual base58
// decimal-text step), never a generic Bitcoin base58check approximation.
func statisticsAddress(public [32]byte) (string, error) {
	if public == [32]byte{} {
		return "", ErrLaunch
	}
	code := append(append([]byte{32}, public[:]...), 0xac)
	hash, err := common.ToCodeHash(code)
	if err != nil {
		return "", ErrLaunch
	}
	address, err := hash.ToAddress()
	if err != nil || address == "" || len(address) > 128 {
		return "", ErrLaunch
	}
	return address, nil
}
