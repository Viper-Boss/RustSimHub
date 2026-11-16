// SPDX-License-Identifier: AGPL-3.0-only
package usagetx

import (
	"bytes"
	"crypto/ed25519"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
)

func field(tag byte, data []byte) []byte {
	return append(binary.AppendUvarint([]byte{tag}, uint64(len(data))), data...)
}

// VerifySubmission accepts only the canonical zero-fee/empty-attributes fixed
// subscription signed by this public key, at the preflight's original nonce.
// It neither generates a signature nor accepts an arbitrary transaction type.
func VerifySubmission(r Request, nonce uint64) ([]byte, error) {
	if r.Validate(Submit) != nil {
		return nil, ErrInvalid
	}
	raw, _ := hex.DecodeString(r.Transaction)
	if len(raw) < ed25519.SignatureSize {
		return nil, ErrInvalid
	}
	public, _ := hex.DecodeString(r.PublicKey)
	meta, _ := json.Marshal(r.Intent)
	payload := field(0x0a, public)
	payload = append(payload, field(0x1a, []byte(r.Intent.Topic))...)
	payload = append(payload, binary.AppendUvarint([]byte{0x28}, uint64(r.Intent.SubscriptionBlocks))...)
	payload = append(payload, field(0x32, meta)...)
	unsigned := binary.LittleEndian.AppendUint32(nil, 6)
	if len(payload) < 253 {
		unsigned = append(unsigned, byte(len(payload)))
	} else {
		unsigned = binary.LittleEndian.AppendUint16(append(unsigned, 253), uint16(len(payload)))
	}
	unsigned = append(unsigned, payload...)
	unsigned = binary.LittleEndian.AppendUint64(unsigned, nonce)
	unsigned = binary.LittleEndian.AppendUint64(unsigned, 0)
	unsigned = append(unsigned, 0)
	hash := sha256.Sum256(unsigned)
	digest := sha256.Sum256(hash[:])
	if r.Digest != hex.EncodeToString(digest[:]) {
		return nil, ErrInvalid
	}
	sig := raw[len(raw)-ed25519.SignatureSize:]
	if !ed25519.Verify(public, hash[:], sig) {
		return nil, ErrInvalid
	}
	pb := field(0x0a, append([]byte{0x08, 6}, field(0x12, payload)...))
	if nonce != 0 {
		pb = append(pb, binary.AppendUvarint([]byte{0x10}, nonce)...)
	}
	code := append(append([]byte{32}, public...), 0xac)
	program := append(field(0x0a, code), field(0x12, append([]byte{64}, sig...))...)
	expected := append(field(0x0a, pb), field(0x12, program)...)
	if !bytes.Equal(raw, expected) {
		return nil, ErrInvalid
	}
	return raw, nil
}
