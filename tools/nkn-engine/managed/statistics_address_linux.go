// SPDX-License-Identifier: AGPL-3.0-only
//go:build linux && !nkn_sdk_candidate

package managed

// A bare/default engine cannot construct a transaction RPC backend.
func statisticsAddress([32]byte) (string, error) { return "", ErrLaunch }
