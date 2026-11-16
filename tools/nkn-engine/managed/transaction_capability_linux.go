// SPDX-License-Identifier: AGPL-3.0-only
//go:build !nkn_sdk_candidate || !nkn_usage_transaction_candidate

package managed

func UsageTransactionProtocol() uint8 { return 0 }
