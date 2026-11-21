// SPDX-License-Identifier: AGPL-3.0-only
//go:build linux && !nkn_sdk_candidate

package managed

import (
	"net/http"
	"sync/atomic"
	"testing"
)

func TestStatisticsDefaultBuildCannotConstructRPCBackend(t *testing.T) {
	var calls atomic.Int32
	c, _ := transactionTLS(t, func(w http.ResponseWriter, r *http.Request) { calls.Add(1) })
	dial := &scopedDialer{lease: c.lease}
	if backend, err := newUsageTransactions(c.lease, dial); err == nil || backend != nil || calls.Load() != 0 {
		t.Fatal("bare/default SDK backend became available")
	}
}
