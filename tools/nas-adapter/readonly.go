// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math"
	"reflect"
	"strconv"
	"strings"
	"unicode/utf8"
)

const maxReply = 65536

// Failure contains only local labels and an HTTP status, never remote error text.
type Failure struct {
	Kind string
	HTTP int
}

func (e *Failure) Error() string {
	if e.HTTP != 0 {
		return fmt.Sprintf("%s (HTTP %d)", e.Kind, e.HTTP)
	}
	return e.Kind
}

func invalidReply() error { return &Failure{Kind: "invalid_reply"} }

// strictJSON rejects ambiguous documents before decoding: duplicate keys, trailing
// documents, invalid UTF-8, excessive nesting and case aliases for struct fields.
func strictJSON(raw []byte, target any) error {
	if len(raw) == 0 || len(raw) > maxReply || !utf8.Valid(raw) {
		return invalidReply()
	}
	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()
	var walk func(int) error
	walk = func(depth int) error {
		if depth > 16 {
			return invalidReply()
		}
		t, err := d.Token()
		if err != nil {
			return invalidReply()
		}
		if delim, ok := t.(json.Delim); ok {
			switch delim {
			case '{':
				keys := map[string]bool{}
				for d.More() {
					key, err := d.Token()
					name, ok := key.(string)
					if err != nil || !ok || keys[name] {
						return invalidReply()
					}
					keys[name] = true
					if err := walk(depth + 1); err != nil {
						return err
					}
				}
			case '[':
				for d.More() {
					if err := walk(depth + 1); err != nil {
						return err
					}
				}
			default:
				return invalidReply()
			}
			if _, err := d.Token(); err != nil {
				return invalidReply()
			}
		}
		return nil
	}
	if err := walk(0); err != nil {
		return err
	}
	if _, err := d.Token(); err != io.EOF {
		return invalidReply()
	}
	// Check exact field names recursively, and do not let null silently turn a
	// required bool, integer, string, array or object into its Go zero value.
	var shape func([]byte, reflect.Type) error
	shape = func(value []byte, typ reflect.Type) error {
		if typ == reflect.TypeOf(json.RawMessage{}) {
			return nil
		}
		if typ.Kind() == reflect.Pointer {
			if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
				return nil
			}
			return shape(value, typ.Elem())
		}
		if bytes.Equal(bytes.TrimSpace(value), []byte("null")) {
			return invalidReply()
		}
		switch typ.Kind() {
		case reflect.Struct:
			var fields map[string]json.RawMessage
			if json.Unmarshal(value, &fields) != nil || fields == nil {
				return invalidReply()
			}
			for i := 0; i < typ.NumField(); i++ {
				field := typ.Field(i)
				name := strings.Split(field.Tag.Get("json"), ",")[0]
				v, ok := fields[name]
				if !ok && field.Type.Kind() != reflect.Pointer {
					return invalidReply()
				}
				if ok {
					if err := shape(v, field.Type); err != nil {
						return err
					}
				}
				delete(fields, name)
			}
			if len(fields) != 0 {
				return invalidReply()
			}
		case reflect.Slice:
			var values []json.RawMessage
			if json.Unmarshal(value, &values) != nil {
				return invalidReply()
			}
			for _, v := range values {
				if err := shape(v, typ.Elem()); err != nil {
					return err
				}
			}
		}
		return nil
	}
	if err := shape(raw, reflect.TypeOf(target).Elem()); err != nil {
		return err
	}
	d = json.NewDecoder(bytes.NewReader(raw))
	d.DisallowUnknownFields()
	if d.Decode(target) != nil {
		return invalidReply()
	}
	return nil
}

type Cursor struct {
	DBEpoch string `json:"db_epoch"`
	Seq     string `json:"seq"`
}

func uuid(value string) bool {
	if len(value) != 36 {
		return false
	}
	for i, c := range value {
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !strings.ContainsRune("0123456789abcdefABCDEF", c) {
			return false
		}
	}
	return true
}

func (c Cursor) valid() bool {
	if !uuid(c.DBEpoch) || len(c.Seq) == 0 || len(c.Seq) > 19 {
		return false
	}
	for _, c := range c.Seq {
		if c < '0' || c > '9' {
			return false
		}
	}
	_, err := strconv.ParseInt(c.Seq, 10, 64)
	return err == nil
}

type Query struct {
	Entity   string  `json:"entity"`
	After    *Cursor `json:"after"`
	Limit    int     `json:"limit"`
	RecordID *string `json:"record_id"`
}

func (q Query) validate() error {
	if q.Limit < 1 || q.Limit > 16 || (q.After != nil && !q.After.valid()) {
		return &Failure{Kind: "invalid_query"}
	}
	switch q.Entity {
	case "status", "sms", "calls":
		if q.RecordID != nil {
			return &Failure{Kind: "invalid_query"}
		}
	case "sms_detail":
		if q.RecordID == nil || !uuid(*q.RecordID) {
			return &Failure{Kind: "invalid_query"}
		}
	default:
		return &Failure{Kind: "invalid_query"}
	}
	if q.After != nil && q.Entity != "sms" && q.Entity != "calls" {
		return &Failure{Kind: "invalid_query"}
	}
	return nil
}

func readInput(raw []byte) (string, Query, error) {
	var input struct {
		Token string `json:"token"`
		Query Query  `json:"query"`
	}
	if len(raw) > 8192 || strictJSON(raw, &input) != nil || !hexToken(input.Token) || input.Query.validate() != nil {
		return "", Query{}, &Failure{Kind: "invalid_query"}
	}
	return input.Token, input.Query, nil
}

func hexToken(value string) bool {
	_, err := hex.DecodeString(value)
	return len(value) == 64 && value == strings.ToLower(value) && err == nil
}

// These are projections of the existing v2 storage views, not old NAS payloads.
// Optional presentation fields remain absent; no phone number or time is invented.
type LineView struct {
	Line   uint64 `json:"line"`
	Status string `json:"status"`
}

type SmsDisplay struct {
	Sender    string `json:"sender"`
	Timestamp string `json:"timestamp"`
	Preview   string `json:"preview"`
}

type SmsSummary struct {
	ID                 string      `json:"id"`
	Line               uint64      `json:"line"`
	Binary             bool        `json:"binary"`
	DuplicateCandidate *string     `json:"duplicate_candidate"`
	Display            *SmsDisplay `json:"display,omitempty"`
}

type SmsPage struct {
	Messages []SmsSummary `json:"messages"`
	Cursor   Cursor       `json:"cursor"`
}

type SmsDetail struct {
	Summary   SmsSummary `json:"summary"`
	Sender    string     `json:"sender"`
	Timestamp string     `json:"timestamp"`
	Text      string     `json:"text"`
	// []byte would incorrectly encode Rust Vec<u8> as Base64 in Go.
	Data []uint16 `json:"data"`
}

type CallSummary struct {
	ID           string  `json:"id"`
	Line         uint64  `json:"line"`
	Direction    int32   `json:"direction"`
	State        string  `json:"state"`
	ModemState   int32   `json:"modem_state"`
	Reason       int32   `json:"reason"`
	Availability string  `json:"availability"`
	MediaState   string  `json:"media_state"`
	Number       *string `json:"number,omitempty"`
}

type CallPage struct {
	Calls  []CallSummary `json:"calls"`
	Cursor Cursor        `json:"cursor"`
}

type ReadReply struct {
	AdapterAPI int `json:"adapter_api"`
	DataFormat int `json:"data_format"`
	Data       any `json:"data"`
}

func validSms(s SmsSummary) bool {
	return uuid(s.ID) && s.Line <= math.MaxInt64 && (s.DuplicateCandidate == nil || uuid(*s.DuplicateCandidate))
}

func member(value string, choices ...string) bool {
	for _, choice := range choices {
		if value == choice {
			return true
		}
	}
	return false
}

// The decoder is stateless. It neither stores records nor advances a checkpoint.
// A caller commits the returned cursor only after successfully consuming a page.
func decodeRead(q Query, raw []byte) (*ReadReply, error) {
	if err := q.validate(); err != nil {
		return nil, err
	}
	var envelope struct {
		AdapterAPI int             `json:"adapter_api"`
		DataFormat int             `json:"data_format"`
		Data       json.RawMessage `json:"data"`
	}
	if strictJSON(raw, &envelope) != nil {
		return nil, invalidReply()
	}
	if envelope.AdapterAPI != 1 || envelope.DataFormat != 2 {
		return nil, &Failure{Kind: "version_unsupported"}
	}
	var data any
	var cursor *Cursor
	count := 0
	ids := map[string]bool{}
	add := func(id string) bool {
		if ids[id] {
			return false
		}
		ids[id] = true
		return true
	}
	switch q.Entity {
	case "status":
		var status LineView
		if strictJSON(envelope.Data, &status) != nil || status.Line > math.MaxInt64 || !member(status.Status, "ready", "absent", "locked", "observing", "unavailable") {
			return nil, invalidReply()
		}
		data = status
	case "sms":
		var page SmsPage
		if strictJSON(envelope.Data, &page) != nil {
			return nil, invalidReply()
		}
		for _, s := range page.Messages {
			if !validSms(s) || !add(s.ID) {
				return nil, invalidReply()
			}
		}
		data, cursor, count = page, &page.Cursor, len(page.Messages)
	case "sms_detail":
		var detail SmsDetail
		if strictJSON(envelope.Data, &detail) != nil || !validSms(detail.Summary) || detail.Summary.ID != *q.RecordID {
			return nil, invalidReply()
		}
		for _, b := range detail.Data {
			if b > 255 {
				return nil, invalidReply()
			}
		}
		if detail.Summary.Binary != (len(detail.Data) > 0) {
			return nil, invalidReply()
		}
		data = detail
	case "calls":
		var page CallPage
		if strictJSON(envelope.Data, &page) != nil {
			return nil, invalidReply()
		}
		for _, c := range page.Calls {
			if !uuid(c.ID) || c.Line > math.MaxInt64 || !add(c.ID) ||
				!member(c.State, "outgoing_pending", "dialing", "ringing_out", "ringing_in", "active", "held", "waiting", "ended", "reconciling") ||
				!member(c.Availability, "observed", "reconciling") || c.MediaState != "none" {
				return nil, invalidReply()
			}
		}
		data, cursor, count = page, &page.Cursor, len(page.Calls)
	}
	if cursor != nil {
		if !cursor.valid() || count > q.Limit {
			return nil, invalidReply()
		}
		prior := int64(0)
		if q.After != nil {
			if cursor.DBEpoch != q.After.DBEpoch {
				return nil, &Failure{Kind: "resync_required"}
			}
			prior, _ = strconv.ParseInt(q.After.Seq, 10, 64)
		}
		next, _ := strconv.ParseInt(cursor.Seq, 10, 64)
		if (count == 0 && next != prior) || (count > 0 && next <= prior) {
			return nil, invalidReply()
		}
	}
	return &ReadReply{AdapterAPI: 1, DataFormat: 2, Data: data}, nil
}
