// SPDX-License-Identifier: AGPL-3.0-only
package protocol

import (
	"bytes"
	"errors"
	"strings"
	"testing"
)

const valid = `{"protocol":1,"id":"11111111-1111-4111-8111-111111111111","op":"status"}`

func TestFiniteCommands(t *testing.T) {
	for _, op := range []Operation{Status, Stop} {
		data := []byte(strings.Replace(valid, "status", string(op), 1))
		before := bytes.Clone(data)
		r, err := Parse(data)
		if err != nil || r.Protocol != 1 || r.Op != op || !bytes.Equal(data, before) {
			t.Fatalf("finite command rejected or input changed: %v", err)
		}
	}
}

func TestStrictBoundedObject(t *testing.T) {
	for _, bad := range []string{
		"", "null", "[]", "{}", valid[:len(valid)-1], valid + valid,
		valid + "false", strings.Repeat(" ", MaxRequestBytes+1),
		strings.Replace(valid, `"protocol":1`, `"protocol":1,"protocol":1`, 1),
		strings.Replace(valid, `"id":`, `"id":"11111111-1111-4111-8111-111111111111","\u0069d":`, 1),
		strings.Replace(valid, `"protocol":1`, `"protocol":1,"seed":"SYNTHETIC_SECRET"`, 1),
		strings.Replace(valid, `"protocol":1`, `"protocol":true`, 1),
		strings.Replace(valid, `"protocol":1`, `"protocol":1.5`, 1),
		strings.Replace(valid, `"id":`, `"url":`, 1),
		strings.Replace(valid, "11111111-1111-4111-8111-111111111111", "not-an-id", 1),
		strings.Replace(valid, "11111111-1111-4111-8111-111111111111", "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA", 1),
	} {
		r, err := Parse([]byte(bad))
		if !errors.Is(err, ErrMalformed) || r != (Request{}) || strings.Contains(err.Error(), "SYNTHETIC_SECRET") {
			t.Fatal("malformed input must return a redacted category and no request")
		}
	}
}

func TestUnsupportedActionsNeverBecomeCommands(t *testing.T) {
	for _, op := range []string{"start", "dial", "shell", "set_route", "http://example.invalid", "subscribe"} {
		r, err := Parse([]byte(strings.Replace(valid, "status", op, 1)))
		if !errors.Is(err, ErrUnsupported) || r != (Request{}) {
			t.Fatal("unsupported action admitted")
		}
	}
	if _, err := Parse([]byte(strings.Replace(valid, `"protocol":1`, `"protocol":2`, 1))); !errors.Is(err, ErrUnsupported) {
		t.Fatal("unknown protocol admitted")
	}
}
