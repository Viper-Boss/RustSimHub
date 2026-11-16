// SPDX-License-Identifier: AGPL-3.0-only
// Package usagetx defines an offline candidate for fixed-purpose statistics
// operations. It grants no Root capability and contains no signer or network.
package usagetx

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"math"
)

const MaxRequestBytes = 2048
const MaxTransactionBytes = 768

type Operation uint8

const (
	Preflight Operation = 10
	Submit    Operation = 11
	Query     Operation = 12
)

var ErrInvalid = errors.New("invalid statistics transaction operation")

// Intent has exactly the existing CheckInIntent fields and serialization order.
type Intent struct {
	Version            uint8  `json:"version"`
	PublicKey          string `json:"public_key"`
	Topic              string `json:"topic"`
	Window             string `json:"window"`
	SubscriptionBlocks uint32 `json:"subscription_blocks"`
	RenewalPeriod      uint64 `json:"renewal_period"`
}

type Request struct {
	Version                 uint8   `json:"version"`
	OperationID             string  `json:"operation_id"`
	ParticipationGeneration uint64  `json:"participation_generation"`
	PublicKey               string  `json:"public_key"`
	Intent                  *Intent `json:"intent,omitempty"`
	Digest                  string  `json:"digest,omitempty"`
	Transaction             string  `json:"transaction,omitempty"`
}

func lowerHex(s string, size int) bool {
	if len(s) != size*2 {
		return false
	}
	for _, c := range s {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}

func operationID(s string) bool {
	if len(s) != 36 {
		return false
	}
	for i, c := range s {
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return s != "00000000-0000-0000-0000-000000000000"
}

// Fields bounds input and rejects duplicate keys recursively, null, excessive
// nesting and trailing values. Callers must also enforce their exact key set.
func Fields(raw []byte, limit int) (map[string]json.RawMessage, error) {
	if len(raw) == 0 || len(raw) > limit {
		return nil, ErrInvalid
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()
	if validateJSON(d, 0) != nil {
		return nil, ErrInvalid
	}
	var tail any
	if d.Decode(&tail) != io.EOF {
		return nil, ErrInvalid
	}
	var fields map[string]json.RawMessage
	if json.Unmarshal(raw, &fields) != nil || fields == nil {
		return nil, ErrInvalid
	}
	return fields, nil
}

func validateJSON(d *json.Decoder, depth int) error {
	if depth > 8 {
		return ErrInvalid
	}
	t, err := d.Token()
	if err != nil || t == nil {
		return ErrInvalid
	}
	if delimiter, ok := t.(json.Delim); ok {
		switch delimiter {
		case '{':
			seen := make(map[string]bool)
			for d.More() {
				key, e := d.Token()
				name, ok := key.(string)
				if e != nil || !ok || seen[name] {
					return ErrInvalid
				}
				seen[name] = true
				if validateJSON(d, depth+1) != nil {
					return ErrInvalid
				}
			}
			end, e := d.Token()
			if e != nil || end != json.Delim('}') {
				return ErrInvalid
			}
		case '[':
			for d.More() {
				if validateJSON(d, depth+1) != nil {
					return ErrInvalid
				}
			}
			end, e := d.Token()
			if e != nil || end != json.Delim(']') {
				return ErrInvalid
			}
		default:
			return ErrInvalid
		}
	}
	return nil
}

func (in Intent) Validate() error {
	blocks := map[string]uint32{"24h": 8640, "30d": 133920, "90d": 393120}[in.Window]
	if in.Version != 1 || !lowerHex(in.PublicKey, 32) || blocks == 0 ||
		in.SubscriptionBlocks != blocks || in.Topic != "rustsimhub-usage-v1-"+in.Window ||
		in.RenewalPeriod >= math.MaxUint64/86400 {
		return ErrInvalid
	}
	public, _ := hex.DecodeString(in.PublicKey)
	if bytes.Equal(public, make([]byte, 32)) {
		return ErrInvalid
	}
	return nil
}

func Parse(op Operation, raw []byte) (Request, error) {
	f, err := Fields(raw, MaxRequestBytes)
	if err != nil {
		return Request{}, ErrInvalid
	}
	want := 4
	if op == Submit {
		want = 7
	} else if op == Query {
		want = 6
	} else if op != Preflight {
		return Request{}, ErrInvalid
	}
	if len(f) != want {
		return Request{}, ErrInvalid
	}
	for _, key := range []string{"version", "operation_id", "participation_generation", "public_key"} {
		if _, ok := f[key]; !ok {
			return Request{}, ErrInvalid
		}
	}
	if op != Preflight {
		in, ok := f["intent"]
		if !ok {
			return Request{}, ErrInvalid
		}
		fields, e := Fields(in, 512)
		if e != nil || len(fields) != 6 {
			return Request{}, ErrInvalid
		}
		for _, key := range []string{"version", "public_key", "topic", "window", "subscription_blocks", "renewal_period"} {
			if _, ok := fields[key]; !ok {
				return Request{}, ErrInvalid
			}
		}
		if _, ok := f["digest"]; !ok {
			return Request{}, ErrInvalid
		}
		if op == Submit {
			if _, ok := f["transaction"]; !ok {
				return Request{}, ErrInvalid
			}
		}
	}
	var r Request
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if d.Decode(&r) != nil || r.Validate(op) != nil {
		return Request{}, ErrInvalid
	}
	return r, nil
}

// Validate also fences requests constructed internally without the JSON parser.
func (r Request) Validate(op Operation) error {
	if r.Version != 1 || !operationID(r.OperationID) || r.ParticipationGeneration == 0 ||
		r.ParticipationGeneration >= math.MaxInt64 || !lowerHex(r.PublicKey, 32) {
		return ErrInvalid
	}
	public, _ := hex.DecodeString(r.PublicKey)
	if bytes.Equal(public, make([]byte, 32)) {
		return ErrInvalid
	}
	if op == Preflight {
		if r.Intent != nil || r.Digest != "" || r.Transaction != "" {
			return ErrInvalid
		}
		return nil
	}
	if op != Submit && op != Query || r.Intent == nil || r.Intent.Validate() != nil ||
		r.Intent.PublicKey != r.PublicKey || !lowerHex(r.Digest, 32) {
		return ErrInvalid
	}
	if op == Query {
		if r.Transaction != "" {
			return ErrInvalid
		}
	} else if len(r.Transaction) == 0 || len(r.Transaction)%2 != 0 ||
		len(r.Transaction) > MaxTransactionBytes*2 || !lowerHex(r.Transaction, len(r.Transaction)/2) {
		return ErrInvalid
	}
	return nil
}
