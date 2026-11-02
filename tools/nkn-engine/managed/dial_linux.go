// SPDX-License-Identifier: AGPL-3.0-only
package managed

import (
	"context"
	"net"
	"net/netip"
	"net/url"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

var forbidden = func() []netip.Prefix {
	texts := []string{"0.0.0.0/8", "10.0.0.0/8", "100.64.0.0/10", "127.0.0.0/8", "169.254.0.0/16", "172.16.0.0/12", "192.0.0.0/24", "192.0.2.0/24", "192.88.99.0/24", "192.168.0.0/16", "198.18.0.0/15", "198.51.100.0/24", "203.0.113.0/24", "224.0.0.0/4", "240.0.0.0/4", "::/96", "64:ff9b::/96", "64:ff9b:1::/48", "100::/64", "2001::/32", "2001:2::/48", "2001:10::/28", "2001:20::/28", "2001:db8::/32", "2002::/16", "3fff::/20"}
	var result []netip.Prefix
	for _, text := range texts {
		result = append(result, netip.MustParsePrefix(text))
	}
	return result
}()

func allowedIP(ip netip.Addr, fixture bool) bool {
	ip = ip.Unmap()
	if fixture {
		return ip.IsLoopback()
	}
	if !ip.IsGlobalUnicast() || ip.IsPrivate() || ip.IsLoopback() || ip.IsLinkLocalUnicast() {
		return false
	}
	for _, prefix := range forbidden {
		if prefix.Contains(ip) {
			return false
		}
	}
	return true
}

type scopedDialer struct {
	lease         *Lease
	startDeadline time.Time
	wsAttempts    atomic.Uint32
	mu            sync.Mutex
	closed        bool
	connections   map[*scopedConn]struct{}
}
type scopedConn struct {
	net.Conn
	owner *scopedDialer
	once  sync.Once
}

func (c *scopedConn) Close() error {
	var err error
	c.once.Do(func() { err = c.Conn.Close(); c.owner.mu.Lock(); delete(c.owner.connections, c); c.owner.mu.Unlock() })
	return err
}
func (c *scopedConn) Read(data []byte) (int, error) {
	if c.owner.lease.Check() != nil {
		return 0, ErrLaunch
	}
	n, e := c.Conn.Read(data)
	if c.owner.lease.Check() != nil {
		clear(data[:n])
		return 0, ErrLaunch
	}
	return n, e
}
func (c *scopedConn) Write(data []byte) (int, error) {
	if c.owner.lease.Check() != nil {
		return 0, ErrLaunch
	}
	n, e := c.Conn.Write(data)
	if c.owner.lease.Check() != nil {
		return n, ErrLaunch
	}
	return n, e
}
func (d *scopedDialer) close() {
	d.mu.Lock()
	d.closed = true
	all := make([]*scopedConn, 0, len(d.connections))
	for c := range d.connections {
		all = append(all, c)
	}
	d.mu.Unlock()
	for _, c := range all {
		_ = c.Close()
	}
}
func (d *scopedDialer) tracked(c net.Conn) (net.Conn, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.closed || len(d.connections) >= 8 || d.lease.Check() != nil {
		c.Close()
		return nil, ErrLaunch
	}
	owned := &scopedConn{Conn: c, owner: d}
	d.connections[owned] = struct{}{}
	return owned, nil
}
func (d *scopedDialer) bounded(parent context.Context) (context.Context, func(), error) {
	if d.lease.Check() != nil {
		return nil, nil, ErrLaunch
	}
	deadline := time.Now().Add(2 * time.Second)
	if d.wsAttempts.Load() == 0 && d.startDeadline.Before(deadline) {
		deadline = d.startDeadline
	}
	ctx, cancel := context.WithDeadline(parent, deadline)
	stop := context.AfterFunc(d.lease.Context(), cancel)
	return ctx, func() { stop(); cancel() }, nil
}
func (d *scopedDialer) httpDial(ctx context.Context, network, address string) (net.Conn, error) {
	allowed := false
	for _, seed := range d.lease.launch.seeds {
		u, _ := url.Parse(seed)
		port := u.Port()
		if port == "" {
			port = "443"
		}
		if strings.EqualFold(address, net.JoinHostPort(u.Hostname(), port)) {
			allowed = true
		}
	}
	if !allowed {
		return nil, ErrLaunch
	}
	return d.dial(ctx, network, address)
}
func (d *scopedDialer) wsDial(ctx context.Context, network, address string) (net.Conn, error) {
	// The outer Rust supervisor owns the only reconnect budget. SDK-internal
	// reconnect must not create a second automatic retry/authorization loop.
	if d.wsAttempts.Add(1) != 1 {
		d.lease.Close()
		return nil, ErrLaunch
	}
	return d.dial(ctx, network, address)
}
func (d *scopedDialer) dial(parent context.Context, network, address string) (net.Conn, error) {
	if network != "tcp" && network != "tcp4" && network != "tcp6" {
		return nil, ErrLaunch
	}
	ctx, release, e := d.bounded(parent)
	if e != nil {
		return nil, e
	}
	defer release()
	host, port, e := net.SplitHostPort(address)
	if e != nil || !asciiHost(host) {
		return nil, ErrLaunch
	}
	var ips []netip.Addr
	if ip, e := netip.ParseAddr(host); e == nil {
		ips = []netip.Addr{ip}
	} else {
		resolver := &net.Resolver{PreferGo: true, StrictErrors: true, Dial: func(ctx context.Context, kind, _ string) (net.Conn, error) {
			if kind != "udp" && kind != "tcp" || d.lease.Check() != nil {
				return nil, ErrLaunch
			}
			dns := d.lease.launch.dns[0]
			server, e := netip.ParseAddrPort(dns)
			if e != nil || server.Addr().IsLoopback() != d.lease.launch.fixture {
				return nil, ErrLaunch
			}
			c, e := (&net.Dialer{Timeout: time.Second}).DialContext(ctx, kind, dns)
			if e != nil {
				return nil, ErrLaunch
			}
			return d.tracked(c)
		}}
		ips, e = resolver.LookupNetIP(ctx, "ip", host+".")
		if e != nil || len(ips) == 0 || len(ips) > 16 {
			return nil, ErrLaunch
		}
	}
	// Validate ALL answers before any connect, then make ONE attempt. No ambient
	// DNS, proxy, alternate family, private/LAN or cellular fallback is permitted.
	for _, ip := range ips {
		if !allowedIP(ip, d.lease.launch.fixture) {
			return nil, ErrLaunch
		}
	}
	if d.lease.Check() != nil {
		return nil, ErrLaunch
	}
	c, e := (&net.Dialer{Timeout: time.Second}).DialContext(ctx, network, net.JoinHostPort(ips[0].String(), port))
	if e != nil {
		return nil, ErrLaunch
	}
	return d.tracked(c)
}
