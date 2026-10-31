// SPDX-License-Identifier: AGPL-3.0-only
// Package bridge implements an actual finite stdio frontend. No NKN SDK,
// identity, Root port or network stack is linked; unsupported means unavailable.
package bridge

import (
	"context"
	"encoding/binary"
	"encoding/json"
	"errors"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/protocol"
	"io"
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
	LastError  string  `json:"last_error"`
}

func unavailable(sequence uint32) Status {
	return Status{Contract: 1, Source: "unavailable", Sequence: sequence, Enabled: "unknown", Service: "unknown", Path: "unknown", DHT: "unknown", LastError: "engine_unavailable"}
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
	fail := func(code string) error { return write(output, Response{Protocol: 1, Error: code}, beforeWrite) }
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
		response := Response{Protocol: 1, ID: &request.ID, Result: &Result{Kind: kind, Status: unavailable(uint32(index + 1))}}
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
