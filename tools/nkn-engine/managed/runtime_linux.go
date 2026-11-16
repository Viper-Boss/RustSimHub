//go:build nkn_sdk_candidate

// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"net/http"
	"os"
	"sync"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/sdkstatus"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
	sdk "github.com/nknorg/nkn-sdk-go"
	"github.com/nknorg/nkngomobile"
)

type ownedSource struct {
	mu           sync.Mutex
	source       *sdkstatus.Source
	lease        *Lease
	usageHTTP    *http.Client
	transactions *usageTransactions
}

func (s *ownedSource) Transaction(ctx context.Context, op usagetx.Operation, r usagetx.Request) (usagetx.Reply, error) {
	s.mu.Lock()
	ready := s.source != nil && s.lease.Check() == nil && s.lease.launch.transactions == 1 && UsageTransactionProtocol() == 1
	port := s.transactions
	s.mu.Unlock()
	if !ready || port == nil {
		return usagetx.Reply{}, ErrLaunch
	}
	v, e := port.execute(ctx, op, r)
	if e != nil || s.lease.Check() != nil {
		return usagetx.Reply{}, ErrLaunch
	}
	state := v.State
	if op == usagetx.Preflight {
		state = "ready"
	}
	return usagetx.NewReply(op, r, state, v.Nonce)
}

func (s *ownedSource) Usage(ctx context.Context, op protocol.Operation) (bridge.UsageCount, error) {
	s.mu.Lock()
	ready := s.source != nil && s.lease.Check() == nil
	s.mu.Unlock()
	if !ready {
		return bridge.UsageCount{}, ErrLaunch
	}
	value, err := usageCount(ctx, op, s.usageHTTP, s.lease.launch.seeds[0])
	if s.lease.Check() != nil {
		return bridge.UsageCount{}, ErrLaunch
	}
	return value, err
}

func (*ownedSource) SDKLinked() bool { return true }
func (s *ownedSource) Read(ctx context.Context, sequence uint32) (bridge.Status, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.source == nil || s.lease.Check() != nil {
		return bridge.Status{}, ErrLaunch
	}
	return s.source.Read(ctx, sequence)
}

// Run owns one Root-launched SDK process and its inherited stdio. Only the
// trusted shared-engine supervisor may own that process-level control pipe;
// releasing an individual service reference must never call Run's stop wire.
// The parent still has to reap the process and prove cgroup/policy release.
func Run(lease *Lease, input, output *os.File) error {
	if lease == nil || input == nil || output == nil || lease.Check() != nil {
		return ErrLaunch
	}
	defer func() { lease.Close(); input.Close(); output.Close() }()
	dial := &scopedDialer{lease: lease, startDeadline: time.Now().Add(10 * time.Second), connections: make(map[*scopedConn]struct{})}
	port := &relay{lease: lease}
	source := &ownedSource{lease: lease, usageHTTP: newUsageHTTP(dial)}
	if lease.launch.transactions == 1 {
		if UsageTransactionProtocol() != 1 {
			return ErrLaunch
		}
		port, e := newUsageTransactions(lease, dial)
		if e != nil {
			return ErrLaunch
		}
		source.transactions = port
	}
	defer func() {
		port.close()
		dial.close()
		source.mu.Lock()
		if source.source != nil {
			source.source.Retire()
		}
		source.mu.Unlock()
	}()
	done := make(chan struct{})
	defer close(done)
	go func() {
		select {
		case <-lease.Context().Done():
			port.close()
			dial.close()
			input.Close()
			output.Close()
		case <-done:
		}
	}()
	// Observe EOF/stop/malformed stdio while NewClient is still constructing.
	stdioDone := make(chan error, 1)
	go func() {
		serve := bridge.ServeOwnedWithSource
		if lease.launch.transactions == 1 && UsageTransactionProtocol() == 1 {
			serve = bridge.ServeOwnedTransactions
		}
		err := serve(lease.Context(), input, output,
			func() error { deadline, _ := lease.Context().Deadline(); return input.SetReadDeadline(deadline) },
			func() error { return output.SetWriteDeadline(time.Now().Add(2 * time.Second)) }, source)
		stdioDone <- err
		lease.Close()
	}()
	startup := time.AfterFunc(time.Until(dial.startDeadline), lease.Close)
	defer startup.Stop()
	lease.mu.Lock()
	seed := lease.launch.seed
	clear(lease.launch.seed[:])
	lease.mu.Unlock()
	account, e := sdk.NewAccount(seed[:])
	clear(seed[:])
	if e != nil {
		return ErrLaunch
	}
	client, e := sdk.NewClient(account, "", &sdk.ClientConfig{ManagedMode: true,
		SeedRPCServerAddr: nkngomobile.NewStringArray(lease.launch.seeds...),
		HttpDialContext:   dial.httpDial, WsDialContext: dial.wsDial,
		MsgChanLen: 4, ConnectRetries: 1, RPCConcurrency: 1, RPCTimeout: 1000, WsHandshakeTimeout: 1000, WsWriteTimeout: 1000,
		ResolverTimeout: 1000, MsgCacheExpiration: 1000, MsgCacheCleanupInterval: 1000})
	if e != nil {
		return ErrLaunch
	}
	defer func() { dial.close(); _ = client.Close() }()
	select {
	case node, ok := <-client.OnConnect.C:
		if !ok || node == nil || client.IsClosed() || lease.Check() != nil {
			return ErrLaunch
		}
	case <-lease.Context().Done():
		return ErrLaunch
	}
	if !startup.Stop() || !time.Now().Before(dial.startDeadline) || lease.Check() != nil {
		return ErrLaunch
	}
	sampled, e := sdkstatus.Attach(client, lease.Generation())
	if e != nil {
		return ErrLaunch
	}
	source.mu.Lock()
	source.source = sampled
	source.mu.Unlock()
	if lease.ready() != nil {
		return ErrLaunch
	}
	tick := time.NewTicker(100 * time.Millisecond)
	defer tick.Stop()
	for {
		select {
		case err := <-stdioDone:
			return err
		case <-lease.Context().Done():
			select {
			case err := <-stdioDone:
				return err
			default:
				return ErrLaunch
			}
		case <-tick.C:
			if client.IsClosed() || lease.Check() != nil {
				return ErrLaunch
			}
			port.observeEOF()
		case msg, ok := <-client.OnMessage.C:
			if !ok || msg == nil {
				return ErrLaunch
			}
			if !msg.Encrypted || msg.Type != 0 || !msg.NoReply || len(msg.MessageID) != 8 {
				continue
			}
			var id [8]byte
			copy(id[:], msg.MessageID)
			reply, err := port.exchange(msg.Src, id, msg.Data)
			if err != nil {
				continue
			} // Source is fenced; never retry its operation.
			if len(reply) == 0 {
				continue
			}
			if lease.Check() != nil {
				clear(reply)
				return ErrLaunch
			}
			_, err = client.SendBinary(nkngomobile.NewStringArray(msg.Src), reply, &sdk.MessageConfig{NoReply: true, MaxHoldingSeconds: 0})
			clear(reply)
			if err != nil {
				port.close()
				return ErrLaunch
			}
		}
	}
}
