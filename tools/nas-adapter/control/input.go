// SPDX-License-Identifier: AGPL-3.0-only
package control

import (
	"context"
	"encoding/json"
)

// Run dispatches only finite public commands; there is no arbitrary RPC port.
// Tokens can only come from the separately loaded normal-client configuration.
func Run(ctx context.Context, c *Client, raw []byte) (any, error) {
	var fields map[string]json.RawMessage
	if StrictJSON(raw, &fields) != nil {
		return nil, invalid
	}
	var op string
	if json.Unmarshal(fields["op"], &op) != nil {
		return nil, invalid
	}
	switch op {
	case "prepare_call":
		var v struct {
			Op      string  `json:"op"`
			ID      string  `json:"id"`
			Line    uint64  `json:"line_epoch"`
			Key     string  `json:"idempotency_key"`
			Action  string  `json:"action"`
			CallID  *string `json:"call_id"`
			Payload string  `json:"payload"`
		}
		if StrictJSON(raw, &v) != nil {
			return nil, invalid
		}
		return c.Prepare(ctx, v.ID, v.Line, v.Key, v.Action, v.CallID, v.Payload)
	case "submit_call":
		var v struct {
			Op          string `json:"op"`
			ID          string `json:"id"`
			OperationID string `json:"operation_id"`
			Lease       *Lease `json:"lease"`
			Urgent      bool   `json:"urgent"`
		}
		if StrictJSON(raw, &v) != nil {
			return nil, invalid
		}
		return c.Submit(ctx, v.ID, v.OperationID, v.Lease, v.Urgent)
	case "call_operation", "cancel_prepared_call":
		var v struct {
			Op          string `json:"op"`
			ID          string `json:"id"`
			OperationID string `json:"operation_id"`
		}
		if StrictJSON(raw, &v) != nil {
			return nil, invalid
		}
		if op == "call_operation" {
			return c.Operation(ctx, v.ID, v.OperationID)
		}
		return c.Cancel(ctx, v.ID, v.OperationID)
	case "renew_call":
		var v struct {
			Op    string `json:"op"`
			ID    string `json:"id"`
			Lease Lease  `json:"lease"`
		}
		if StrictJSON(raw, &v) != nil {
			return nil, invalid
		}
		return c.Renew(ctx, v.ID, v.Lease)
	default:
		return nil, invalid
	}
}
