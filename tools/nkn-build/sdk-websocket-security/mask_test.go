// SPDX-License-Identifier: AGPL-3.0-only
package sdkevaluation

import (
	"bufio"
	"crypto/sha1"
	"encoding/base64"
	"encoding/binary"
	"fmt"
	"io"
	"math/rand"
	"net"
	"net/http"
	"os"
	"testing"
	"time"

	"github.com/gorilla/websocket"
)

// This runs separately from SDK lifecycle tests: no other goroutine consumes
// math/rand. It observes real outgoing client frame bytes on an owned net.Pipe.
func TestActualClientFrameMaskSource(t *testing.T) {
	const seed int64 = 424242
	const frames = 16
	if os.Getenv("RSH_WS_EXPECT_PREDICTABLE") == "" {
		t.Fatal("explicit baseline/candidate expectation required")
	}
	// Only this isolated probe opts into legacy Seed behavior through GODEBUG.
	// The local PRNG control proves Seed took effect; product flags stay intact.
	rand.Seed(seed)
	if rand.Uint32() != rand.New(rand.NewSource(seed)).Uint32() {
		t.Fatal("controlled legacy PRNG fixture was not enabled")
	}
	client, server := net.Pipe()
	t.Cleanup(func() { _ = client.Close(); _ = server.Close() })
	_ = client.SetDeadline(time.Now().Add(2 * time.Second))
	_ = server.SetDeadline(time.Now().Add(2 * time.Second))
	type result struct {
		masks [frames][4]byte
		err   error
	}
	results := make(chan result, 1)
	go func() {
		var r result
		defer func() { results <- r }()
		reader := bufio.NewReader(server)
		request, err := http.ReadRequest(reader)
		if err != nil {
			r.err = err
			return
		}
		accept := sha1.Sum([]byte(request.Header.Get("Sec-Websocket-Key") + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))
		_, err = fmt.Fprintf(server, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-Websocket-Accept: %s\r\n\r\n", base64.StdEncoding.EncodeToString(accept[:]))
		if err != nil {
			r.err = err
			return
		}
		for i := range r.masks {
			var header [6]byte
			if _, err = io.ReadFull(reader, header[:]); err != nil {
				r.err = err
				return
			}
			if header[0] != 0x81 || header[1] != 0x89 {
				r.err = fmt.Errorf("unexpected bounded text frame header")
				return
			}
			copy(r.masks[i][:], header[2:])
			var body [9]byte
			if _, err = io.ReadFull(reader, body[:]); err != nil {
				r.err = err
				return
			}
			for j := range body {
				body[j] ^= r.masks[i][j%4]
			}
			if string(body[:]) != "synthetic" {
				r.err = fmt.Errorf("actual frame did not preserve synthetic payload")
				return
			}
		}
	}()
	connection, _, err := (&websocket.Dialer{NetDial: func(string, string) (net.Conn, error) {
		return client, nil // No DNS or external socket: only the pipe above.
	}}).Dial("ws://mask-fixture.invalid/", nil)
	if err != nil {
		t.Fatal("owned handshake failed")
	}
	defer connection.Close()
	rand.Seed(seed)
	for i := 0; i < frames; i++ {
		if err = connection.WriteMessage(websocket.TextMessage, []byte("synthetic")); err != nil {
			t.Fatal("owned client frame write failed")
		}
	}
	r := <-results
	if r.err != nil {
		t.Fatal(r.err)
	}
	predictor := rand.New(rand.NewSource(seed))
	predictable := true
	for _, actual := range r.masks {
		var expected [4]byte
		binary.LittleEndian.PutUint32(expected[:], predictor.Uint32())
		predictable = predictable && actual == expected
	}
	wantPredictable := os.Getenv("RSH_WS_EXPECT_PREDICTABLE") == "1"
	if predictable != wantPredictable {
		t.Fatal("actual client mask source did not match the explicit version expectation")
	}
}
