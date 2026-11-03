// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"crypto/ed25519"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"syscall"
	"testing"
)

func sample() map[string]any {
	return map[string]any{"format": 1, "generation": 7, "core_uid": 1001, "engine_uid": 1002,
		"seed": strings.Repeat("23", 32), "core_pin": strings.Repeat("12", 32), "statistics_public_key": strings.Repeat("34", 32),
		"engine_sha256": strings.Repeat("45", 32), "policy_sha256": strings.Repeat("56", 32),
		"seed_rpc": []string{"https://seed.example.invalid:443"}, "dns_servers": []string{"9.9.9.9:53"}, "mode": "public"}
}
func encoded(t *testing.T, v map[string]any) []byte {
	t.Helper()
	b, e := json.Marshal(v)
	if e != nil {
		t.Fatal(e)
	}
	return b
}
func TestLaunchStrictPrivateFormatAndNoFallback(t *testing.T) {
	v := sample()
	l, e := parseLaunch(encoded(t, v))
	if e != nil || l.generation != 7 {
		t.Fatal("valid bounded private launch rejected")
	}
	for _, text := range []string{fmt.Sprintf("%v", l), fmt.Sprintf("%#v", l), fmt.Sprintf("%+v", *l)} {
		if strings.Contains(text, v["seed"].(string)) || text != "managed launch(private)" {
			t.Fatal("private launch escaped formatting")
		}
	}
	for key, value := range map[string]any{"format": 2, "generation": 0, "core_uid": 0, "engine_uid": 1001, "seed": strings.Repeat("00", 32), "engine_sha256": "bad", "policy_sha256": strings.Repeat("00", 32), "seed_rpc": []string{"http://public.example.invalid"}, "dns_servers": []string{"9.9.9.9:443"}, "mode": "ambient", "unknown": "private"} {
		v := sample()
		v[key] = value
		if _, e := parseLaunch(encoded(t, v)); e == nil {
			t.Fatalf("unsafe field accepted: %s", key)
		}
	}
	for _, suffix := range []string{`,"seed":"` + strings.Repeat("23", 32) + `"}`, `,"s\u0065ed":"` + strings.Repeat("23", 32) + `"}`} {
		data := encoded(t, sample())
		data = append(data[:len(data)-1], []byte(suffix)...)
		if _, e := parseLaunch(data); e == nil {
			t.Fatal("duplicate/aliased credential accepted")
		}
	}
	data := append(encoded(t, sample()), []byte(` {"format":1}`)...)
	if _, e := parseLaunch(data); e == nil {
		t.Fatal("trailing object accepted")
	}
}
func TestLaunchRejectsTransportIdentityReuseAndUnboundedOrPrivateURL(t *testing.T) {
	v := sample()
	seed, _ := hex.DecodeString(v["seed"].(string))
	private := ed25519.NewKeyFromSeed(seed)
	public := hex.EncodeToString(private.Public().(ed25519.PublicKey))
	clear(private)
	clear(seed)
	for _, key := range []string{"core_pin", "statistics_public_key"} {
		v := sample()
		v[key] = public
		if _, e := parseLaunch(encoded(t, v)); e == nil {
			t.Fatal("transport identity reused business/statistics key")
		}
	}
	for _, url := range []string{"https://user:secret@seed.invalid", "https://seed.invalid/?token=secret", "https://seed.invalid/#secret", "file:///tmp/secret", "https://seed.invalid/hidden/path", strings.Repeat("x", 257)} {
		v := sample()
		v["seed_rpc"] = []string{url}
		if _, e := parseLaunch(encoded(t, v)); e == nil {
			t.Fatal("unsupported credential endpoint accepted")
		}
	}
	v = sample()
	v["mode"] = "isolated-loopback-fixture"
	if !fixtureBuild {
		if _, e := parseLaunch(encoded(t, v)); e == nil {
			t.Fatal("fixture route enabled in production build")
		}
	}
}
func TestRootLaunchCannotBeForgedWithCurrentUserSocketOrPublicStdio(t *testing.T) {
	read, write, e := os.Pipe()
	if e != nil {
		t.Fatal(e)
	}
	defer read.Close()
	defer write.Close()
	fds, e := syscall.Socketpair(syscall.AF_UNIX, syscall.SOCK_STREAM, 0)
	if e != nil {
		t.Fatal(e)
	}
	a := os.NewFile(uintptr(fds[0]), "owned-fixture-control")
	b := os.NewFile(uintptr(fds[1]), "owned-fixture-peer")
	defer a.Close()
	defer b.Close()
	if _, e := LoadRootLease(read, a); e == nil {
		t.Fatal("non-Root credential channel started managed launch")
	}
	// There were no reads from the anonymous seed pipe before kernel authority
	// was rejected, and no SDK/network dependency is present in this test.
	if _, e := write.Write([]byte("private-not-consumed")); e != nil {
		t.Fatal(e)
	}
	buf := make([]byte, len("private-not-consumed"))
	if _, e := read.Read(buf); e != nil || string(buf) != "private-not-consumed" {
		t.Fatal("unauthorized preflight consumed secret bytes")
	}
}
