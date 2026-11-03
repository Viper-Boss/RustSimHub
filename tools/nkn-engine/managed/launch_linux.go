// SPDX-License-Identifier: AGPL-3.0-only
// Package managed validates an inherited, Root-owned launch. The public stdio
// parser cannot create this authority, provide a seed, URL, path or UID.
package managed

import (
	"bytes"
	"context"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/netip"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

var ErrLaunch = errors.New("managed launch unavailable")

const MaxCredential = 4096
const CoreSocket = "/run/rustsimhub/nkn/core.sock"

// FixtureBuild is immutable build metadata. Release assembly must reject true.
func FixtureBuild() bool { return fixtureBuild }

type launch struct {
	generation, coreUID, engineUID                   uint32
	seed, corePin, statisticsKey, executable, policy [32]byte
	seeds, dns                                       []string
	fixture                                          bool
}

func (launch) String() string   { return "managed launch(private)" }
func (launch) GoString() string { return "managed launch(private)" }

func fixedHex(raw json.RawMessage, out *[32]byte) error {
	// Do not create a second immutable Go string containing the secret seed.
	if len(raw) != 66 || raw[0] != '"' || raw[65] != '"' {
		return ErrLaunch
	}
	for _, c := range raw[1:65] {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return ErrLaunch
		}
	}
	if _, e := hex.Decode(out[:], raw[1:65]); e != nil || *out == [32]byte{} {
		return ErrLaunch
	}
	return nil
}

func parseLaunch(data []byte) (*launch, error) {
	if len(data) == 0 || len(data) > MaxCredential {
		return nil, ErrLaunch
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	first, e := decoder.Token()
	if e != nil || first != json.Delim('{') {
		return nil, ErrLaunch
	}
	fields := make(map[string]json.RawMessage)
	defer func() {
		for _, value := range fields {
			clear(value)
		}
	}()
	for decoder.More() {
		token, e := decoder.Token()
		name, ok := token.(string)
		if e != nil || !ok {
			return nil, ErrLaunch
		}
		if _, exists := fields[name]; exists {
			return nil, ErrLaunch
		}
		var value json.RawMessage
		if decoder.Decode(&value) != nil {
			return nil, ErrLaunch
		}
		fields[name] = value
	}
	if token, e := decoder.Token(); e != nil || token != json.Delim('}') {
		return nil, ErrLaunch
	}
	var extra any
	if decoder.Decode(&extra) != io.EOF {
		return nil, ErrLaunch
	}
	keys := []string{"format", "generation", "core_uid", "engine_uid", "seed", "core_pin", "statistics_public_key", "engine_sha256", "policy_sha256", "seed_rpc", "dns_servers", "mode"}
	if len(fields) != len(keys) {
		return nil, ErrLaunch
	}
	for _, key := range keys {
		if _, ok := fields[key]; !ok {
			return nil, ErrLaunch
		}
	}
	var version uint8
	var mode string
	l := &launch{}
	if json.Unmarshal(fields["format"], &version) != nil || version != 1 ||
		json.Unmarshal(fields["generation"], &l.generation) != nil || l.generation == 0 ||
		json.Unmarshal(fields["core_uid"], &l.coreUID) != nil || l.coreUID == 0 || l.coreUID == ^uint32(0) ||
		json.Unmarshal(fields["engine_uid"], &l.engineUID) != nil || l.engineUID == 0 || l.engineUID == ^uint32(0) || l.engineUID == l.coreUID ||
		json.Unmarshal(fields["mode"], &mode) != nil {
		return nil, ErrLaunch
	}
	switch mode {
	case "public":
	case "isolated-loopback-fixture":
		if !fixtureBuild {
			return nil, ErrLaunch
		}
		l.fixture = true
	default:
		return nil, ErrLaunch
	}
	for name, dest := range map[string]*[32]byte{"seed": &l.seed, "core_pin": &l.corePin, "statistics_public_key": &l.statisticsKey, "engine_sha256": &l.executable, "policy_sha256": &l.policy} {
		if fixedHex(fields[name], dest) != nil {
			clear(l.seed[:])
			return nil, ErrLaunch
		}
	}
	private := ed25519.NewKeyFromSeed(l.seed[:])
	public := private.Public().(ed25519.PublicKey)
	separate := !bytes.Equal(public, l.corePin[:]) && !bytes.Equal(public, l.statisticsKey[:]) && l.corePin != l.statisticsKey
	clear(private)
	if !separate {
		clear(l.seed[:])
		return nil, ErrLaunch
	}
	if json.Unmarshal(fields["seed_rpc"], &l.seeds) != nil || len(l.seeds) < 1 || len(l.seeds) > 3 ||
		json.Unmarshal(fields["dns_servers"], &l.dns) != nil || len(l.dns) < 1 || len(l.dns) > 3 {
		clear(l.seed[:])
		return nil, ErrLaunch
	}
	for _, seed := range l.seeds {
		u, e := url.Parse(seed)
		if e != nil || len(seed) > 256 || u.User != nil || u.RawQuery != "" || u.Fragment != "" || u.Opaque != "" || u.Hostname() == "" || u.Path != "" && u.Path != "/" {
			clear(l.seed[:])
			return nil, ErrLaunch
		}
		if u.Scheme != "https" {
			ip, e := netip.ParseAddr(u.Hostname())
			if !l.fixture || u.Scheme != "http" || e != nil || !ip.IsLoopback() {
				clear(l.seed[:])
				return nil, ErrLaunch
			}
		}
		if !asciiHost(u.Hostname()) {
			clear(l.seed[:])
			return nil, ErrLaunch
		}
		if u.Port() != "" {
			if p, e := strconv.ParseUint(u.Port(), 10, 16); e != nil || p == 0 {
				clear(l.seed[:])
				return nil, ErrLaunch
			}
		}
	}
	for _, server := range l.dns {
		addr, e := netip.ParseAddrPort(server)
		if e != nil || addr.Port() != 53 || addr.Addr().IsUnspecified() || addr.Addr().IsMulticast() || addr.Addr().IsLoopback() && !l.fixture {
			clear(l.seed[:])
			return nil, ErrLaunch
		}
	}
	return l, nil
}

func asciiHost(host string) bool {
	if len(host) > 253 || host == "" {
		return false
	}
	for _, c := range host {
		if !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || strings.ContainsRune(".-:", c)) {
			return false
		}
	}
	if _, e := netip.ParseAddr(host); e == nil {
		return true
	}
	for _, label := range strings.Split(host, ".") {
		if len(label) == 0 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' || strings.Contains(label, ":") {
			return false
		}
	}
	return true
}

func isolatedProcess() bool {
	f, e := os.Open("/proc/self/status")
	if e != nil {
		return false
	}
	defer f.Close()
	data, e := io.ReadAll(io.LimitReader(f, 8193))
	if e != nil || len(data) > 8192 {
		return false
	}
	seen := map[string]bool{}
	for _, line := range strings.Split(string(data), "\n") {
		key, value, ok := strings.Cut(line, ":")
		if !ok {
			continue
		}
		value = strings.TrimSpace(value)
		switch key {
		case "CapInh", "CapPrm", "CapEff", "CapAmb":
			if seen[key] || value != "0000000000000000" {
				return false
			}
			seen[key] = true
		case "NoNewPrivs":
			if seen[key] || value != "1" {
				return false
			}
			seen[key] = true
		}
	}
	return len(seen) == 5
}

// bootMS uses the kernel's boot clock, including suspend, conservatively rounded
// UP from /proc/uptime's centiseconds. A caller supplies no timestamp or clock.
func bootMS() (uint64, error) {
	f, e := os.Open("/proc/uptime")
	if e != nil {
		return 0, ErrLaunch
	}
	defer f.Close()
	b, e := io.ReadAll(io.LimitReader(f, 128))
	if e != nil || len(b) >= 128 {
		return 0, ErrLaunch
	}
	values := strings.Fields(string(b))
	if len(values) != 2 {
		return 0, ErrLaunch
	}
	parts := strings.Split(values[0], ".")
	if len(parts) != 2 || len(parts[1]) != 2 {
		return 0, ErrLaunch
	}
	seconds, e := strconv.ParseUint(parts[0], 10, 54)
	if e != nil {
		return 0, ErrLaunch
	}
	centi, e := strconv.ParseUint(parts[1], 10, 8)
	if e != nil || centi >= 100 {
		return 0, ErrLaunch
	}
	return seconds*1000 + centi*10 + 10, nil
}

type Lease struct {
	launch                 *launch
	control                *net.UnixConn
	deadlineBoot, lastBoot uint64
	nonce                  [32]byte
	ctx                    context.Context
	cancel                 context.CancelFunc
	mu                     sync.Mutex
	closed                 bool
}

func (*Lease) String() string             { return "managed lease(private)" }
func (*Lease) GoString() string           { return "managed lease(private)" }
func (l *Lease) Context() context.Context { return l.ctx }
func (l *Lease) Generation() uint32       { return l.launch.generation }
func (l *Lease) Check() error {
	l.mu.Lock()
	defer l.mu.Unlock()
	now, e := bootMS()
	if e != nil || l.closed || l.ctx.Err() != nil || now < l.lastBoot || now >= l.deadlineBoot {
		return ErrLaunch
	}
	l.lastBoot = now
	return nil
}
func (l *Lease) Close() {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.closed {
		return
	}
	l.closed = true
	l.cancel()
	clear(l.launch.seed[:])
	_ = l.control.Close()
}
func (l *Lease) ready() error {
	if l.Check() != nil {
		return ErrLaunch
	}
	frame := make([]byte, 41)
	copy(frame, []byte("RSHR\x01"))
	binary.BigEndian.PutUint32(frame[5:9], l.Generation())
	copy(frame[9:], l.nonce[:])
	_ = l.control.SetWriteDeadline(time.Now().Add(time.Second))
	if n, e := l.control.Write(frame); e != nil || n != len(frame) {
		return ErrLaunch
	}
	return l.Check()
}

// LoadRootLease accepts only the exact direct Root parent's AF_UNIX stream,
// plus one bounded anonymous credential pipe. No public stdio request reaches
// this parser; kernel peer credentials are checked before reading any secret.
func LoadRootLease(credential, control *os.File) (*Lease, error) {
	if os.Geteuid() == 0 || credential == nil || control == nil {
		return nil, ErrLaunch
	}
	info, e := credential.Stat()
	if e != nil || info.Mode()&os.ModeNamedPipe == 0 {
		return nil, ErrLaunch
	}
	ucred, e := syscall.GetsockoptUcred(int(control.Fd()), syscall.SOL_SOCKET, syscall.SO_PEERCRED)
	typeID, typeError := syscall.GetsockoptInt(int(control.Fd()), syscall.SOL_SOCKET, syscall.SO_TYPE)
	if e != nil || typeError != nil || typeID != syscall.SOCK_STREAM || ucred.Uid != 0 || ucred.Pid <= 1 || int(ucred.Pid) != os.Getppid() {
		return nil, ErrLaunch
	}
	if !isolatedProcess() {
		return nil, ErrLaunch
	}
	connection, e := net.FileConn(control)
	if e != nil {
		return nil, ErrLaunch
	}
	owned, ok := connection.(*net.UnixConn)
	if !ok {
		connection.Close()
		return nil, ErrLaunch
	}
	failed := true
	defer func() {
		if failed {
			owned.Close()
		}
	}()
	_ = owned.SetReadDeadline(time.Now().Add(2 * time.Second))
	var root [81]byte
	if _, e := io.ReadFull(owned, root[:]); e != nil || !bytes.Equal(root[:5], []byte("RSHL\x01")) {
		return nil, ErrLaunch
	}
	if credential.SetReadDeadline(time.Now().Add(2*time.Second)) != nil {
		return nil, ErrLaunch
	}
	var size [4]byte
	if _, e := io.ReadFull(credential, size[:]); e != nil {
		return nil, ErrLaunch
	}
	n := binary.BigEndian.Uint32(size[:])
	if n == 0 || n > MaxCredential {
		return nil, ErrLaunch
	}
	raw := make([]byte, n)
	defer clear(raw)
	if _, e := io.ReadFull(credential, raw); e != nil {
		return nil, ErrLaunch
	}
	var trailing [1]byte
	if n, e := credential.Read(trailing[:]); n != 0 || e != io.EOF {
		return nil, ErrLaunch
	}
	if digest := sha256.Sum256(raw); !bytes.Equal(digest[:], root[49:81]) {
		return nil, ErrLaunch
	}
	launch, e := parseLaunch(raw)
	if e != nil {
		return nil, ErrLaunch
	}
	if launch.engineUID != uint32(os.Geteuid()) || binary.BigEndian.Uint32(root[5:9]) != launch.generation {
		clear(launch.seed[:])
		return nil, ErrLaunch
	}
	exe, e := os.Open("/proc/self/exe")
	if e != nil {
		clear(launch.seed[:])
		return nil, ErrLaunch
	}
	hash := sha256.New()
	read, e := io.Copy(hash, io.LimitReader(exe, 64*1024*1024+1))
	exe.Close()
	if e != nil || read > 64*1024*1024 || !bytes.Equal(hash.Sum(nil), launch.executable[:]) {
		clear(launch.seed[:])
		return nil, ErrLaunch
	}
	deadline := binary.BigEndian.Uint64(root[9:17])
	now, e := bootMS()
	var nonce [32]byte
	copy(nonce[:], root[17:49])
	if e != nil || deadline <= now || deadline-now > 300000 || nonce == [32]byte{} {
		clear(launch.seed[:])
		return nil, ErrLaunch
	}
	ctx, cancel := context.WithTimeout(context.Background(), time.Duration(deadline-now)*time.Millisecond)
	l := &Lease{launch: launch, control: owned, deadlineBoot: deadline, lastBoot: now, nonce: nonce, ctx: ctx, cancel: cancel}
	_ = owned.SetReadDeadline(time.Now().Add(time.Duration(deadline-now) * time.Millisecond))
	go func() { var stop [1]byte; _, _ = owned.Read(stop[:]); l.Close() }()
	failed = false
	return l, nil
}
