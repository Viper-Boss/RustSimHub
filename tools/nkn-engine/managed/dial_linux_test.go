// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"net/netip"
	"testing"
)

func TestFinitePublicAddressPolicyRejectsLANAndEmbeddedDestinations(t *testing.T) {
	for _, text := range []string{"127.0.0.1", "10.1.2.3", "100.64.0.1", "169.254.1.2", "192.168.1.1", "198.18.1.1", "192.0.2.1", "0.0.0.0", "224.0.0.1", "::1", "fe80::1", "fc00::1", "::ffff:127.0.0.1", "64:ff9b::7f00:1", "2002:7f00:1::1", "2001:db8::1", "3fff::1"} {
		if allowedIP(netip.MustParseAddr(text), false) {
			t.Fatal("nonpublic destination accepted")
		}
	}
	for _, text := range []string{"1.1.1.1", "9.9.9.9", "2606:4700:4700::1111"} {
		ip := netip.MustParseAddr(text)
		if !allowedIP(ip, false) || allowedIP(ip, true) {
			t.Fatal("public/fixture policy mixed")
		}
	}
	if !allowedIP(netip.MustParseAddr("127.0.0.1"), true) {
		t.Fatal("isolated loopback fixture unavailable")
	}
}
