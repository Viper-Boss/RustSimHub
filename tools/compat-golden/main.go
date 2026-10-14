// SPDX-License-Identifier: AGPL-3.0-only
// Run against the pinned, unmodified AirSimHub proto module; every key is synthetic.
package main

import (
	"crypto/ed25519"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"os"
	proto "github.com/human-agent65535/nassimhub-node/proto"
)

type signingVector struct {
	Method string `json:"method"`
	Path string `json:"path"`
	Timestamp string `json:"timestamp"`
	Nonce string `json:"nonce"`
	Body string `json:"body_hex"`
	Canonical string `json:"canonical_hex"`
	Public string `json:"public_hex"`
	Signature string `json:"signature"`
	Device string `json:"device_id"`
	Fingerprint string `json:"fingerprint"`
}
type versionVector struct {
	Node proto.Compatibility `json:"node"`
	Core proto.CoreRequirements `json:"core"`
	Result string `json:"result"`
}
func main() {
	seed := make([]byte,32)
	for i := range seed { seed[i]=77 }
	key:=ed25519.NewKeyFromSeed(seed)
	public:=key.Public().(ed25519.PublicKey)
	var signing []signingVector
	for _, body := range []string{"", "{}", "{\"core_id\":\"synthetic\"}", "{\"text\":\"合成短信，仅测试\"}", "\x00\xff\n"} {
		v:=signingVector{Method:"post",Path:"/v1/session",Timestamp:"2026-10-14T02:00:00Z",Nonce:"synthetic-nonce-0123456789",Body:hex.EncodeToString([]byte(body)),Public:hex.EncodeToString(public)}
		canonical:=proto.SigningString(v.Method,v.Path,v.Timestamp,v.Nonce,[]byte(body))
		v.Canonical=hex.EncodeToString(canonical)
		v.Signature=base64.StdEncoding.EncodeToString(ed25519.Sign(key,canonical))
		v.Device=proto.DeviceID(proto.PlatformMSM8916,public)
		v.Fingerprint=proto.CoreFingerprint(public)
		signing=append(signing,v)
	}
	var versions []versionVector
	for _, spec := range []struct{major,minor int; agent,minCore,minimumAgent,core string}{
		{1,5,"0.1.0","0.1.0","0.1.0","0.1.0"},
		{2,5,"0.1.0","0.1.0","0.1.0","0.1.0"},
		{1,4,"0.1.0","0.1.0","0.1.0","0.1.0"},
		{1,5,"0.0.9","0.1.0","0.1.0","0.1.0"},
		{1,5,"0.1.0","0.2.0","0.1.0","0.1.0"},
		{1,5,"0.1.0","nonsense","0.1.0","0.1.0"},
		{1,5,"unversioned-local-label","0.1.0","0.1.0","0.1.0"},
		{1,5,"dev","0.1.0","0.1.0","dev"},
		{1,5,"v1.2.3-rc1+build7","1.2","0.1.0","1.2.0"},
		{1,5,"0.1.0","0.1.0","nonsense","0.1.0"},
		{1,5,"nonsense","","nonsense","nonsense"},
		{1,5,"0.1.0","0.1.0","0.1.0","nonsense"},
	} {
		v:=versionVector{Node:proto.Compatibility{ProtocolMajor:spec.major,ProtocolMinor:spec.minor,AgentVersion:spec.agent,MinCoreVersion:spec.minCore},Core:proto.CoreRequirements{ProtocolMajor:1,MinProtocolMinor:5,MinAgentVersion:spec.minimumAgent,CoreVersion:spec.core},Result:"ok"}
		if err:=proto.Negotiate(v.Node,v.Core); err!=nil {
			var incompatible *proto.IncompatibleError
			if !errors.As(err,&incompatible) {panic("unexpected transport error")}
			v.Result=string(incompatible.Reason)
		}
		versions=append(versions,v)
	}
	encoder:=json.NewEncoder(os.Stdout)
	encoder.SetIndent("","  ")
	if err:=encoder.Encode(map[string]any{"source_revision":"ea019485e5d890a96acae357c52b94c746de837f","signing":signing,"versions":versions});err!=nil {panic(err)}
}
