// SPDX-License-Identifier: AGPL-3.0-only
package usagetx

import (
	"bytes"
	"encoding/json"
)

// Reply carries only a bound conclusion, never raw node data. This candidate
// deliberately has NO confirmed state, even if an upstream node claims one.
type Reply struct {
	Version                 uint8   `json:"version"`
	OperationID             string  `json:"operation_id"`
	ParticipationGeneration uint64  `json:"participation_generation"`
	PublicKey               string  `json:"public_key"`
	Status                  string  `json:"status"`
	Nonce                   *uint64 `json:"nonce,omitempty"`
	Intent                  *Intent `json:"intent,omitempty"`
	Digest                  string  `json:"digest,omitempty"`
}

func NewReply(op Operation, r Request, status string, nonce *uint64) (Reply, error) {
	v := Reply{Version: 1, OperationID: r.OperationID, ParticipationGeneration: r.ParticipationGeneration, PublicKey: r.PublicKey, Status: status, Nonce: nonce}
	if op != Preflight && r.Intent != nil {
		in := *r.Intent
		v.Intent = &in
		v.Digest = r.Digest
	}
	if v.Validate(op, r) != nil {
		return Reply{}, ErrInvalid
	}
	return v, nil
}

func (v Reply) Validate(op Operation, r Request) error {
	if r.Validate(op) != nil || v.Version != 1 || v.OperationID != r.OperationID || v.ParticipationGeneration != r.ParticipationGeneration || v.PublicKey != r.PublicKey {
		return ErrInvalid
	}
	if op == Preflight {
		if v.Intent != nil || v.Digest != "" || (v.Status == "ready") != (v.Nonce != nil) {
			return ErrInvalid
		}
	} else if v.Nonce != nil || v.Intent == nil || *v.Intent != *r.Intent || v.Digest != r.Digest {
		return ErrInvalid
	}
	switch v.Status {
	case "ready":
		if op != Preflight {
			return ErrInvalid
		}
	case "accepted":
		if op != Submit {
			return ErrInvalid
		}
	case "unknown":
		if op == Preflight {
			return ErrInvalid
		}
	case "unavailable", "denied":
	default:
		return ErrInvalid
	}
	return nil
}

func ParseReply(op Operation, r Request, raw []byte) (Reply, error) {
	f, e := Fields(raw, MaxRequestBytes)
	if e != nil {
		return Reply{}, ErrInvalid
	}
	want := 5
	if op != Preflight {
		want += 2
	} else if _, ok := f["nonce"]; ok {
		want++
	}
	if len(f) != want {
		return Reply{}, ErrInvalid
	}
	for _, k := range []string{"version", "operation_id", "participation_generation", "public_key", "status"} {
		if _, ok := f[k]; !ok {
			return Reply{}, ErrInvalid
		}
	}
	if op != Preflight {
		if f["intent"] == nil || f["digest"] == nil {
			return Reply{}, ErrInvalid
		}
	}
	var v Reply
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if d.Decode(&v) != nil || v.Validate(op, r) != nil {
		return Reply{}, ErrInvalid
	}
	return v, nil
}
