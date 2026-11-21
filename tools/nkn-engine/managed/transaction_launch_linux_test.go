// SPDX-License-Identifier: AGPL-3.0-only
package managed

import "testing"

func TestPrivateLaunchRequiresExplicitBuildAndKnownTransactionVersion(t *testing.T) {
	v := sample()
	l, e := parseLaunch(encoded(t, v))
	if e != nil || l.transactions != 0 {
		t.Fatal("old12-field launch changed")
	}
	v["usage_transaction_protocol"] = 1
	l, e = parseLaunch(encoded(t, v))
	if UsageTransactionProtocol() == 1 {
		if e != nil || l.transactions != 1 {
			t.Fatal("compiled private capability refused")
		}
	} else if e == nil {
		t.Fatal("uncompiled capability accepted")
	}
	for _, bad := range []any{0, 2, nil, "1", 1.5} {
		v := sample()
		v["usage_transaction_protocol"] = bad
		if _, e := parseLaunch(encoded(t, v)); e == nil {
			t.Fatal("invalid optional launch capability accepted", bad)
		}
	}
}
