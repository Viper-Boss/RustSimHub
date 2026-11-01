// SPDX-License-Identifier: AGPL-3.0-only
package sdkevaluation

import (
	"context"
	"errors"
	"net"
	"strings"
	"sync/atomic"
	"testing"

	sdk "github.com/nknorg/nkn-sdk-go"
)

func rejectWebRTC(t *testing.T, config *sdk.ClientConfig) {
	t.Helper()
	before := reconnectWorkers()
	var attempts atomic.Int32
	deny := func(context.Context, string, string) (net.Conn, error) {
		attempts.Add(1)
		return nil, errors.New("unexpected dial in rejected WebRTC candidate")
	}
	config.HttpDialContext, config.WsDialContext = deny, deny
	a := account(t)
	for _, multi := range []bool{false, true} {
		var err error
		if multi {
			var client *sdk.MultiClient
			client, err = sdk.NewMultiClient(a, "synthetic", 2, true, config)
			if client != nil {
				_ = client.Close()
				t.Fatal("WebRTC multiclient was allocated")
			}
		} else {
			var client *sdk.Client
			client, err = sdk.NewClient(a, "synthetic", config)
			if client != nil {
				_ = client.Close()
				t.Fatal("WebRTC client was allocated")
			}
		}
		if err == nil || !strings.Contains(err.Error(), "WebRTC is unavailable") {
			t.Fatal("WebRTC did not fail explicitly at configuration merge")
		}
		if attempts.Load() != 0 || reconnectWorkers() != before {
			t.Fatal("rejected configuration started dial/worker activity")
		}
	}
}

func TestExplicitWebRTCRejectedBeforeClientActivity(t *testing.T) {
	rejectWebRTC(t, &sdk.ClientConfig{WebRTC: true})
}

func TestInheritedWebRTCDefaultRejectedBeforeClientActivity(t *testing.T) {
	before := sdk.DefaultClientConfig
	t.Cleanup(func() { sdk.DefaultClientConfig = before })
	sdk.DefaultClientConfig.WebRTC = true
	// A zero caller flag must not silently override an enabled inherited default.
	rejectWebRTC(t, &sdk.ClientConfig{})
}
