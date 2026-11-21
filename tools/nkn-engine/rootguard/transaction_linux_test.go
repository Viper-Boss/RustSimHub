// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"io"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

func rootTxFixture(t *testing.T) usagetx.Request {
	t.Helper()
	raw, e := os.ReadFile("../../nkn-build/usage-vectors/vectors.json")
	if e != nil {
		t.Fatal(e)
	}
	var bundle struct {
		Vectors []struct {
			Intent      usagetx.Intent `json:"intent"`
			Transaction string         `json:"transaction_hex"`
			Digest      string         `json:"digest_hex"`
		} `json:"vectors"`
	}
	if json.Unmarshal(raw, &bundle) != nil || len(bundle.Vectors) != 15 {
		t.Fatal("fixed SDK fixture missing")
	}
	v := bundle.Vectors[0]
	return usagetx.Request{Version: 1, OperationID: "11111111-2222-3333-4444-555555555555", ParticipationGeneration: 1, PublicKey: v.Intent.PublicKey, Intent: &v.Intent, Digest: v.Digest, Transaction: v.Transaction}
}
func rootPreflight(r usagetx.Request) usagetx.Request {
	r.Intent = nil
	r.Digest = ""
	r.Transaction = ""
	return r
}

type rootTransactionSource struct {
	usagePipeSource
	txCalls int
	block   chan struct{}
	started chan struct{}
}

func (s *rootTransactionSource) Transaction(ctx context.Context, op usagetx.Operation, r usagetx.Request) (usagetx.Reply, error) {
	s.txCalls++
	if s.started != nil {
		close(s.started)
	}
	if s.block != nil {
		select {
		case <-s.block:
		case <-ctx.Done():
			return usagetx.Reply{}, ctx.Err()
		}
	}
	state := "unknown"
	var nonce *uint64
	if op == usagetx.Preflight {
		state = "ready"
		n := uint64(7)
		nonce = &n
	} else if op == usagetx.Submit {
		state = "accepted"
	}
	return usagetx.NewReply(op, r, state, nonce)
}
func rootTxPipes(t *testing.T, source *rootTransactionSource) *ownedEngine {
	t.Helper()
	inputR, inputW, e := os.Pipe()
	if e != nil {
		t.Fatal(e)
	}
	outputR, outputW, e := os.Pipe()
	if e != nil {
		t.Fatal(e)
	}
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan error, 1)
	go func() { done <- bridge.ServeOwnedTransactions(ctx, inputR, outputW, nil, nil, source) }()
	t.Cleanup(func() {
		cancel()
		inputW.Close()
		inputR.Close()
		outputW.Close()
		outputR.Close()
		select {
		case <-done:
		case <-time.After(time.Second):
			t.Error("bridge leaked")
		}
	})
	return &ownedEngine{input: inputW, output: outputR, ready: true, statusEpoch: 99, transactionProtocol: 1}
}
func TestRootOwnedTransactionSharesOriginalStatusUsageLockAndPipes(t *testing.T) {
	source := &rootTransactionSource{}
	engine := rootTxPipes(t, source)
	r := rootPreflight(rootTxFixture(t))
	v := engine.transaction(context.Background(), usagetx.Preflight, r)
	if v == nil || v.Status != "ready" || v.Nonce == nil || *v.Nonce != 7 || source.txCalls != 1 || engine.statusFrames != 1 {
		t.Fatal("private transaction frame failed")
	}
	if engine.transaction(context.Background(), usagetx.Preflight, r) != nil || source.txCalls != 1 {
		t.Fatal("transaction throttle bypass")
	}
	status, _ := engine.readonly(context.Background(), 7)
	if status.Source != "engine" || status.Sequence != 2 {
		t.Fatal("transaction corrupted status sequence")
	}
	if value := engine.usage(context.Background(), usage24h); value == nil || value.Count != 42 || engine.statusFrames != 3 {
		t.Fatal("transaction replaced usage pipe")
	}
	engine.statusMu.Lock()
	if engine.transaction(context.Background(), usagetx.Preflight, r) != nil {
		t.Fatal("parallel IO bypassed shared mutex")
	}
	engine.statusMu.Unlock()
	if _, e := engine.input.Stat(); e != nil {
		t.Fatal("individual transaction closed shared engine")
	}
	engine.statusFrames = 1024
	if engine.transaction(context.Background(), usagetx.Query, func() usagetx.Request { q := rootTxFixture(t); q.Transaction = ""; return q }()) != nil {
		t.Fatal("frame budget bypass")
	}
}
func TestRootOwnedTransactionCancellationPoisonsReaderWithoutClosingSharedSDK(t *testing.T) {
	started := make(chan struct{})
	release := make(chan struct{})
	source := &rootTransactionSource{started: started, block: release}
	engine := rootTxPipes(t, source)
	defer close(release)
	ctx, cancel := context.WithCancel(context.Background())
	done := make(chan *usagetx.Reply, 1)
	go func() { done <- engine.transaction(ctx, usagetx.Preflight, rootPreflight(rootTxFixture(t))) }()
	select {
	case <-started:
	case <-time.After(time.Second):
		t.Fatal("transaction did not start")
	}
	at := time.Now()
	cancel()
	select {
	case v := <-done:
		if v != nil || !engine.statusPoisoned || time.Since(at) > 300*time.Millisecond {
			t.Fatal("cancel did not fence original pipe")
		}
	case <-time.After(time.Second):
		t.Fatal("cancelled pipe read blocked")
	}
	if _, e := engine.input.Stat(); e != nil {
		t.Fatal("cancel stopped shared SDK")
	}
	if engine.transaction(context.Background(), usagetx.Preflight, rootPreflight(rootTxFixture(t))) != nil || source.txCalls != 1 {
		t.Fatal("poisoned pipe reused")
	}
}
func TestRootTransactionReplyRejectsMixedOrForgedContext(t *testing.T) {
	r := rootPreflight(rootTxFixture(t))
	zero := uint64(0)
	v, _ := usagetx.NewReply(usagetx.Preflight, r, "ready", &zero)
	id := "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
	raw, _ := json.Marshal(bridge.Response{Protocol: 1, ID: &id, Transaction: &v})
	if value, e := transactionReply(raw, id, usagetx.Preflight, r); e != nil || value == nil || value.Nonce == nil || *value.Nonce != 0 {
		t.Fatal("bound zero nonce rejected")
	}
	for _, bad := range [][]byte{
		bytes.Replace(raw, []byte(`"nonce":0`), []byte(`"nonce":null`), 1),
		bytes.Replace(raw, []byte(`"status":"ready"`), []byte(`"status":"confirmed"`), 1),
		bytes.Replace(raw, []byte(`"protocol":1`), []byte(`"protocol":1,"usage":{"window":"24h","count":0}`), 1),
		bytes.Replace(raw, []byte(`"protocol":1`), []byte(`"protocol":1,"protocol":1`), 1),
		append(raw, []byte(" {}")...),
	} {
		if _, e := transactionReply(bad, id, usagetx.Preflight, r); e == nil {
			t.Fatal("mixed/unbound transaction reply accepted")
		}
	}
	if _, e := transactionReply(raw, "wrong", usagetx.Preflight, r); e == nil {
		t.Fatal("wrong request ID accepted")
	}
	legacyStatus := sampleReply()
	legacyStatus = append(legacyStatus[:len(legacyStatus)-1], []byte(`,"transaction":null}`)...)
	if _, e := sdkReply(legacyStatus, "11111111-1111-4111-8111-111111111111", 7, 1); e == nil {
		t.Fatal("status accepted transaction alias")
	}
	usage := []byte(`{"protocol":1,"id":"` + id + `","usage":{"window":"24h","count":0},"transaction":null}`)
	if _, e := usageReply(usage, id, "usage_24h"); e == nil {
		t.Fatal("usage accepted transaction alias")
	}
}
func txUnixPair(t *testing.T) (*net.UnixConn, *net.UnixConn) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "s")
	listener, e := net.ListenUnix("unix", &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	defer listener.Close()
	client, e := net.DialUnix("unix", nil, &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	server, e := listener.AcceptUnix()
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { client.Close(); server.Close() })
	return client, server
}
func TestRootTransactionRegistrationBindingAndKernelReadinessAreSeparateGates(t *testing.T) {
	_, owner := txUnixPair(t)
	_, observer := txUnixPair(t)
	tx := rootPreflight(rootTxFixture(t))
	now, e := bootMillis()
	if e != nil {
		t.Fatal(e)
	}
	var nonce, policy [32]byte
	nonce[0] = 1
	policy[0] = 2
	r := request{op: 10, generation: 9, deadline: now + 10000, nonce: nonce, policy: policy}
	engine := &ownedEngine{ready: true, transactionProtocol: 1}
	reg := &registration{request: r, owner: owner, engine: engine}
	m := &manager{policy: &Policy{hash: policy, config: configuration{StatisticsKey: tx.PublicKey, UsageTransactionProtocol: 1}, roles: roles{core: uint32(os.Getuid())}}, scope: reg}
	if !m.transactionAllowed(r, observer) {
		t.Fatal("original registration observer refused")
	}
	for _, change := range []func(*request){func(r *request) { r.generation++ }, func(r *request) { r.deadline++ }, func(r *request) { r.nonce[0]++ }, func(r *request) { r.policy[0]++ }} {
		bad := r
		change(&bad)
		if m.transactionAllowed(bad, observer) {
			t.Fatal("wrong original registration admitted")
		}
	}
	if m.transactionAllowed(r, owner) || m.transactionAllowed(r, nil) {
		t.Fatal("transaction reused original owner control")
	}
	m.policy.config.UsageTransactionProtocol = 0
	if m.transactionAllowed(r, observer) {
		t.Fatal("default policy granted transaction")
	}
	m.policy.config.UsageTransactionProtocol = 1
	m.policy.roles.core++
	if m.transactionAllowed(r, observer) {
		t.Fatal("wrong actual kernel peer admitted")
	}
	m.policy.roles.core--
	if _, e := m.handle(r, observer); e == nil {
		t.Fatal("transaction bypassed RSTX body path")
	}
	// Synthetic ready=true is NOT a kernel/cgroup/executable proof: production
	// registration path must refuse IO rather than accepting an injected flag.
	raw, e := m.handleTransaction(context.Background(), r, observer, tx)
	if e != nil || len(raw) <= responseBytes+9 || engine.statusFrames != 0 || reg.released || reg.unknown {
		t.Fatal("kernel rejection mutated lifecycle or emitted IO", e)
	}
	v, e := usagetx.ParseReply(usagetx.Preflight, tx, raw[responseBytes+9:])
	if e != nil || v.Status != "unavailable" {
		t.Fatal("fake kernel evidence granted readiness")
	}
	tx.PublicKey = strings.Repeat("12", 32)
	if _, e := m.handleTransaction(context.Background(), r, observer, tx); e == nil {
		t.Fatal("wrong statistics identity admitted")
	}
}
func TestRootTransactionObserverEOFDoesNotCloseOriginalOwner(t *testing.T) {
	ownerClient, owner := txUnixPair(t)
	client, observer := txUnixPair(t)
	ctx, finish := watchObserver(context.Background(), observer)
	defer finish()
	client.Close()
	select {
	case <-ctx.Done():
	case <-time.After(300 * time.Millisecond):
		t.Fatal("observer EOF did not cancel")
	}
	if _, e := ownerClient.Write([]byte{1}); e != nil {
		t.Fatal("observer closed original owner")
	}
	owner.SetReadDeadline(time.Now().Add(time.Second))
	var b [1]byte
	if _, e := io.ReadFull(owner, b[:]); e != nil || b[0] != 1 {
		t.Fatal("shared owner socket not usable")
	}
}
func TestRootTransactionFramesAndVersionNegotiation(t *testing.T) {
	tx := rootPreflight(rootTxFixture(t))
	raw, _ := json.Marshal(tx)
	header := make([]byte, 9)
	copy(header, []byte("RSTX\x01"))
	binary.BigEndian.PutUint32(header[5:], uint32(len(raw)))
	valid := append(header, raw...)
	if _, e := readTransaction(bytes.NewReader(valid), usagetx.Preflight); e != nil {
		t.Fatal(e)
	}
	for _, bad := range [][]byte{nil, valid[:8], valid[:len(valid)-1], append([]byte("RSUC\x01"), valid[5:]...), append([]byte("RSTX\x01\x00\x00\x08\x01"), raw...)} {
		if _, e := readTransaction(bytes.NewReader(bad), usagetx.Preflight); e == nil {
			t.Fatal("bad transaction frame accepted")
		}
	}
	c := configuration{Format: 1, Source: strings.Repeat("a", 40), Executable: strings.Repeat("b", 64), CorePin: strings.Repeat("c", 64), StatisticsKey: strings.Repeat("d", 64), Interface: "usb0", Seeds: []string{"https://seed.invalid"}}
	version := `{"contract":1,"source_commit":"` + c.Source + `","go":"go1.27.3","nkn_sdk_linked":true,"root_launch_required":true,"isolated_fixture_build":false}`
	if !configurationValid(c) || !engineVersionValid([]byte(version), c) {
		t.Fatal("old default version/config compatibility lost")
	}
	c.UsageTransactionProtocol = 1
	if configurationValid(c) || engineVersionValid([]byte(version), c) {
		t.Fatal("new capability enabled with old status/SDK")
	}
	c.StatusProtocol = 1
	newVersion := version[:len(version)-1] + `,"usage_transaction_protocol":1}`
	if !configurationValid(c) || !engineVersionValid([]byte(newVersion), c) {
		t.Fatal("explicit v1 negotiation refused")
	}
	for _, bad := range []string{strings.Replace(newVersion, `"usage_transaction_protocol":1`, `"usage_transaction_protocol":2`, 1), strings.Replace(newVersion, `"usage_transaction_protocol":1`, `"usage_transaction_protocol":null`, 1), strings.Replace(newVersion, `"usage_transaction_protocol":1`, `"usage_transaction_protocol":1,"usage_transaction_protocol":1`, 1)} {
		if engineVersionValid([]byte(bad), c) {
			t.Fatal("unknown/null/duplicate capability enabled")
		}
	}
	c.UsageTransactionProtocol = 2
	if configurationValid(c) {
		t.Fatal("unknown policy capability enabled")
	}
}
