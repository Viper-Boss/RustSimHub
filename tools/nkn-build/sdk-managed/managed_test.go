// SPDX-License-Identifier: AGPL-3.0-only
package nkn

import (
	"context"
	"encoding/json"
	"net"
	"testing"

	"github.com/gorilla/websocket"
	"github.com/nknorg/nkn-sdk-go/payloads"
)

func parserClient(managed bool) *Client {
	ctx, cancel := context.WithCancel(context.Background())
	return &Client{config: &ClientConfig{ManagedMode: managed}, lifecycle: ctx, cancelLifecycle: cancel,
		OnConnect: NewOnConnect(1, nil), OnMessage: NewOnMessage(1, nil), reconnectChan: make(chan *Node),
		chChallengeSignature: make(chan *stSaltAndSignature, 1), sharedKeys: make(map[string]*[sharedKeySize]byte)}
}
func TestManagedMissingFieldsActuallyReproducesOldParserPanic(t *testing.T) {
	old := parserClient(false)
	defer old.Close()
	panicked := false
	func() {
		defer func() { panicked = recover() != nil }()
		_ = old.handleMessage(websocket.TextMessage, []byte(`{}`))
	}()
	if !panicked {
		t.Fatal("old reachable panic was not reproduced")
	}
	for _, data := range []string{`{}`, `null`, `{"Action":"setClient","Error":0}`, `{"Action":"authChallenge","Error":0}`, `{"Action":"updateSigChainBlockHash","Error":0}`, `{"Action":"x","Error":48001}`} {
		c := parserClient(true)
		func() {
			defer c.Close()
			if e := c.handleMessage(websocket.TextMessage, []byte(data)); e == nil {
				t.Fatal("missing managed field accepted")
			}
		}()
	}
}
func TestManagedShortCipherAndNonceAreRejectedBeforeKeyWork(t *testing.T) {
	c := parserClient(true)
	defer c.Close()
	for _, msg := range []*payloads.Message{{Payload: []byte{1}}, {Payload: make([]byte, 16), Nonce: make([]byte, 23)}, {Payload: make([]byte, 16), Nonce: make([]byte, 48), EncryptedKey: []byte{1}}, {Payload: make([]byte, 3073), Nonce: make([]byte, 24)}} {
		if _, e := c.decryptPayload(msg, "invalid"); e == nil {
			t.Fatal("unsafe encrypted payload accepted")
		}
	}
	if len(c.sharedKeys) != 0 {
		t.Fatal("invalid cipher performed peer key work")
	}
}
func TestManagedReconnectClosesInsteadOfSchedulingAutomaticRetry(t *testing.T) {
	c := parserClient(true)
	c.Reconnect(nil)
	if !c.IsClosed() || c.lifecycle.Err() == nil {
		t.Fatal("managed retry escaped outer supervisor")
	}
	c.Reconnect(nil) // Idempotent, without a send on the closed retry channel.
}
func TestManagedConfigRequiresExplicitFiniteHostPolicy(t *testing.T) {
	dial := func(context.Context, string, string) (net.Conn, error) { return nil, ErrClosed }
	valid := func() *ClientConfig {
		return &ClientConfig{ManagedMode: true, SeedRPCServerAddr: NewStringArray("https://seed.invalid"), HttpDialContext: dial, WsDialContext: dial, MsgChanLen: 4, ConnectRetries: 1, RPCConcurrency: 1, RPCTimeout: 1000, WsHandshakeTimeout: 1000, WsWriteTimeout: 1000}
	}
	if _, e := MergeClientConfig(valid()); e != nil {
		t.Fatal(e)
	}
	for _, change := range []func(*ClientConfig){func(c *ClientConfig) { c.HttpDialContext = nil }, func(c *ClientConfig) { c.WsDialContext = nil }, func(c *ClientConfig) { c.MsgChanLen = 5 }, func(c *ClientConfig) { c.ConnectRetries = -1 }, func(c *ClientConfig) { c.AllowUnencrypted = true }, func(c *ClientConfig) { c.SeedRPCServerAddr = nil }} {
		c := valid()
		change(c)
		if _, e := MergeClientConfig(c); e == nil {
			b, _ := json.Marshal(c.ManagedMode)
			t.Fatal("unbounded managed configuration accepted", string(b))
		}
	}
}
