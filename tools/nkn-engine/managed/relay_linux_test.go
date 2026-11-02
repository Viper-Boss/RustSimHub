// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"encoding/binary"
	"io"
	"net"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func relayPair(t *testing.T) (*relay, *net.UnixConn, context.CancelFunc) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "owned-framing.sock")
	listener, e := net.ListenUnix("unix", &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { listener.Close() })
	c, e := net.DialUnix("unix", nil, &net.UnixAddr{Name: path, Net: "unix"})
	if e != nil {
		t.Fatal(e)
	}
	server, e := listener.AcceptUnix()
	if e != nil {
		t.Fatal(e)
	}
	t.Cleanup(func() { c.Close(); server.Close() })
	now, e := bootMS()
	if e != nil {
		t.Fatal(e)
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	t.Cleanup(cancel)
	lease := &Lease{launch: &launch{generation: 9}, ctx: ctx, cancel: cancel, deadlineBoot: now + 10000, lastBoot: now}
	r := &relay{lease: lease, conn: c, source: strings.Repeat("12", 32), phase: 4, seen: make(map[[8]byte][32]byte)}
	return r, server, cancel
}
func TestRelayDuplicateDoesNotRepeatCoreIOAndDifferentPeerCannotCancel(t *testing.T) {
	r, server, _ := relayPair(t)
	done := make(chan struct{})
	go func() {
		defer close(done)
		var h [4]byte
		if _, e := io.ReadFull(server, h[:]); e != nil {
			return
		}
		b := make([]byte, binary.BigEndian.Uint32(h[:]))
		if _, e := io.ReadFull(server, b); e != nil {
			return
		}
		server.Write(h[:])
		server.Write(b)
	}()
	id := [8]byte{1}
	data := []byte{1, 2, 4, 5}
	reply, e := r.exchange(r.source, id, data)
	if e != nil || string(reply) != string(data) {
		t.Fatal("opaque exchange failed", e)
	}
	<-done
	if reply, e := r.exchange(r.source, id, data); e != nil || reply != nil {
		t.Fatal("duplicate repeated a Core operation")
	}
	if reply, e := r.exchange(strings.Repeat("34", 32), id, []byte{1, 2, 9}); e != nil || reply != nil || r.current() == nil {
		t.Fatal("another peer cancelled current source")
	}
	if _, e := r.exchange(r.source, id, []byte{1, 2, 9}); e == nil || r.current() != nil {
		t.Fatal("mutated duplicate retained source")
	}
	server.SetReadDeadline(time.Now().Add(time.Second))
	var b [1]byte
	if n, _ := server.Read(b[:]); n != 0 {
		t.Fatal("uncertain request was retried")
	}
}
func TestRelayRejectsWrongCoreReplyAndClosesActualDescriptor(t *testing.T) {
	r, server, _ := relayPair(t)
	go func() {
		var h [4]byte
		io.ReadFull(server, h[:])
		data := make([]byte, binary.BigEndian.Uint32(h[:]))
		io.ReadFull(server, data)
		server.Write([]byte{0, 0, 0, 2, 1, 1})
	}()
	if _, e := r.exchange(r.source, [8]byte{2}, []byte{1, 2, 7}); e == nil || r.current() != nil {
		t.Fatal("wrong kind became a reply")
	}
	server.SetReadDeadline(time.Now().Add(time.Second))
	var b [1]byte
	if _, e := server.Read(b[:]); e == nil {
		t.Fatal("failed descriptor retained")
	}
}
func TestRelayRootCancellationDiscardsLateReplyAndReleasesRead(t *testing.T) {
	r, server, cancel := relayPair(t)
	entered := make(chan struct{})
	go func() {
		var h [4]byte
		io.ReadFull(server, h[:])
		data := make([]byte, binary.BigEndian.Uint32(h[:]))
		io.ReadFull(server, data)
		close(entered)
	}()
	done := make(chan error, 1)
	go func() { _, e := r.exchange(r.source, [8]byte{3}, []byte{1, 2, 7}); done <- e }()
	<-entered
	cancel()
	r.close()
	select {
	case e := <-done:
		if e == nil {
			t.Fatal("cancelled exchange succeeded")
		}
	case <-time.After(time.Second):
		t.Fatal("cancellation retained blocking Core read")
	}
}
