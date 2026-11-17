// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/binary"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

// Only the private launch is synthetic. Exercise the original scoped dialer,
// boot-clock checks and HTTP transport against real local sockets. This is not
// a Root registration or proof of public NKN/NAT/TURN connectivity.
func a2Dialer(t *testing.T, seed string) *scopedDialer {
	t.Helper()
	ctx, cancel := context.WithCancel(context.Background())
	now, err := bootMS()
	if err != nil {
		t.Fatal(err)
	}
	l := &Lease{launch: &launch{fixture: true, seeds: []string{seed}}, control: &net.UnixConn{}, ctx: ctx, cancel: cancel, deadlineBoot: now + 10000, lastBoot: now}
	d := &scopedDialer{lease: l, startDeadline: time.Now().Add(5 * time.Second), connections: make(map[*scopedConn]struct{})}
	t.Cleanup(func() { l.Close(); d.close() })
	return d
}

func TestA2HTTPSIPv4IPv6StrictTrustRedirectAndNoProxy(t *testing.T) {
	var proxyCalls atomic.Int32
	proxy := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { proxyCalls.Add(1); w.WriteHeader(502) }))
	defer proxy.Close()
	for _, name := range []string{"HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"} {
		t.Setenv(name, proxy.URL)
	}
	t.Setenv("NO_PROXY", "")
	t.Setenv("no_proxy", "")
	for _, address := range []string{"127.0.0.1:0", "[::1]:0"} {
		t.Run(address, func(t *testing.T) {
			var gets, redirectTarget atomic.Int32
			listener, err := net.Listen("tcp", address)
			if err != nil {
				t.Fatal("local IPv4/IPv6 endpoint required", err)
			}
			s := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				gets.Add(1)
				if r.URL.Path == "/redirect" {
					http.Redirect(w, r, "/must-not-follow", 302)
					return
				}
				if r.URL.Path == "/must-not-follow" {
					redirectTarget.Add(1)
				}
				_, _ = io.WriteString(w, "bounded local endpoint")
			}))
			_ = s.Listener.Close()
			s.Listener = listener
			s.Config.ErrorLog = log.New(io.Discard, "", 0)
			s.StartTLS()
			defer s.Close()
			d := a2Dialer(t, s.URL)
			c := newUsageHTTP(d)
			transport := c.Transport.(*http.Transport)
			transport.TLSClientConfig = s.Client().Transport.(*http.Transport).TLSClientConfig.Clone()
			if transport.Proxy != nil || transport.TLSClientConfig.InsecureSkipVerify {
				t.Fatal("TLS/proxy contract weakened")
			}
			r, err := c.Get(s.URL)
			if err != nil {
				t.Fatal(err)
			}
			_, _ = io.Copy(io.Discard, r.Body)
			_ = r.Body.Close()
			before := gets.Load()
			transport.TLSClientConfig = &tls.Config{RootCAs: x509.NewCertPool()}
			if r, err = c.Get(s.URL); err == nil {
				r.Body.Close()
				t.Fatal("untrusted certificate accepted")
			}
			transport.TLSClientConfig = s.Client().Transport.(*http.Transport).TLSClientConfig.Clone()
			transport.TLSClientConfig.ServerName = "wrong.example.invalid"
			if r, err = c.Get(s.URL); err == nil {
				r.Body.Close()
				t.Fatal("wrong certificate name accepted")
			}
			if gets.Load() != before {
				t.Fatal("failed TLS reached handler")
			}
			transport.TLSClientConfig = s.Client().Transport.(*http.Transport).TLSClientConfig.Clone()
			if r, err = c.Get(s.URL + "/redirect"); err == nil {
				r.Body.Close()
				t.Fatal("redirect followed")
			} else if r != nil {
				r.Body.Close()
			}
			if redirectTarget.Load() != 0 {
				t.Fatal("redirect target reached")
			}
			before = gets.Load()
			d.lease.Close()
			if r, err = c.Get(s.URL); err == nil {
				r.Body.Close()
				t.Fatal("closed original lease reused")
			}
			if gets.Load() != before {
				t.Fatal("closed lease sent another request")
			}
		})
	}
	if proxyCalls.Load() != 0 {
		t.Fatal("ambient proxy used")
	}
}

func TestA2WebSocketSingleAttemptAndExpiredOriginalConnection(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	var accepted atomic.Int32
	first := make(chan struct{})
	done := make(chan struct{})
	go func() {
		defer close(done)
		for {
			c, e := listener.Accept()
			if e != nil {
				return
			}
			if accepted.Add(1) == 1 {
				close(first)
			}
			c.Close()
		}
	}()
	d := a2Dialer(t, "")
	c, err := d.wsDial(context.Background(), "tcp", listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	select {
	case <-first:
	case <-time.After(time.Second):
		t.Fatal("first connection not accepted")
	}
	if other, err := d.wsDial(context.Background(), "tcp", listener.Addr().String()); err == nil {
		other.Close()
		t.Fatal("second SDK attempt accepted")
	}
	if d.lease.Check() == nil {
		t.Fatal("SDK retry did not close original lease")
	}
	if _, err = c.Write([]byte("must not escape")); err == nil {
		t.Fatal("old connection usable after close")
	}
	listener.Close()
	<-done
	if accepted.Load() != 1 {
		t.Fatal("unexpected connect count", accepted.Load())
	}
	// Expiry must fence an existing socket, not just the next dial.
	d = a2Dialer(t, "")
	local, remote := net.Pipe()
	defer remote.Close()
	c, err = d.tracked(local)
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	d.lease.mu.Lock()
	d.lease.deadlineBoot = d.lease.lastBoot
	d.lease.mu.Unlock()
	if _, err = c.Write([]byte("expired")); err == nil {
		t.Fatal("expired original lease wrote data")
	}
}

func TestA2MixedDNSAnswersRefuseBeforeAnyTCPConnect(t *testing.T) {
	listener, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer listener.Close()
	dns, err := net.ListenPacket("udp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer dns.Close()
	var questions atomic.Int32
	done := make(chan struct{})
	go func() {
		defer close(done)
		packet := make([]byte, 2048)
		for {
			n, peer, e := dns.ReadFrom(packet)
			if e != nil {
				return
			}
			if n < 17 {
				continue
			}
			q := append([]byte(nil), packet[:n]...)
			end := 12
			for end < n && q[end] != 0 {
				end += 1 + int(q[end])
			}
			end++
			if end+4 > n {
				continue
			}
			kind := binary.BigEndian.Uint16(q[end : end+2])
			q = q[:end+4]
			q[2] = 0x81
			q[3] = 0x80
			for i := 6; i < 12; i++ {
				q[i] = 0
			}
			questions.Add(1)
			if kind == 1 {
				q[7] = 2
				for _, ip := range [][4]byte{{127, 0, 0, 1}, {192, 0, 2, 1}} {
					q = append(q, 0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 1, 0, 4)
					q = append(q, ip[:]...)
				}
			}
			_, _ = dns.WriteTo(q, peer)
		}
	}()
	d := a2Dialer(t, "")
	d.lease.launch.dns = []string{dns.LocalAddr().String()}
	_, port, _ := net.SplitHostPort(listener.Addr().String())
	if c, err := d.dial(context.Background(), "tcp", "a2.invalid:"+port); err == nil {
		c.Close()
		t.Fatal("mixed allowed/forbidden DNS answers accepted")
	}
	if questions.Load() == 0 {
		t.Fatal("fixed DNS endpoint not queried")
	}
	listener.(*net.TCPListener).SetDeadline(time.Now().Add(50 * time.Millisecond))
	if c, err := listener.Accept(); err == nil {
		c.Close()
		t.Fatal("TCP connected before all answers validated")
	}
	dns.Close()
	<-done
}
