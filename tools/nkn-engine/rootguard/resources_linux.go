// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

const nftPath = "/usr/sbin/nft"
const nftTable = "rustsimhub_nkn"
const parentGroup = "/sys/fs/cgroup/system.slice/rustsimhub-nkn-root.service"

func nftCommand(ctx context.Context, args []string, input []byte) ([]byte, error) {
	if rootDirectory("/usr/sbin") != nil {
		return nil, ErrDenied
	}
	info, e := os.Lstat(nftPath)
	if e != nil || !info.Mode().IsRegular() || info.Mode().Perm()&0022 != 0 {
		return nil, ErrDenied
	}
	// Distribution binary ownership is checked by the same no-follow reader.
	if _, e := rootFile(nftPath, 8*1024*1024, false); e != nil {
		return nil, ErrDenied
	}
	ctx, cancel := context.WithTimeout(ctx, time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, nftPath, args...)
	cmd.Env = []string{"LC_ALL=C"}
	cmd.Stdin = bytes.NewReader(input)
	var out boundedOutput
	cmd.Stdout = &out
	cmd.Stderr = io.Discard
	if cmd.Run() != nil {
		return nil, ErrDenied
	}
	return out.bytes, nil
}
func tableAbsent(ctx context.Context) bool {
	raw, e := nftCommand(ctx, []string{"-j", "list", "tables"}, nil)
	if e != nil {
		return false
	}
	var response struct {
		Items []json.RawMessage `json:"nftables"`
	}
	if json.Unmarshal(raw, &response) != nil || len(response.Items) > 256 {
		return false
	}
	for _, item := range response.Items {
		var row struct {
			Table *struct {
				Family string `json:"family"`
				Name   string `json:"name"`
			} `json:"table"`
		}
		if json.Unmarshal(item, &row) != nil {
			return false
		}
		if row.Table != nil && row.Table.Name == nftTable {
			return false
		}
	}
	return true
}
func tableDigest(ctx context.Context) ([32]byte, error) {
	raw, e := nftCommand(ctx, []string{"-j", "list", "table", "inet", nftTable}, nil)
	if e != nil {
		return [32]byte{}, ErrDenied
	}
	var response struct {
		Items []map[string]json.RawMessage `json:"nftables"`
	}
	if json.Unmarshal(raw, &response) != nil || len(response.Items) < 5 || len(response.Items) > 20 {
		return [32]byte{}, ErrDenied
	}
	var filtered []map[string]json.RawMessage
	tables, chains, rules := 0, 0, 0
	for _, item := range response.Items {
		if _, ok := item["metainfo"]; ok {
			continue
		}
		if _, ok := item["table"]; ok {
			tables++
		}
		if _, ok := item["chain"]; ok {
			chains++
		}
		if _, ok := item["rule"]; ok {
			rules++
		}
		filtered = append(filtered, item)
	}
	if tables != 1 || chains != 1 || rules < 4 || rules > 10 {
		return [32]byte{}, ErrDenied
	}
	canonical, e := json.Marshal(filtered)
	if e != nil {
		return [32]byte{}, ErrDenied
	}
	return sha256.Sum256(canonical), nil
}
func ruleScript(uid uint32, r route) ([]byte, error) {
	if uid == 0 || uid == ^uint32(0) || r.iface != "wlan0" && r.iface != "usb0" || r.index == 0 || len(r.dns) < 1 || len(r.dns) > 3 {
		return nil, ErrDenied
	}
	var b strings.Builder
	fmt.Fprintf(&b, "add table inet %s\nadd chain inet %s output { type filter hook output priority -60; policy accept; }\n", nftTable, nftTable)
	prefix := fmt.Sprintf("add rule inet %s output meta skuid %d ", nftTable, uid)
	// Interface fence precedes every allow, including established connections
	// and fixed DNS. No routing/proxy/WWAN fallback can bypass this UID rule.
	fmt.Fprintf(&b, "%soifname != \"%s\" drop\n", prefix, r.iface)
	for _, dns := range r.dns {
		ip := dns.Addr()
		if dns.Port() != 53 || ip.IsUnspecified() || ip.IsMulticast() || ip.IsLoopback() || ip.Zone() != "" {
			return nil, ErrDenied
		}
		family := "ip"
		if ip.Is6() {
			family = "ip6"
		}
		fmt.Fprintf(&b, "%s%s daddr %s meta l4proto { tcp, udp } th dport 53 accept\n", prefix, family, ip.String())
	}
	fmt.Fprintf(&b, "%sip daddr != { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 127.0.0.0/8, 169.254.0.0/16, 172.16.0.0/12, 192.0.0.0/24, 192.0.2.0/24, 192.88.99.0/24, 192.168.0.0/16, 198.18.0.0/15, 198.51.100.0/24, 203.0.113.0/24, 224.0.0.0/4, 240.0.0.0/4 } meta l4proto tcp accept\n", prefix)
	fmt.Fprintf(&b, "%sip6 daddr 2000::/3 ip6 daddr != { 2001::/32, 2001:2::/48, 2001:10::/28, 2001:20::/28, 2001:db8::/32, 2002::/16, 3fff::/20 } meta l4proto tcp accept\n", prefix)
	fmt.Fprintf(&b, "%sdrop\n", prefix)
	return []byte(b.String()), nil
}

type resources struct {
	policy                     *Policy
	route                      route
	nft                        [32]byte
	tableCreated, groupCreated bool
	uncertain                  bool
}

func writeKernel(path, value string) error {
	f, e := os.OpenFile(path, os.O_WRONLY, 0)
	if e != nil {
		return ErrDenied
	}
	defer f.Close()
	if n, e := f.WriteString(value); e != nil || n != len(value) {
		return ErrDenied
	}
	return nil
}
func readKernel(path string) (string, error) {
	f, e := os.Open(path)
	if e != nil {
		return "", ErrDenied
	}
	defer f.Close()
	b, e := io.ReadAll(io.LimitReader(f, 4097))
	if e != nil || len(b) > 4096 {
		return "", ErrDenied
	}
	return strings.TrimSpace(string(b)), nil
}
func exactSelfGroup(expected string) bool {
	data, e := readKernel("/proc/self/cgroup")
	return e == nil && data == "0::"+expected
}
func prepareManagerGroup() error {
	stem := "/system.slice/rustsimhub-nkn-root.service"
	if !exactSelfGroup(stem) && !exactSelfGroup(stem+"/manager") {
		return ErrDenied
	}
	if rootDirectory(parentGroup) != nil {
		return ErrDenied
	}
	manager := parentGroup + "/manager"
	if e := os.Mkdir(manager, 0755); e != nil && !os.IsExist(e) {
		return ErrDenied
	}
	if rootDirectory(manager) != nil {
		return ErrDenied
	}
	pids, e := readKernel(manager + "/cgroup.procs")
	if e != nil || pids != "" && pids != strconv.Itoa(os.Getpid()) {
		return ErrDenied
	}
	if writeKernel(manager+"/cgroup.procs", strconv.Itoa(os.Getpid())) != nil {
		return ErrDenied
	}
	if !exactSelfGroup(stem + "/manager") {
		return ErrDenied
	}
	if writeKernel(parentGroup+"/cgroup.subtree_control", "+cpu +memory +pids") != nil {
		return ErrDenied
	}
	controllers, e := readKernel(parentGroup + "/cgroup.subtree_control")
	if e != nil {
		return ErrDenied
	}
	for _, name := range []string{"cpu", "memory", "pids"} {
		if !strings.Contains(" "+controllers+" ", " "+name+" ") {
			return ErrDenied
		}
	}
	return nil
}
func engineUIDIdle(uid uint32) bool {
	entries, e := os.ReadDir("/proc")
	if e != nil || len(entries) > 65536 {
		return false
	}
	for _, entry := range entries {
		if _, e := strconv.ParseUint(entry.Name(), 10, 32); e != nil {
			continue
		}
		path := "/proc/" + entry.Name() + "/status"
		f, e := os.Open(path)
		if os.IsNotExist(e) {
			continue
		}
		if e != nil {
			return false
		}
		data, e := io.ReadAll(io.LimitReader(f, 8193))
		f.Close()
		if e != nil || len(data) > 8192 {
			return false
		}
		for _, line := range strings.Split(string(data), "\n") {
			if strings.HasPrefix(line, "Uid:") {
				for _, value := range strings.Fields(strings.TrimPrefix(line, "Uid:")) {
					n, e := strconv.ParseUint(value, 10, 32)
					if e != nil || uint32(n) == uid {
						return false
					}
				}
			}
		}
	}
	return true
}
func prepareResources(ctx context.Context, p *Policy) (*resources, error) {
	r := &resources{policy: p}
	if !engineUIDIdle(p.roles.engine) || prepareManagerGroup() != nil {
		return r, ErrDenied
	}
	route, e := currentRoute(p)
	if e != nil {
		return r, e
	}
	r.route = route
	if _, e := os.Lstat(groupPath); !os.IsNotExist(e) {
		return r, ErrDenied
	}
	if e := os.Mkdir(groupPath, 0755); e != nil {
		return r, ErrDenied
	}
	r.groupCreated = true
	for file, value := range map[string]string{"memory.max": "134217728", "memory.oom.group": "1", "pids.max": "32", "cpu.max": "20000 100000"} {
		if writeKernel(filepath.Join(groupPath, file), value) != nil {
			return r, ErrDenied
		}
		if actual, e := readKernel(filepath.Join(groupPath, file)); e != nil || actual != value {
			return r, ErrDenied
		}
	}
	if !tableAbsent(ctx) {
		return r, ErrDenied
	}
	script, e := ruleScript(p.roles.engine, route)
	if e != nil {
		return r, e
	}
	// nft applies one transaction. Unknown command completion is cleanup-only;
	// never adopt a table whose successful creation was not observed.
	r.uncertain = true
	if _, e := nftCommand(ctx, []string{"-f", "-"}, script); e != nil {
		return r, e
	}
	r.tableCreated = true
	r.uncertain = false
	r.nft, e = tableDigest(ctx)
	if e != nil {
		r.uncertain = true
		return r, e
	}
	return r, nil
}
func (r *resources) check(ctx context.Context) error {
	if r == nil || r.uncertain || !r.tableCreated || !r.groupCreated {
		return ErrDenied
	}
	current, e := currentRoute(r.policy)
	if e != nil || current.hash != r.route.hash || current.index != r.route.index {
		return ErrDenied
	}
	if digest, e := tableDigest(ctx); e != nil || digest != r.nft {
		return ErrDenied
	}
	return nil
}
func (r *resources) release(ctx context.Context) error {
	if r == nil || r.uncertain {
		return ErrDenied
	}
	if r.groupCreated {
		pids, e := readKernel(groupPath + "/cgroup.procs")
		if e != nil || pids != "" {
			return ErrDenied
		}
		events, e := readKernel(groupPath + "/cgroup.events")
		if e != nil || !strings.Contains("\n"+events+"\n", "\npopulated 0\n") {
			return ErrDenied
		}
		if !engineUIDIdle(r.policy.roles.engine) {
			return ErrDenied
		}
	}
	if r.tableCreated {
		if digest, e := tableDigest(ctx); e != nil || digest != r.nft {
			return ErrDenied
		}
		if _, e := nftCommand(ctx, []string{"delete", "table", "inet", nftTable}, nil); e != nil {
			return ErrDenied
		}
		if !tableAbsent(ctx) {
			return ErrDenied
		}
		r.tableCreated = false
	}
	if r.groupCreated {
		if os.Remove(groupPath) != nil {
			return ErrDenied
		}
		r.groupCreated = false
		if _, e := os.Lstat(groupPath); !os.IsNotExist(e) {
			return ErrDenied
		}
	}
	return nil
}
