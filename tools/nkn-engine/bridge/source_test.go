// SPDX-License-Identifier: AGPL-3.0-only
package bridge

import (
	"bytes"
	"context"
	"errors"
	"strings"
	"testing"
)

type sourceFixture struct {
	status Status
	err    error
	calls  int
	cancel context.CancelFunc
}

func (s *sourceFixture) SDKLinked() bool { return true }
func (s *sourceFixture) Read(context.Context, uint32) (Status, error) {
	s.calls++
	if s.cancel != nil {
		s.cancel()
	}
	return s.status, s.err
}

func freshStatus() Status {
	age := uint64(0)
	return Status{Contract: 1, Source: "engine", Generation: 7, Sequence: 1, AgeMS: &age,
		Enabled: "unknown", Service: "unknown", Path: "unknown", DHT: "unknown"}
}

func TestSourceUsesExistingWireAndStopDoesNotSample(t *testing.T) {
	s := &sourceFixture{status: freshStatus()}
	input := append(frame(request), frame(strings.Replace(request, "status", "stop", 1))...)
	var out bytes.Buffer
	if ServeWithSource(context.Background(), bytes.NewReader(input), &out, nil, nil, s) != nil {
		t.Fatal("source serve failed")
	}
	r := decode(t, out.Bytes())
	if s.calls != 1 || len(r) != 2 || !r[0].Result.SDKLinked || r[0].Result.Status.LastError != nil ||
		r[1].Result.Kind != "closing_this_bridge" || !r[1].Result.SDKLinked {
		t.Fatal("wrong source/stop projection")
	}
}

func TestSourceFailureAndPrivateFieldsAreNotSerialized(t *testing.T) {
	secret := "SYNTHETIC_PRIVATE_ENDPOINT"
	tooOld := uint64(5001)
	badCounter := "18446744073709551616"
	for _, mutate := range []func(*sourceFixture){
		func(s *sourceFixture) { s.err = errors.New(secret) },
		func(s *sourceFixture) { s.status.LastError = &secret },
		func(s *sourceFixture) { s.status.Path = secret },
		func(s *sourceFixture) { s.status.AgeMS = &tooOld },
		func(s *sourceFixture) { s.status.Generation = 0 },
		func(s *sourceFixture) { s.status.Sequence = 2 },
		func(s *sourceFixture) { s.status.RX = &badCounter },
		func(s *sourceFixture) { s.status.Source = "mock" },
		func(s *sourceFixture) { s.status.Source = "unavailable" },
	} {
		s := &sourceFixture{status: freshStatus()}
		mutate(s)
		var out bytes.Buffer
		if ServeWithSource(context.Background(), bytes.NewReader(frame(request)), &out, nil, nil, s) != nil {
			t.Fatal("finite source failure should retain framing")
		}
		r := decode(t, out.Bytes())
		if bytes.Contains(out.Bytes(), []byte(secret)) || len(r) != 1 || r[0].Error != "status_unavailable" || r[0].Result != nil {
			t.Fatal("untrusted source escaped the typed finite projection")
		}
	}
}

func TestCancelledAndInvalidRequestsNeverAdoptSourceEvidence(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	s := &sourceFixture{status: freshStatus(), cancel: cancel}
	var out bytes.Buffer
	if !errors.Is(ServeWithSource(ctx, bytes.NewReader(frame(request)), &out, nil, nil, s), context.Canceled) || out.Len() != 0 {
		t.Fatal("late sample published after cancellation")
	}
	s.cancel, s.calls = nil, 0
	bad := frame(strings.Replace(request, "status", "start", 1))
	out.Reset()
	if ServeWithSource(context.Background(), bytes.NewReader(bad), &out, nil, nil, s) != nil || s.calls != 0 {
		t.Fatal("invalid request reached source")
	}
}
