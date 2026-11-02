// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"encoding/binary"
	"encoding/json"
	"net"
	"net/netip"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func networkFixture(t *testing.T) []byte {
	t.Helper()
	data, e := json.Marshal(map[string]any{"schema": 1, "available": true, "boot_id": "owned-fixture-boot", "sample_unix_seconds": 100,
		"devices": []map[string]any{{"interface": "wlan0", "kind": "wifi", "state": "100 (connected)", "ipv4": []string{"192.168.1.2/24"}, "ipv6": []string{}, "dns_ipv4": []string{"192.168.1.1"}, "dns_ipv6": []string{}}}})
	if e != nil {
		t.Fatal(e)
	}
	return data
}
func TestRootRouteRequiresFreshBootExactInterfaceAndFixedDNS(t *testing.T) {
	raw := networkFixture(t)
	r, e := selectRoute(raw, "wlan0", "owned-fixture-boot", 110)
	if e != nil || len(r.dns) != 1 || r.dns[0].String() != "192.168.1.1:53" {
		t.Fatal("fixed route refused")
	}
	for _, args := range []struct {
		iface, boot string
		at          uint64
	}{{"usb0", "owned-fixture-boot", 110}, {"wwan0", "owned-fixture-boot", 110}, {"wlan0", "other-boot", 110}, {"wlan0", "owned-fixture-boot", 131}, {"wlan0", "owned-fixture-boot", 99}} {
		if _, e := selectRoute(raw, args.iface, args.boot, args.at); e == nil {
			t.Fatal("invalid route proof accepted")
		}
	}
	for _, change := range []func(map[string]any){
		func(v map[string]any) { v["available"] = false }, func(v map[string]any) { devices := v["devices"].([]any); v["devices"] = append(devices, devices[0]) },
		func(v map[string]any) { v["devices"].([]any)[0].(map[string]any)["kind"] = "cellular" },
		func(v map[string]any) { v["devices"].([]any)[0].(map[string]any)["dns_ipv4"] = []string{"127.0.0.53"} },
		func(v map[string]any) { v["devices"].([]any)[0].(map[string]any)["ipv4"] = []string{"0.0.0.0/0"} },
	} {
		var v map[string]any
		json.Unmarshal(raw, &v)
		change(v)
		bad, _ := json.Marshal(v)
		if _, e := selectRoute(bad, "wlan0", "owned-fixture-boot", 110); e == nil {
			t.Fatal("untrusted route fallback accepted")
		}
	}
}
func TestRootEgressScriptScopesAllAllowsToUIDAndInterface(t *testing.T) {
	r := route{iface: "wlan0", index: 3, dns: []netip.AddrPort{netip.MustParseAddrPort("192.168.1.1:53")}}
	b, e := ruleScript(1002, r)
	if e != nil {
		t.Fatal(e)
	}
	text := string(b)
	fence := strings.Index(text, `meta skuid 1002 oifname != "wlan0" drop`)
	allow := strings.Index(text, "daddr 192.168.1.1")
	if fence < 0 || allow < fence || !strings.HasSuffix(text, "meta skuid 1002 drop\n") || strings.Contains(text, "ct state established") {
		t.Fatal("egress bypass exists")
	}
	for _, line := range strings.Split(strings.TrimSpace(text), "\n") {
		if strings.HasPrefix(line, "add rule") && !strings.Contains(line, "meta skuid 1002 ") {
			t.Fatal("unscoped rule")
		}
	}
	for _, bad := range []route{{iface: "wwan0", index: 3, dns: r.dns}, {iface: "wlan0\";flush ruleset", index: 3, dns: r.dns}, {iface: "wlan0", index: 3, dns: []netip.AddrPort{netip.MustParseAddrPort("127.0.0.1:53")}}, {iface: "wlan0", index: 3, dns: []netip.AddrPort{netip.MustParseAddrPort("9.9.9.9:443")}}} {
		if _, e := ruleScript(1002, bad); e == nil {
			t.Fatal("nft grammar/route injection accepted")
		}
	}
	if _, e := ruleScript(0, r); e == nil {
		t.Fatal("Root egress policy accepted")
	}
}
func TestRootPrivateProtocolIsExactAndCannotReplaceOwnedOperation(t *testing.T) {
	b := make([]byte, requestBytes)
	copy(b, []byte("RSCN\x01"))
	b[5] = status
	binary.BigEndian.PutUint32(b[6:10], 7)
	binary.BigEndian.PutUint64(b[10:18], 1000)
	b[18] = 1
	b[50] = 2
	r, e := parseRequest(b)
	if e != nil {
		t.Fatal(e)
	}
	s := &registration{request: r}
	if !s.matches(r) {
		t.Fatal("original operation lost")
	}
	for _, offset := range []int{0, 4, 5, 6, 10, 18, 50} {
		changed := append([]byte(nil), b...)
		changed[offset] ^= 128
		if parsed, e := parseRequest(changed); e == nil && s.matches(parsed) {
			t.Fatal("different proof adopted")
		}
	}
	for _, invalid := range [][]byte{b[:len(b)-1], append(append([]byte(nil), b...), 0), []byte(`{"op":"start","root":true}`)} {
		if _, e := parseRequest(invalid); e == nil {
			t.Fatal("public or ambiguous request accepted")
		}
	}
	m := &manager{policy: &Policy{hash: r.policy}}
	if _, e := m.handle(r, nil); e == nil {
		t.Fatal("status created a Root scope")
	}
	if m.scope != nil {
		t.Fatal("read-only request allocated authority")
	}
}
func TestRootSocketChecksActualKernelCoreUIDBeforeReading(t *testing.T) {
	path := filepath.Join(t.TempDir(), "owned-root-socket")
	listener, e := net.ListenUnix("unix", &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	defer listener.Close()
	client, e := net.DialUnix("unix", nil, &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	defer client.Close()
	server, e := listener.AcceptUnix()
	if e != nil {
		t.Fatal(e)
	}
	defer server.Close()
	if !peerIsCore(server, uint32(os.Geteuid())) || peerIsCore(server, uint32(os.Geteuid())+1) {
		t.Fatal("kernel source UID was not enforced")
	}
	if os.Geteuid() != 0 {
		if _, e := LoadInstalled(); e == nil {
			t.Fatal("low UID loaded Root policy")
		}
	}
}
