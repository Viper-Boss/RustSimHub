// SPDX-License-Identifier: AGPL-3.0-only
// Package bridge implements a finite stdio frontend. The default source is
// unavailable; a host may attach an independently authorized read-only source.
package bridge

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"io"
	"strconv"
)

const MaxFrames = 32
const MaxResponse = 2048

var ErrFrame = errors.New("invalid local frame")
var ErrLimit = errors.New("local frame limit")

// Exact T5 read-only status shape. Unknown values are null, never invented zero
// counts/RTT, process evidence or source=engine. Sequence is frontend-local only.
type Status struct {
	Contract   uint8   `json:"contract"`
	Source     string  `json:"source"`
	Generation uint32  `json:"generation"`
	Sequence   uint32  `json:"sequence"`
	AgeMS      *uint64 `json:"age_ms"`
	Enabled    string  `json:"enabled"`
	Service    string  `json:"service"`
	Path       string  `json:"path"`
	DHT        string  `json:"dht"`
	RX         *string `json:"rx_bytes"`
	TX         *string `json:"tx_bytes"`
	RTT        *uint32 `json:"rtt_ms"`
	Peers      *uint32 `json:"peer_count"`
	LastError  *string `json:"last_error"`
}

func unavailable(sequence uint32) Status {
	code := "engine_unavailable"
	return Status{Contract: 1, Source: "unavailable", Sequence: sequence, Enabled: "unknown", Service: "unknown", Path: "unknown", DHT: "unknown", LastError: &code}
}

// StatusSource is an in-process, read-only host interface, not an authorization
// proof or new IPC. Read must perform no network IO, startup or credential work.
// The host owns its lifetime and must independently authorize each caller.
type StatusSource interface {
	Read(context.Context, uint32) (Status, error)
	SDKLinked() bool
}

func oneOf(value string, allowed ...string) bool {
	for _, item := range allowed {
		if value == item {
			return true
		}
	}
	return false
}

func counter(value *string) bool {
	if value == nil {
		return true
	}
	n, err := strconv.ParseUint(*value, 10, 64)
	return err == nil && strconv.FormatUint(n, 10) == *value
}

// Reject invalid/private provider text before serializing it. The bridge owns
// the sequence; an unavailable source cannot advertise fabricated live fields.
func validStatus(s Status, sequence uint32) bool {
	if s.Contract != 1 || s.Sequence != sequence || !oneOf(s.Source, "engine", "unavailable") ||
		!oneOf(s.Enabled, "on", "off", "unknown") ||
		!oneOf(s.Service, "stopped", "starting", "running", "stopping", "failed", "unknown") ||
		!oneOf(s.Path, "direct", "relay", "offline", "unknown") ||
		!oneOf(s.DHT, "connected", "disconnected", "unknown") ||
		!counter(s.RX) || !counter(s.TX) || s.RTT != nil && *s.RTT > 60000 ||
		s.Peers != nil && *s.Peers > 100000 ||
		s.LastError != nil && !oneOf(*s.LastError, "engine_unavailable", "deadline", "protocol_error", "stale", "permission_denied") {
		return false
	}
	if s.Source == "unavailable" {
		return s.AgeMS == nil && s.Enabled == "unknown" && s.Service == "unknown" &&
			s.Path == "unknown" && s.DHT == "unknown" && s.RX == nil && s.TX == nil && s.RTT == nil && s.Peers == nil
	}
	return s.Generation != 0 && s.AgeMS != nil && *s.AgeMS <= 5000
}

type Result struct {
	Kind      string `json:"kind"`
	Status    Status `json:"status"`
	SDKLinked bool   `json:"nkn_sdk_linked"`
}
type Response struct {
	Protocol uint8   `json:"protocol"`
	ID       *string `json:"id"`
	Result   *Result `json:"result,omitempty"`
	Error    string  `json:"error,omitempty"`
}

func write(output io.Writer, response Response, before func() error) error {
	if before != nil {
		if err := before(); err != nil {
			return ErrFrame
		}
	}
	data, err := json.Marshal(response)
	if err != nil || len(data) > MaxResponse {
		return ErrFrame
	}
	var header [4]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(data)))
	for _, part := range [][]byte{header[:], data} {
		n, e := output.Write(part)
		if e != nil || n != len(part) {
			return ErrFrame
		}
	}
	return nil
}

// Serve processes <=32 finite frames, each a big-endian u32 length + <=1024B
// strict request JSON. Main provides one read/write deadline per frame and
// closes its OWN stdin/stdout on cancellation. No IO goroutines escape Serve.
func Serve(ctx context.Context, input io.Reader, output io.Writer, beforeRead, beforeWrite func() error) error {
	return ServeWithSource(ctx, input, output, beforeRead, beforeWrite, nil)
}

// ServeWithSource preserves the existing status/stop wire. Stop still closes
// only this bridge; it never stops an unrelated/shared SDK or Root service.
func ServeWithSource(ctx context.Context, input io.Reader, output io.Writer, beforeRead, beforeWrite func() error, source StatusSource) error {
	fail := func(code string) error { return write(output, Response{Protocol: 1, Error: code}, beforeWrite) }
	// Static build metadata, not live readiness. It remains true in a closing
	// response when this host actually attached an SDK-backed implementation.
	linked := source != nil && source.SDKLinked()
	for index := 0; index < MaxFrames; index++ {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if beforeRead != nil {
			if err := beforeRead(); err != nil {
				return ErrFrame
			}
		}
		var header [4]byte
		n, err := io.ReadFull(input, header[:])
		if errors.Is(err, io.EOF) && n == 0 {
			return nil
		}
		if err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			_ = fail("invalid_frame")
			return ErrFrame
		}
		length := binary.BigEndian.Uint32(header[:])
		if length == 0 || length > protocol.MaxRequestBytes {
			_ = fail("invalid_frame")
			return ErrFrame
		}
		data := make([]byte, int(length))
		if _, err = io.ReadFull(input, data); err != nil {
			_ = fail("invalid_frame")
			return ErrFrame
		}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		request, err := protocol.Parse(data)
		if err != nil {
			code := "invalid_request"
			if errors.Is(err, protocol.ErrUnsupported) {
				code = "unsupported_request"
			}
			if err = fail(code); err != nil {
				return err
			}
			continue
		}
		kind := "status"
		if request.Op == protocol.Stop {
			kind = "closing_this_bridge"
		}
		sequence := uint32(index + 1)
		status := unavailable(sequence)
		// Closing must not depend on a live Read callback or create a fresh sample.
		if source != nil && request.Op == protocol.Status {
			var sourceErr error
			status, sourceErr = source.Read(ctx, sequence)
			if ctx.Err() != nil {
				return ctx.Err()
			}
			if sourceErr != nil || !validStatus(status, sequence) {
				if err = write(output, Response{Protocol: 1, ID: &request.ID, Error: "status_unavailable"}, beforeWrite); err != nil {
					return err
				}
				continue
			}
		}
		response := Response{Protocol: 1, ID: &request.ID, Result: &Result{Kind: kind, Status: status, SDKLinked: linked}}
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if err = write(output, response, beforeWrite); err != nil {
			return err
		}
		// A stop acknowledgement is intent to exit this frontend. It says nothing
		// about any separate NKN daemon/Root unit; the parent must wait for exit.
		if request.Op == protocol.Stop {
			return nil
		}
	}
	_ = fail("frame_limit")
	return ErrLimit
}
