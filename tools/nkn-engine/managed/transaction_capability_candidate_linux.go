// SPDX-License-Identifier: AGPL-3.0-only
//go:build nkn_sdk_candidate && nkn_usage_transaction_candidate

package managed

// Compile metadata only; the installed policy and original private launch must
// independently enable version 1. This is never an Owner/consent proof.
func UsageTransactionProtocol() uint8 { return 1 }
