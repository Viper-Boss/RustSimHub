// SPDX-License-Identifier: AGPL-3.0-only
// Package rootguard implements the isolated Root-owned SDK process provider.
// Public clients cannot construct Policy or supply a command, UID or route.
package rootguard

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/netip"
	"net/url"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
)

var ErrDenied = errors.New("NKN Root provider unavailable")

const EnginePath = "/opt/rustsimhub/selected/nkn-sdk-engine"
const SocketPath = "/run/rustsimhub-nkn-root/root.sock"
const groupPath = "/sys/fs/cgroup/system.slice/rustsimhub-nkn-root.service/engine"
const seedPath = "/var/lib/rustsimhub/nkn/transport-seed"
const networkPath = "/run/rustsimhub-network/status.json"

type configuration struct {
	Format                   uint8    `json:"format"`
	Source                   string   `json:"source_commit"`
	Executable               string   `json:"engine_sha256"`
	CorePin                  string   `json:"core_pin"`
	StatisticsKey            string   `json:"statistics_public_key"`
	Interface                string   `json:"interface"`
	Seeds                    []string `json:"seed_rpc"`
	StatusProtocol           uint8    `json:"status_protocol,omitempty"`
	UsageQueryProtocol       uint8    `json:"usage_query_protocol,omitempty"`
	UsageTransactionProtocol uint8    `json:"usage_transaction_protocol,omitempty"`
}
type roles struct {
	core, coreGID, engine, engineGID uint32
	all                              [6]uint32
}
type Policy struct {
	config     configuration
	roles      roles
	hash       [32]byte
	executable [32]byte
}

func (*Policy) String() string { return "fixed NKN Root policy" }

func rootDirectory(path string) error {
	if !filepath.IsAbs(path) || filepath.Clean(path) != path {
		return ErrDenied
	}
	for current := path; ; current = filepath.Dir(current) {
		info, e := os.Lstat(current)
		if e != nil || !info.IsDir() || info.Mode().Perm()&0022 != 0 {
			return ErrDenied
		}
		stat, ok := info.Sys().(*syscall.Stat_t)
		if !ok || stat.Uid != 0 {
			return ErrDenied
		}
		if current == "/" {
			return nil
		}
	}
}
func rootFile(path string, limit int, secret bool) ([]byte, error) {
	if rootDirectory(filepath.Dir(path)) != nil {
		return nil, ErrDenied
	}
	fd, e := syscall.Open(path, syscall.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK|syscall.O_CLOEXEC, 0)
	if e != nil {
		return nil, ErrDenied
	}
	f := os.NewFile(uintptr(fd), "fixed-root-file")
	defer f.Close()
	info, e := f.Stat()
	if e != nil || !info.Mode().IsRegular() || info.Size() > int64(limit) || info.Mode().Perm()&0022 != 0 {
		return nil, ErrDenied
	}
	stat, ok := info.Sys().(*syscall.Stat_t)
	if !ok || stat.Uid != 0 || stat.Nlink != 1 || secret && info.Mode().Perm() != 0600 {
		return nil, ErrDenied
	}
	data, e := io.ReadAll(io.LimitReader(f, int64(limit)+1))
	if e != nil || len(data) > limit {
		clear(data)
		return nil, ErrDenied
	}
	return data, nil
}
func hex32(text string) ([32]byte, error) {
	var b [32]byte
	if len(text) != 64 || strings.ToLower(text) != text {
		return b, ErrDenied
	}
	_, e := hex.Decode(b[:], []byte(text))
	if e != nil || b == [32]byte{} {
		return [32]byte{}, ErrDenied
	}
	return b, nil
}
func configurationValid(c configuration) bool {
	if c.UsageTransactionProtocol == 1 && c.StatusProtocol != 1 {
		return false
	}
	if c.UsageTransactionProtocol > 1 || c.UsageQueryProtocol > 1 || c.StatusProtocol > 1 || c.Format != 1 || len(c.Source) != 40 || strings.ToLower(c.Source) != c.Source || c.Interface != "wlan0" && c.Interface != "usb0" || len(c.Seeds) < 1 || len(c.Seeds) > 3 {
		return false
	}
	if _, e := hex.DecodeString(c.Source); e != nil {
		return false
	}
	for _, key := range []string{c.Executable, c.CorePin, c.StatisticsKey} {
		if _, e := hex32(key); e != nil {
			return false
		}
	}
	if c.CorePin == c.StatisticsKey {
		return false
	}
	for _, seed := range c.Seeds {
		u, e := url.Parse(seed)
		if e != nil || u.Scheme != "https" || len(seed) > 256 || u.Hostname() == "" || u.User != nil || u.RawQuery != "" || u.Fragment != "" || (u.Path != "" && u.Path != "/" && seed != protocol.MainnetWalletRPC) || u.Opaque != "" {
			return false
		}
	}
	return true
}
func installedRoles() (roles, error) {
	var result roles
	seen := map[uint32]bool{}
	// NetworkManager/network-agent are Root roles and cannot be the NKN UID.
	// Media, Edge, BLE, forwarding and Core have distinct low-privilege accounts.
	for i, name := range []string{"rustsimhub-core", "rustsimhub-edge", "rustsimhub-forwarder", "rustsimhub-ble", "rustsimhub-media", "rustsimhub-nkn"} {
		u, e := user.Lookup(name)
		if e != nil || u.Username != name {
			return roles{}, ErrDenied
		}
		uid, e := strconv.ParseUint(u.Uid, 10, 32)
		if e != nil || uid == 0 || uid == 1<<32-1 || seen[uint32(uid)] {
			return roles{}, ErrDenied
		}
		gid, e := strconv.ParseUint(u.Gid, 10, 32)
		if e != nil || gid == 0 || gid == 1<<32-1 {
			return roles{}, ErrDenied
		}
		canonical, e := user.LookupId(u.Uid)
		if e != nil || canonical.Username != name {
			return roles{}, ErrDenied
		}
		seen[uint32(uid)] = true
		result.all[i] = uint32(uid)
		if i == 0 {
			result.core = uint32(uid)
			result.coreGID = uint32(gid)
		}
		if i == 5 {
			groups, e := u.GroupIds()
			if e != nil || len(groups) != 1 || groups[0] != u.Gid {
				return roles{}, ErrDenied
			}
			result.engine = uint32(uid)
			result.engineGID = uint32(gid)
		}
	}
	return result, nil
}

// LoadInstalled reads only preinstalled Root-owned files. OFF/status callers
// must not call it to manufacture authority or create a transport identity.
// Missing policy, accounts, selected program or seed fail closed.
func LoadInstalled() (*Policy, error) {
	if os.Getuid() != 0 || os.Geteuid() != 0 {
		return nil, ErrDenied
	}
	raw, e := rootFile("/etc/rustsimhub/nkn-policy.json", 4096, false)
	if e != nil {
		return nil, ErrDenied
	}
	var c configuration
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.DisallowUnknownFields()
	if decoder.Decode(&c) != nil || decoder.Decode(new(any)) != io.EOF || !configurationValid(c) {
		return nil, ErrDenied
	}
	// Requiring canonical bytes rejects duplicate/escaped aliases and alternate
	// encodings. This file is provisioning output, never a browser payload.
	canonical, _ := json.Marshal(c)
	if !bytes.Equal(bytes.TrimSpace(raw), canonical) {
		return nil, ErrDenied
	}
	r, e := installedRoles()
	if e != nil {
		return nil, e
	}
	exe, e := rootFile(EnginePath, 32*1024*1024, false)
	if e != nil {
		return nil, e
	}
	actual := sha256.Sum256(exe)
	clear(exe)
	expected, e := hex32(c.Executable)
	if e != nil || actual != expected {
		return nil, ErrDenied
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, EnginePath, "--version")
	cmd.Env = []string{"LC_ALL=C"}
	var out boundedOutput
	cmd.Stdout = &out
	cmd.Stderr = io.Discard
	if cmd.Run() != nil {
		return nil, ErrDenied
	}
	if !engineVersionValid(out.bytes, c) {
		return nil, ErrDenied
	}
	return &Policy{config: c, roles: r, hash: sha256.Sum256(canonical), executable: actual}, nil
}

func engineVersionValid(raw []byte, c configuration) bool {
	if len(raw) == 0 || len(raw) > 65536 {
		return false
	}
	pre := json.NewDecoder(bytes.NewReader(raw))
	if strictObject(pre, 0) != nil {
		return false
	}
	if _, e := pre.Token(); e != io.EOF {
		return false
	}
	var version struct {
		Contract     uint8  `json:"contract"`
		Source       string `json:"source_commit"`
		SDK          bool   `json:"nkn_sdk_linked"`
		Root         bool   `json:"root_launch_required"`
		Fixture      bool   `json:"isolated_fixture_build"`
		Go           string `json:"go"`
		Transactions uint8  `json:"usage_transaction_protocol,omitempty"`
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if d.Decode(&version) != nil || d.Decode(new(any)) != io.EOF || version.Contract != 1 || version.Source != c.Source || !version.SDK || !version.Root || version.Fixture || version.Go != "go1.27.3" || version.Transactions > 1 || c.UsageTransactionProtocol == 1 && version.Transactions != 1 {
		return false
	}
	return true
}

type boundedOutput struct{ bytes []byte }

func (b *boundedOutput) Write(data []byte) (int, error) {
	if len(b.bytes)+len(data) > 65536 {
		return 0, ErrDenied
	}
	b.bytes = append(b.bytes, data...)
	return len(data), nil
}

type networkSnapshot struct {
	Schema    uint8           `json:"schema"`
	Available bool            `json:"available"`
	Boot      string          `json:"boot_id"`
	At        uint64          `json:"sample_unix_seconds"`
	Devices   []networkDevice `json:"devices"`
}
type networkDevice struct {
	Interface string   `json:"interface"`
	Kind      string   `json:"kind"`
	State     string   `json:"state"`
	IPv4      []string `json:"ipv4"`
	IPv6      []string `json:"ipv6"`
	DNS4      []string `json:"dns_ipv4"`
	DNS6      []string `json:"dns_ipv6"`
}
type route struct {
	iface string
	index uint32
	dns   []netip.AddrPort
	hash  [32]byte
}

func selectRoute(raw []byte, iface, boot string, now uint64) (route, error) {
	var snapshot networkSnapshot
	if len(raw) > 60000 || json.Unmarshal(raw, &snapshot) != nil || snapshot.Schema != 1 || !snapshot.Available || snapshot.Boot != boot || snapshot.At > now || now-snapshot.At > 30 || len(snapshot.Devices) > 16 || iface != "wlan0" && iface != "usb0" {
		return route{}, ErrDenied
	}
	var match *networkDevice
	for i := range snapshot.Devices {
		if snapshot.Devices[i].Interface == iface {
			if match != nil {
				return route{}, ErrDenied
			}
			match = &snapshot.Devices[i]
		}
	}
	expected := "wifi"
	if iface == "usb0" {
		expected = "ethernet"
	}
	if match == nil || match.Kind != expected || !strings.HasPrefix(match.State, "100 ") {
		return route{}, ErrDenied
	}
	if len(match.IPv4) > 16 || len(match.IPv6) > 16 || len(match.DNS4) > 16 || len(match.DNS6) > 16 {
		return route{}, ErrDenied
	}
	families := map[bool]bool{}
	for _, list := range [][]string{match.IPv4, match.IPv6} {
		for _, s := range list {
			prefix, e := netip.ParsePrefix(s)
			if e != nil || prefix.Addr().IsUnspecified() || prefix.Addr().IsMulticast() || prefix.Addr().IsLoopback() {
				return route{}, ErrDenied
			}
			families[prefix.Addr().Is4()] = true
		}
	}
	r := route{iface: iface}
	seen := map[netip.Addr]bool{}
	for _, list := range [][]string{match.DNS4, match.DNS6} {
		for _, s := range list {
			ip, e := netip.ParseAddr(s)
			if e != nil || ip.IsUnspecified() || ip.IsMulticast() || ip.IsLoopback() || !families[ip.Is4()] {
				return route{}, ErrDenied
			}
			if !seen[ip] && len(r.dns) < 3 {
				r.dns = append(r.dns, netip.AddrPortFrom(ip, 53))
				seen[ip] = true
			}
		}
	}
	if len(r.dns) == 0 {
		return route{}, ErrDenied
	}
	canonical, _ := json.Marshal(match)
	r.hash = sha256.Sum256(canonical)
	return r, nil
}
func currentRoute(p *Policy) (route, error) {
	raw, e := rootFile(networkPath, 60000, false)
	if e != nil {
		return route{}, e
	}
	boot, e := os.ReadFile("/proc/sys/kernel/random/boot_id")
	if e != nil || len(boot) > 64 {
		return route{}, ErrDenied
	}
	r, e := selectRoute(raw, p.config.Interface, strings.TrimSpace(string(boot)), uint64(time.Now().Unix()))
	if e != nil {
		return route{}, e
	}
	index, e := os.ReadFile("/sys/class/net/" + r.iface + "/ifindex")
	if e != nil || len(index) > 16 {
		return route{}, ErrDenied
	}
	i, e := strconv.ParseUint(strings.TrimSpace(string(index)), 10, 32)
	if e != nil || i == 0 {
		return route{}, ErrDenied
	}
	r.index = uint32(i)
	return r, nil
}
