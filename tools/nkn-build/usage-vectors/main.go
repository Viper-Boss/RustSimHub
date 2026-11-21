// SPDX-License-Identifier: AGPL-3.0-only
// Offline reference probe, never a production signer or RPC transport.
package main

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"net/http"
	"os"

	nkn "github.com/nknorg/nkn-sdk-go"
	"github.com/nknorg/nkn/v2/transaction"
)

var negative = flag.String("negative", "", "test-only mutation: intent-hash, single-hash, big-endian, sdk-attributes")

type denyNetwork struct{ calls int }

func (d *denyNetwork) RoundTrip(*http.Request) (*http.Response, error) {
	d.calls++
	return nil, errors.New("offline reference probe forbids HTTP")
}

// This encoder uses no NKN protobuf/serialization helpers. It is checked against
// the fixed upstream constructor, unsigned serializer, SDK signer and marshaler.
func varint(n uint64) []byte {
	return binary.AppendUvarint(nil, n)
}

func field(tag byte, data []byte) []byte {
	r := append([]byte{tag}, varint(uint64(len(data)))...)
	return append(r, data...)
}

func chainBytes(data []byte) []byte {
	n := len(data)
	var prefix []byte
	switch {
	case n < 253:
		prefix = []byte{byte(n)}
	case n <= 65535:
		prefix = binary.LittleEndian.AppendUint16([]byte{253}, uint16(n))
	default:
		panic("reference vector outside bounded size")
	}
	return append(prefix, data...)
}

type intent struct {
	Version            int    `json:"version"`
	PublicKey          string `json:"public_key"`
	Topic              string `json:"topic"`
	Window             string `json:"window"`
	SubscriptionBlocks uint32 `json:"subscription_blocks"`
	RenewalPeriod      uint64 `json:"renewal_period"`
}

type vector struct {
	Intent      intent `json:"intent"`
	Nonce       uint64 `json:"nonce"`
	Meta        string `json:"meta"`
	Unsigned    string `json:"unsigned_hex"`
	SigningHash string `json:"signing_hash_hex"`
	Signature   string `json:"signature_hex"`
	Transaction string `json:"transaction_hex"`
	Digest      string `json:"digest_hex"`
}

func check(equal bool, message string) {
	if !equal {
		panic(message)
	}
}

func one(seed, public []byte, wallet *nkn.Wallet, in intent, nonce uint64) vector {
	meta, err := json.Marshal(in)
	check(err == nil, "intent encoding")
	// SUBSCRIBE protobuf: subscriber=1, identifier=2 omitted, topic=3,
	// deprecated bucket=4 omitted, duration=5, meta=6. Fee and attributes empty.
	payload := field(0x0a, public)
	payload = append(payload, field(0x1a, []byte(in.Topic))...)
	payload = append(payload, append([]byte{0x28}, varint(uint64(in.SubscriptionBlocks))...)...)
	payload = append(payload, field(0x32, meta)...)
	unsigned := binary.LittleEndian.AppendUint32(nil, 6)
	unsigned = append(unsigned, chainBytes(payload)...)
	if *negative == "big-endian" {
		unsigned = binary.BigEndian.AppendUint64(unsigned, nonce)
	} else {
		unsigned = binary.LittleEndian.AppendUint64(unsigned, nonce)
	}
	unsigned = binary.LittleEndian.AppendUint64(unsigned, 0) // fee, fixed 0
	unsigned = append(unsigned, 0)                           // empty attributes, fixed explicitly
	signingHash := sha256.Sum256(unsigned)
	digest := sha256.Sum256(signingHash[:])
	sig := ed25519.Sign(ed25519.NewKeyFromSeed(seed), signingHash[:])
	check(ed25519.Verify(public, signingHash[:], sig), "standard Ed25519 verification")
	if *negative == "intent-hash" {
		digest = sha256.Sum256(meta)
	}
	if *negative == "single-hash" {
		digest = signingHash
	}
	// Independent protobuf transaction bytes, not the unsigned chain serializer.
	pl := append([]byte{0x08, 6}, field(0x12, payload)...)
	unsignedPB := field(0x0a, pl)
	if nonce != 0 {
		unsignedPB = append(unsignedPB, append([]byte{0x10}, varint(nonce)...)...)
	}
	code := append(append([]byte{32}, public...), 0xac)
	parameter := append([]byte{64}, sig...)
	program := append(field(0x0a, code), field(0x12, parameter)...)
	raw := append(field(0x0a, unsignedPB), field(0x12, program)...)
	check(len(raw) <= 768, "bounded signed transaction")

	upstream, err := transaction.NewSubscribeTransaction(public, "", in.Topic, in.SubscriptionBlocks, string(meta), nonce, 0)
	check(err == nil, "upstream constructor")
	// NewSubscribeTransaction generates 32 random attribute bytes. The candidate
	// contract intentionally selects empty attributes, so clear them BEFORE any
	// signing, hash, serialization or signature-verification cache is populated.
	check(len(upstream.UnsignedTx.Attributes) == 32, "fixed upstream attribute default changed")
	if *negative != "sdk-attributes" {
		upstream.UnsignedTx.Attributes = nil
	}
	var encoded bytes.Buffer
	check(upstream.SerializeUnsigned(&encoded) == nil, "upstream unsigned serialization")
	check(bytes.Equal(unsigned, encoded.Bytes()), "independent unsigned bytes differ from fixed upstream")
	check(wallet.SignTransaction(upstream) == nil, "fixed SDK signer")
	check(upstream.VerifySignature() == nil, "fixed upstream signature verification")
	check(bytes.Equal(sig, upstream.Programs[0].Parameter[1:]), "signature differs from fixed SDK")
	actual, err := upstream.Marshal()
	check(err == nil && bytes.Equal(raw, actual), "independent protobuf bytes differ from SDK")
	actualHash := upstream.Hash()
	check(bytes.Equal(digest[:], actualHash[:]), "candidate digest differs from actual chain transaction hash")
	check(actualHash.ToHexString() == hex.EncodeToString(digest[:]), "RPC digest hex byte order")

	// Always decode into fresh objects: NKN caches both hash and verification.
	for _, corrupt := range []struct {
		name string
		edit func([]byte)
	}{
		{"signature", func(b []byte) { b[len(b)-1] ^= 1 }},
		{"signed payload", func(b []byte) { at := bytes.Index(b, []byte(in.Topic)); check(at >= 0, "topic location"); b[at] ^= 1 }},
	} {
		bad := append([]byte(nil), raw...)
		corrupt.edit(bad)
		fresh := new(transaction.Transaction)
		check(fresh.Unmarshal(bad) == nil, "mutation must decode, not merely fail parsing")
		check(fresh.VerifySignature() != nil, "modified "+corrupt.name+" passed signature verification")
	}
	return vector{in, nonce, string(meta), hex.EncodeToString(unsigned), hex.EncodeToString(signingHash[:]), hex.EncodeToString(sig), hex.EncodeToString(raw), hex.EncodeToString(digest[:])}
}

func main() {
	flag.Parse()
	check(flag.NArg() == 0, "no positional arguments")
	check(*negative == "" || *negative == "intent-hash" || *negative == "single-hash" || *negative == "big-endian" || *negative == "sdk-attributes", "unknown mutation")
	defer func() {
		if reason := recover(); reason != nil {
			fmt.Fprintln(os.Stderr, "offline vector assertion:", reason)
			os.Exit(1)
		}
	}()
	deny := &denyNetwork{}
	http.DefaultTransport = deny
	http.DefaultClient = &http.Client{Transport: deny}
	// PUBLIC synthetic entropy 00..1f, domain-separated exactly as StatisticsIdentity.
	entropy := make([]byte, 32)
	for i := range entropy {
		entropy[i] = byte(i)
	}
	material := append([]byte("RustSimHub/usage-stats/ed25519-seed/v1\x00"), entropy...)
	seed := sha256.Sum256(material)
	account, err := nkn.NewAccount(seed[:])
	check(err == nil, "fixed SDK account")
	wallet, err := nkn.NewWallet(account, nil)
	check(err == nil, "fixed SDK wallet")
	public := ed25519.NewKeyFromSeed(seed[:]).Public().(ed25519.PublicKey)
	check(bytes.Equal(public, wallet.PubKey()), "independent public key")
	windows := []struct {
		label  string
		blocks uint32
	}{{"24h", 8640}, {"30d", 133920}, {"90d", 393120}}
	rows := make([]vector, 0, 15)
	for _, w := range windows {
		for _, nonce := range []uint64{0, 1, 128, 4294967296, 18446744073709551615} {
			in := intent{1, hex.EncodeToString(public), "rustsimhub-usage-v1-" + w.label, w.label, w.blocks, 20000}
			rows = append(rows, one(seed[:], public, wallet, in, nonce))
		}
	}
	check(deny.calls == 0, "reference construction attempted HTTP")
	result := struct {
		Format           int      `json:"format"`
		SyntheticEntropy string   `json:"synthetic_entropy_hex"`
		HTTPCalls        int      `json:"http_calls"`
		Vectors          []vector `json:"vectors"`
	}{1, hex.EncodeToString(entropy), deny.calls, rows}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	check(enc.Encode(result) == nil, "output encoding")
}
