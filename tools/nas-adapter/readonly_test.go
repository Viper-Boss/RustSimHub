// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"reflect"
	"strings"
	"testing"
)

const testEpoch = "00000000-0000-4000-8000-000000000001"
const testID = "00000000-0000-4000-8000-000000000002"
const testToken = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

func envelope(data any) []byte {
	raw, err := json.Marshal(ReadReply{AdapterAPI: 1, DataFormat: 2, Data: data})
	if err != nil {
		panic(err)
	}
	return raw
}

func requireFailure(t *testing.T, err error, kind string) {
	t.Helper()
	var failure *Failure
	if !errors.As(err, &failure) || failure.Kind != kind {
		t.Fatalf("wanted %s, got %v", kind, err)
	}
}

func TestStatusIsNotConnectivityOrRegistration(t *testing.T) {
	for _, state := range []string{"ready", "absent", "locked", "observing", "unavailable"} {
		t.Run(state, func(t *testing.T) {
			reply, err := decodeRead(Query{Entity: "status", Limit: 1}, envelope(LineView{Line: 0, Status: state}))
			if err != nil || reply.Data.(LineView).Status != state {
				t.Fatalf("status lost: %v", err)
			}
		})
	}
	_, err := decodeRead(Query{Entity: "status", Limit: 1}, envelope(LineView{Status: "offline"}))
	requireFailure(t, err, "invalid_reply")
}

func TestFourReadContractsPreserveIDsGenerationsAndUnknownValues(t *testing.T) {
	summary := SmsSummary{ID: testID, Line: 9007199254740993, Binary: true}
	q := Query{Entity: "sms", Limit: 1}
	list, err := decodeRead(q, envelope(SmsPage{Messages: []SmsSummary{summary}, Cursor: Cursor{testEpoch, "1"}}))
	if err != nil {
		t.Fatal(err)
	}
	q.Entity, q.RecordID = "sms_detail", strptr(testID)
	detail, err := decodeRead(q, envelope(SmsDetail{Summary: summary, Sender: "", Timestamp: "", Text: "<script>合成😀</script>", Data: []uint16{0, 255}}))
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(list.Data.(SmsPage).Messages[0], detail.Data.(SmsDetail).Summary) {
		t.Fatal("list/detail identity changed")
	}
	encoded, _ := json.Marshal(detail)
	if !bytes.Contains(encoded, []byte(`"data":[0,255]`)) || bytes.Contains(encoded, []byte(testToken)) || bytes.Contains(encoded, []byte("<script>")) {
		t.Fatal("unsafe projection or binary conversion")
	}
	var output ReadReply
	if json.Unmarshal(encoded, &output) != nil {
		t.Fatal("invalid output")
	}
	for _, key := range []string{"sender", "timestamp"} {
		if output.Data.(map[string]any)[key] != "" {
			t.Fatal("invented unknown value")
		}
	}
	call := CallSummary{ID: testID, Line: summary.Line, State: "reconciling", ModemState: 99, Availability: "reconciling", MediaState: "none"}
	calls, err := decodeRead(Query{Entity: "calls", Limit: 1}, envelope(CallPage{Calls: []CallSummary{call}, Cursor: Cursor{testEpoch, "1"}}))
	if err != nil || calls.Data.(CallPage).Calls[0].Number != nil {
		t.Fatalf("unknown phone lost: %v", err)
	}
}

func strptr(s string) *string { return &s }

func TestInputOnlyAllowsFourReadQueries(t *testing.T) {
	valid := `{"token":"` + testToken + `","query":{"entity":"sms","after":{"db_epoch":"` + testEpoch + `","seq":"0"},"limit":16,"record_id":null}}`
	if _, _, err := readInput([]byte(valid)); err != nil {
		t.Fatal(err)
	}
	for _, entity := range []string{"status", "sms", "calls", "sms_detail"} {
		q := Query{Entity: entity, Limit: 1}
		if entity == "sms_detail" {
			q.RecordID = strptr(testID)
		}
		raw, _ := json.Marshal(struct {
			Token string `json:"token"`
			Query Query  `json:"query"`
		}{testToken, q})
		if _, _, err := readInput(raw); err != nil {
			t.Fatalf("%s: %v", entity, err)
		}
	}
	for _, raw := range []string{
		strings.Replace(valid, `"sms"`, `"send_sms"`, 1),
		strings.Replace(valid, `"sms"`, `"../calls"`, 1),
		strings.Replace(valid, `"sms"`, `"sms_detail"`, 1),
		strings.Replace(valid, `"limit":16`, `"limit":0`, 1),
		strings.Replace(valid, `"limit":16`, `"limit":17`, 1),
		strings.Replace(valid, `"limit":16`, `"limit":"16"`, 1),
		strings.Replace(valid, `"seq":"0"`, `"seq":"9223372036854775808"`, 1),
		strings.Replace(valid, `"record_id":null`, `"record_id":"`+testID+`"`, 1),
		strings.Replace(valid, testToken, strings.Repeat("A", 64), 1),
		strings.Replace(valid, testToken, strings.Repeat("g", 64), 1),
		strings.Replace(valid, `"query":`, `"Query":`, 1),
		valid + `{}`, strings.Repeat(" ", 8193),
	} {
		_, _, err := readInput([]byte(raw))
		requireFailure(t, err, "invalid_query")
	}
}

func TestRejectMalformedUnknownSensitiveAndOversizedReplies(t *testing.T) {
	valid := `{"adapter_api":1,"data_format":2,"data":{"line":0,"status":"ready"}}`
	cases := [][]byte{
		[]byte(`{`), []byte(`null`), []byte(valid + `{}`),
		[]byte(strings.Replace(valid, `"line":0`, `"line":null`, 1)),
		[]byte(strings.Replace(valid, `"line":0`, `"line":"0"`, 1)),
		[]byte(strings.Replace(valid, `"line":0,`, ``, 1)),
		[]byte(strings.Replace(valid, `"line":0`, `"Line":0`, 1)),
		[]byte(strings.Replace(valid, `"line":0`, `"line":0,"line":1`, 1)),
		[]byte(strings.Replace(valid, `"status":"ready"`, `"status":"ready","token":"secret"`, 1)),
		[]byte(strings.Replace(valid, `"status":"ready"`, `"status":"ready","iccid":"secret"`, 1)),
		[]byte(strings.Replace(valid, `"data":{`, `"data":{"seed":"secret",`, 1)),
		[]byte(strings.Replace(valid, `"status":"ready"`, `"status":"ready","future":true`, 1)),
		[]byte(strings.Repeat(" ", maxReply+1)),
		append([]byte(valid), 0xff),
		[]byte(strings.Repeat("[", 18) + "0" + strings.Repeat("]", 18)),
	}
	for i, raw := range cases {
		reply, err := decodeRead(Query{Entity: "status", Limit: 1}, raw)
		if reply != nil {
			t.Fatalf("case %d published invalid data", i)
		}
		requireFailure(t, err, "invalid_reply")
		if strings.Contains(err.Error(), "secret") {
			t.Fatal("remote data leaked")
		}
	}
	for _, raw := range []string{
		strings.Replace(valid, `"adapter_api":1`, `"adapter_api":2`, 1),
		strings.Replace(valid, `"data_format":2`, `"data_format":1`, 1),
	} {
		_, err := decodeRead(Query{Entity: "status", Limit: 1}, []byte(raw))
		requireFailure(t, err, "version_unsupported")
	}
}

func TestPaginationFailureReplayAndDetailCorrespondence(t *testing.T) {
	q := Query{Entity: "sms", Limit: 1, After: &Cursor{testEpoch, "1"}}
	summary := SmsSummary{ID: testID, Line: 7}
	page := SmsPage{Messages: []SmsSummary{summary}, Cursor: Cursor{testEpoch, "2"}}
	raw := envelope(page)
	first, err := decodeRead(q, raw)
	if err != nil {
		t.Fatal(err)
	}
	replayed, err := decodeRead(q, raw)
	if err != nil || !reflect.DeepEqual(first, replayed) || q.After.Seq != "1" {
		t.Fatal("replay changed data/checkpoint")
	}
	for _, seq := range []string{"0", "1", "9223372036854775808"} {
		page.Cursor.Seq = seq
		reply, err := decodeRead(q, envelope(page))
		if err == nil || reply != nil || q.After.Seq != "1" {
			t.Fatal("bad page advanced checkpoint")
		}
	}
	page.Cursor = Cursor{testID, "2"}
	_, err = decodeRead(q, envelope(page))
	requireFailure(t, err, "resync_required")
	page.Cursor = Cursor{testEpoch, "2"}
	page.Messages = []SmsSummary{summary, summary}
	if _, err := decodeRead(q, envelope(page)); err == nil {
		t.Fatal("duplicate page IDs accepted")
	}
	page.Messages[1].ID = testEpoch
	if _, err := decodeRead(q, envelope(page)); err == nil {
		t.Fatal("page exceeded requested limit")
	}
	page.Messages = []SmsSummary{}
	if _, err := decodeRead(q, envelope(page)); err == nil {
		t.Fatal("empty page skipped records")
	}
	page.Cursor.Seq = "1"
	if _, err := decodeRead(q, envelope(page)); err != nil {
		t.Fatal(err)
	}
	detailQ := Query{Entity: "sms_detail", Limit: 1, RecordID: strptr(testEpoch)}
	detail := SmsDetail{Summary: summary, Data: []uint16{}}
	if _, err := decodeRead(detailQ, envelope(detail)); err == nil {
		t.Fatal("wrong detail ID accepted")
	}
	detailQ.RecordID = strptr(testID)
	detail.Data = []uint16{256}
	if _, err := decodeRead(detailQ, envelope(detail)); err == nil {
		t.Fatal("invalid byte accepted")
	}
}

func TestMissingPresentationIsPreservedButMissingRequiredDataIsRejected(t *testing.T) {
	raw := `{"adapter_api":1,"data_format":2,"data":{"messages":[{"id":"` + testID + `","line":3,"binary":false,"duplicate_candidate":null}],"cursor":{"db_epoch":"` + testEpoch + `","seq":"1"}}}`
	q := Query{Entity: "sms", Limit: 1}
	reply, err := decodeRead(q, []byte(raw))
	if err != nil || reply.Data.(SmsPage).Messages[0].Display != nil {
		t.Fatalf("optional presentation invented: %v", err)
	}
	for _, bad := range []string{
		strings.Replace(raw, `"binary":false,`, ``, 1),
		strings.Replace(raw, `"binary":false`, `"binary":null`, 1),
		strings.Replace(raw, `"messages":[`, `"messages":null,"ignored":[`, 1),
		strings.Replace(raw, `"duplicate_candidate":null`, `"duplicate_candidate":3`, 1),
	} {
		if _, err := decodeRead(q, []byte(bad)); err == nil {
			t.Fatal("required/type error ignored")
		}
	}
}
