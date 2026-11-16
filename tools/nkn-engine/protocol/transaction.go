// SPDX-License-Identifier: AGPL-3.0-only
package protocol

import (
	"bytes"
	"encoding/json"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/usagetx"
)

const MaxOwnedRequestBytes = 2048

const (
	UsageNonce  Operation = "usage_nonce"
	UsageSubmit Operation = "usage_submit"
	UsageQuery  Operation = "usage_query"
)

func TransactionOperation(op Operation) usagetx.Operation {
	switch op {
	case UsageNonce:
		return usagetx.Preflight
	case UsageSubmit:
		return usagetx.Submit
	case UsageQuery:
		return usagetx.Query
	}
	return 0
}

// ParseOwned must only be selected by an original privately admitted host with
// the explicit transaction capability. Public Parse stays flat/status-only.
func ParseOwned(raw []byte) (Request, error) {
	if r, e := Parse(raw); e == nil {
		return r, nil
	}
	f, e := usagetx.Fields(raw, MaxOwnedRequestBytes)
	if e != nil || len(f) != 4 || f["protocol"] == nil || f["id"] == nil || f["op"] == nil || f["transaction"] == nil {
		return Request{}, ErrMalformed
	}
	var r Request
	d := json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if d.Decode(&r) != nil || r.Protocol != 1 || !validID(r.ID) || TransactionOperation(r.Op) == 0 {
		return Request{}, ErrMalformed
	}
	tx, e := usagetx.Parse(TransactionOperation(r.Op), f["transaction"])
	if e != nil {
		return Request{}, ErrMalformed
	}
	r.Transaction = &tx
	return r, nil
}
