// SPDX-License-Identifier: AGPL-3.0-only

package main

import (
	"bytes"
	"context"
	"crypto/tls"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/netip"
	"os"
	"strings"
	"syscall"
	"time"
)

const endpoint = "https://mainnet-rpc-node-0001.nkn.org/mainnet/api/wallet"
const requestID = "rustsimhub-usage-checkin-v1"
const limit = 4096

type rpcChain struct {
	client *http.Client
}

func bindWiFi(_, _ string, conn syscall.RawConn) error {
	var bindErr error
	if err := conn.Control(func(fd uintptr) {
		bindErr = syscall.SetsockoptString(int(fd), syscall.SOL_SOCKET, syscall.SO_BINDTODEVICE, "wlan0")
	}); err != nil {
		return err
	}
	return bindErr
}

func newChain() *rpcChain {
	dial := &net.Dialer{Control: bindWiFi, Timeout: 4 * time.Second}
	connect := func(ctx context.Context, network, address string) (net.Conn, error) {
		host, port, err := net.SplitHostPort(address)
		if err != nil || host != "mainnet-rpc-node-0001.nkn.org" || port != "443" {
			return nil, errors.New("invalid statistics RPC destination")
		}
		config, err := os.ReadFile("/etc/resolv.conf")
		if err != nil {
			return nil, err
		}
		ip, err := resolveWiFi(ctx, host, config)
		if err != nil {
			return nil, err
		}
		return dial.DialContext(ctx, "tcp4", net.JoinHostPort(ip.String(), port))
	}
	return &rpcChain{client: &http.Client{
		Timeout: 7 * time.Second,
		Transport: &http.Transport{
			DialContext:            connect,
			TLSClientConfig:        &tls.Config{MinVersion: tls.VersionTLS12},
			DisableCompression:     true,
			MaxResponseHeaderBytes: 8192,
			MaxConnsPerHost:        1,
			IdleConnTimeout:        2 * time.Second,
		},
		CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse },
	}}
}

func resolveWiFi(ctx context.Context, host string, config []byte) (netip.Addr, error) {
	ctx, cancel := context.WithCancel(ctx)
	defer cancel()
	answers := make(chan netip.Addr, 3)
	servers := make(map[netip.Addr]bool)
	dns := &net.Dialer{Control: bindWiFi, Timeout: 2 * time.Second}
	for _, line := range strings.Split(string(config), "\n") {
		fields := strings.Fields(line)
		if len(fields) < 2 || fields[0] != "nameserver" {
			continue
		}
		ip, err := netip.ParseAddr(fields[1])
		if err != nil || !ip.Is4() || ip.IsLoopback() || ip.IsUnspecified() || ip.IsMulticast() || servers[ip] {
			continue
		}
		servers[ip] = true
		server := net.JoinHostPort(ip.String(), "53")
		go func() {
			resolver := &net.Resolver{PreferGo: true, Dial: func(ctx context.Context, network, _ string) (net.Conn, error) {
				return dns.DialContext(ctx, network, server)
			}}
			resolved, _ := resolver.LookupNetIP(ctx, "ip4", host+".")
			answer := netip.Addr{}
			if len(resolved) > 0 {
				answer = resolved[0]
			}
			answers <- answer
		}()
		if len(servers) == cap(answers) {
			break
		}
	}
	for range len(servers) {
		select {
		case answer := <-answers:
			if answer.IsValid() {
				return answer, nil
			}
		case <-ctx.Done():
			return netip.Addr{}, ctx.Err()
		}
	}
	return netip.Addr{}, errors.New("statistics Wi-Fi DNS unavailable")
}

func (c *rpcChain) call(ctx context.Context, method string, params map[string]string, out any) error {
	body, err := json.Marshal(map[string]any{
		"jsonrpc": "2.0", "id": requestID, "method": method, "params": params,
	})
	if err != nil {
		return err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, endpoint, bytes.NewReader(body))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := c.client.Do(req)
	if err != nil {
		return fmt.Errorf("statistics %s unavailable over Wi-Fi", method)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("statistics %s returned HTTP %d", method, resp.StatusCode)
	}
	raw, err := io.ReadAll(io.LimitReader(resp.Body, limit+1))
	if err != nil || len(raw) > limit {
		return errors.New("invalid statistics RPC response length")
	}
	var envelope struct {
		ID      string          `json:"id"`
		Version string          `json:"jsonrpc"`
		Result  json.RawMessage `json:"result"`
		Error   *struct {
			Code int `json:"code"`
		} `json:"error"`
	}
	if json.Unmarshal(raw, &envelope) != nil || envelope.ID != requestID || envelope.Version != "2.0" {
		return errors.New("invalid statistics RPC response")
	}
	if envelope.Error != nil {
		return fmt.Errorf("statistics %s rejected by NKN (code %d)", method, envelope.Error.Code)
	}
	if len(envelope.Result) == 0 || bytes.Equal(envelope.Result, []byte("null")) ||
		json.Unmarshal(envelope.Result, out) != nil {
		return errors.New("statistics RPC response has no valid result")
	}
	return nil
}
