//go:build nkn_sdk_candidate

// SPDX-License-Identifier: AGPL-3.0-only
// Package sdkstatus samples an already owned real SDK client. It never opens
// a client, reads credentials, dials, signs, renews authority or grants access.
package sdkstatus

import (
	"context"
	"errors"
	"sync"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	sdk "github.com/nknorg/nkn-sdk-go"
)

var ErrUnavailable = errors.New("SDK status unavailable")

// Source observes one host-owned client within one trusted launch generation.
// The launch generation is ordering context, never proof of authorization.
// Neither a connection handle nor IsClosed proves network readiness/process exit.
type Source struct {
	mu         sync.Mutex
	client     *sdk.Client
	generation uint32
	sequence   uint32
	retired    bool
	lastRead   time.Time
	now        func() time.Time
}

// Attach performs no IO or identity generation. The caller must already own the
// client and authorize consumers; no payload or browser supplies this handle.
func Attach(client *sdk.Client, generation uint32) (*Source, error) {
	if client == nil || generation == 0 {
		return nil, ErrUnavailable
	}
	return &Source{client: client, generation: generation, now: time.Now}, nil
}

func (*Source) SDKLinked() bool { return true }

// Retire invalidates this observation context without closing a shared client.
// A new bridge/source must be created for a later launch; sequence never wraps.
func (s *Source) Retire() {
	s.mu.Lock()
	s.retired = true
	s.client = nil
	s.mu.Unlock()
}

func (s *Source) Read(ctx context.Context, sequence uint32) (bridge.Status, error) {
	if ctx.Err() != nil {
		return bridge.Status{}, ErrUnavailable
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.retired || s.client == nil || sequence == 0 || sequence <= s.sequence {
		return bridge.Status{}, ErrUnavailable
	}
	start := s.now()
	if start.IsZero() || !s.lastRead.IsZero() && start.Before(s.lastRead) {
		s.retired = true
		s.client = nil
		return bridge.Status{}, ErrUnavailable
	}
	closed := s.client.IsClosed() // Actual SDK state, protected by its own lock.
	end := s.now()
	if ctx.Err() != nil || end.Before(start) || end.Sub(start) > 3*time.Second {
		s.retired = true
		s.client = nil
		return bridge.Status{}, ErrUnavailable
	}
	s.sequence, s.lastRead = sequence, end
	age := uint64(end.Sub(start) / time.Millisecond)
	status := bridge.Status{Contract: 1, Source: "engine", Generation: s.generation,
		Sequence: sequence, AgeMS: &age, Enabled: "unknown", Service: "unknown",
		Path: "unknown", DHT: "unknown"}
	if closed {
		code := "engine_unavailable"
		status.LastError = &code
	}
	// Missing process/Owner/path/DHT/traffic/RTT/node observations remain unknown.
	// In particular, SDK Close does not become service=stopped or enabled=off.
	return status, nil
}
