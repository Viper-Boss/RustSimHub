// SPDX-License-Identifier: AGPL-3.0-only
// Package protocol is a proposed, pure local request parser, not a running engine.
// It never imports a network SDK, accepts key material, or changes host state.
package protocol

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
)

const MaxRequestBytes = 1024

var (
	ErrMalformed   = errors.New("invalid engine request")
	ErrUnsupported = errors.New("unsupported engine request")
)

type Operation string

const (
	Status   Operation = "status"
	Stop     Operation = "stop"
	Usage24h Operation = "usage_24h"
	Usage30d Operation = "usage_30d"
	Usage90d Operation = "usage_90d"
)

type Request struct {
	Protocol uint8     `json:"protocol"`
	ID       string    `json:"id"`
	Op       Operation `json:"op"`
}

// Parse accepts exactly one flat JSON object. Duplicate/unknown keys, extra
// documents and oversized data fail without echoing input or parser diagnostics.
func Parse(data []byte) (Request, error) {
	var result Request
	if len(data) == 0 || len(data) > MaxRequestBytes {
		return result, ErrMalformed
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	first, err := decoder.Token()
	if err != nil || first != json.Delim('{') {
		return result, ErrMalformed
	}
	seen := make(map[string]bool, 3)
	for decoder.More() {
		key, err := decoder.Token()
		name, ok := key.(string)
		if err != nil || !ok || seen[name] {
			return Request{}, ErrMalformed
		}
		seen[name] = true
		switch name {
		case "protocol":
			err = decoder.Decode(&result.Protocol)
		case "id":
			err = decoder.Decode(&result.ID)
		case "op":
			err = decoder.Decode(&result.Op)
		default:
			return Request{}, ErrMalformed
		}
		if err != nil {
			return Request{}, ErrMalformed
		}
	}
	last, err := decoder.Token()
	if err != nil || last != json.Delim('}') {
		return Request{}, ErrMalformed
	}
	var extra json.RawMessage
	if decoder.Decode(&extra) != io.EOF || len(seen) != 3 || !validID(result.ID) {
		return Request{}, ErrMalformed
	}
	if result.Protocol != 1 || result.Op != Status && result.Op != Stop && !IsUsage(result.Op) {
		return Request{}, ErrUnsupported
	}
	return result, nil
}

// The caller chooses one of three product-owned windows, never a topic/URL.
func IsUsage(op Operation) bool { return op == Usage24h || op == Usage30d || op == Usage90d }
func UsageWindow(op Operation) string {
	switch op {
	case Usage24h:
		return "24h"
	case Usage30d:
		return "30d"
	case Usage90d:
		return "90d"
	}
	return ""
}

func validID(id string) bool {
	if len(id) != 36 {
		return false
	}
	for i := range len(id) {
		c := id[i]
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}
