// SPDX-License-Identifier: AGPL-3.0-only
// Isolated fixture counterpart: the actual full SDK carries actual opaque
// Noise records to the Root-owned production SDK. No public network is used.
package main

import (
	"context"
	"crypto/rand"
	"encoding/binary"
	"fmt"
	sdk "github.com/nknorg/nkn-sdk-go"
	"github.com/nknorg/nkngomobile"
	"io"
	"net"
	"os"
	"time"
)

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
func run() error {
	account, e := sdk.NewAccount(makeSeed())
	if e != nil {
		return e
	}
	dial := func(ctx context.Context, network, address string) (net.Conn, error) {
		if network != "tcp" || address != "9.9.9.9:8443" && address != "9.9.9.9:8000" {
			return nil, fmt.Errorf("owned counterpart target denied")
		}
		return (&net.Dialer{Timeout: time.Second}).DialContext(ctx, network, address)
	}
	client, e := sdk.NewClient(account, "", &sdk.ClientConfig{ManagedMode: true, HttpDialContext: dial, WsDialContext: dial, MsgChanLen: 4, WsHandshakeTimeout: 1000, WsWriteTimeout: 1000, SeedRPCServerAddr: nkngomobile.NewStringArray("https://9.9.9.9:8443"), ConnectRetries: 1, RPCConcurrency: 1, RPCTimeout: 1000})
	if e != nil {
		return e
	}
	defer client.Close()
	closed := false
	defer func() {
		if !closed {
			var id [8]byte
			if _, err := rand.Read(id[:]); err == nil {
				_, _ = client.SendBinary(nkngomobile.NewStringArray(stringDestination()), []byte{1, 3}, &sdk.MessageConfig{NoReply: true, MessageID: id[:], MaxHoldingSeconds: 0})
			}
		}
	}()
	select {
	case <-client.OnConnect.C:
	case <-time.After(8 * time.Second):
		return io.ErrUnexpectedEOF
	}
	dest, e := os.ReadFile("/tmp/owned-sdk-address")
	if e != nil {
		return e
	}
	listener, e := net.Listen("unix", "/tmp/sdk-counterpart/core.sock")
	if e != nil {
		return e
	}
	defer listener.Close()
	c, e := listener.Accept()
	if e != nil {
		return e
	}
	defer c.Close()
	_ = c.SetDeadline(time.Now().Add(20 * time.Second))
	for n := uint64(1); n <= 16; n++ {
		var size [4]byte
		if _, e = io.ReadFull(c, size[:]); e == io.EOF {
			return nil
		}
		if e != nil {
			return e
		}
		length := binary.BigEndian.Uint32(size[:])
		if length < 2 || length > 2050 {
			return io.ErrUnexpectedEOF
		}
		data := make([]byte, length)
		if _, e = io.ReadFull(c, data); e != nil {
			return e
		}
		var id [8]byte
		if _, e = rand.Read(id[:]); e != nil {
			return e
		}
		_, e = client.SendBinary(nkngomobile.NewStringArray(string(dest)), data, &sdk.MessageConfig{NoReply: true, MessageID: id[:], MaxHoldingSeconds: 0})
		if e != nil {
			return e
		}
		if data[1] == 3 {
			closed = true
			return nil
		}
		select {
		case message := <-client.OnMessage.C:
			if message == nil || !message.Encrypted || message.Src != string(dest) || len(message.Data) < 2 || len(message.Data) > 2050 {
				return io.ErrUnexpectedEOF
			}
			binary.BigEndian.PutUint32(size[:], uint32(len(message.Data)))
			if _, e = c.Write(size[:]); e != nil {
				return e
			}
			if _, e = c.Write(message.Data); e != nil {
				return e
			}
		case <-time.After(2 * time.Second):
			return io.ErrUnexpectedEOF
		}
	}
	return nil
}
func makeSeed() []byte {
	seed := make([]byte, 32)
	if _, e := rand.Read(seed); e != nil {
		panic(e)
	}
	return seed
}
func stringDestination() string {
	value, _ := os.ReadFile("/tmp/owned-sdk-address")
	return string(value)
}
