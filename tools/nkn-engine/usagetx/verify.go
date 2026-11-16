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

// VerifyCanonicalSubmission lets Root reject arbitrary payloads before handing
// them to the pinned SDK host. The host must ALSO compare the decoded nonce to
// its original preflight ticket; this function does not allocate a nonce.
func VerifyCanonicalSubmission(r Request) ([]byte, error) {
	if r.Validate(Submit) != nil {
		return nil, ErrInvalid
	}
	raw, _ := hex.DecodeString(r.Transaction)
	if len(raw) < 3 || raw[0] != 0x0a {
		return nil, ErrInvalid
	}
	size, used := binary.Uvarint(raw[1:])
	if used <= 0 || size > uint64(len(raw)-1-used) {
		return nil, ErrInvalid
	}
	unsigned := raw[1+used : 1+used+int(size)]
	if len(unsigned) < 2 || unsigned[0] != 0x0a {
		return nil, ErrInvalid
	}
	payload, n := binary.Uvarint(unsigned[1:])
	if n <= 0 || payload > uint64(len(unsigned)-1-n) {
		return nil, ErrInvalid
	}
	tail := unsigned[1+n+int(payload):]
	var nonce uint64
	if len(tail) != 0 {
		if tail[0] != 0x10 {
			return nil, ErrInvalid
		}
		nonce, n = binary.Uvarint(tail[1:])
		if n <= 0 || n+1 != len(tail) {
			return nil, ErrInvalid
		}
	}
	return VerifySubmission(r, nonce)
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
