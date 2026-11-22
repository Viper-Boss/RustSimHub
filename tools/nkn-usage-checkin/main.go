// SPDX-License-Identifier: AGPL-3.0-only

// Report one installation using an independent, zero-fee NKN wallet.
// The renewal and count model follows NKNGuard pkg/usagestats (AGPL-3.0).
package main

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"time"

	nkn "github.com/nknorg/nkn-sdk-go"
	"github.com/nknorg/nkn/v2/transaction"
)

var windows = [...]struct {
	topic  string
	blocks uint32
}{
	{"rustsimhub-usage-v1-24h", 8640},
	{"rustsimhub-usage-v1-30d", 133920},
	{"rustsimhub-usage-v1-90d", 393120},
}

const blocksPerDay = 4320

type pending struct {
	Window int    `json:"window"`
	Nonce  uint64 `json:"nonce"`
}

type reportState struct {
	Pending   *pending  `json:"pending,omitempty"`
	CheckedAt time.Time `json:"checked_at,omitempty"`
}

type nonceStatus struct {
	Confirmed *uint64 `json:"nonce"`
	Queued    *uint64 `json:"nonceInTxPool"`
	Height    uint32  `json:"currentHeight"`
}

type chainReader interface {
	call(context.Context, string, map[string]string, any) error
}

func main() {
	dir := os.Getenv("STATE_DIRECTORY")
	if len(os.Args) != 1 || !filepath.IsAbs(dir) {
		log.Fatal("statistics require a systemd state directory")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 35*time.Second)
	defer cancel()
	if err := checkIn(ctx, newChain(), dir, time.Now()); err != nil {
		log.Fatal(err)
	}
}

func checkIn(ctx context.Context, chain chainReader, dir string, now time.Time) error {
	statePath := filepath.Join(dir, "state.json")
	var state reportState
	if raw, err := readFile(statePath, 512); err == nil {
		if json.Unmarshal(raw, &state) != nil || state.Pending != nil &&
			(state.Pending.Window < 0 || state.Pending.Window >= len(windows)) {
			return errors.New("invalid statistics renewal state")
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	if state.Pending == nil && !now.Before(state.CheckedAt) && now.Sub(state.CheckedAt) < time.Hour {
		return nil
	}

	seedPath := filepath.Join(dir, "seed")
	seed, err := readFile(seedPath, 32)
	if errors.Is(err, os.ErrNotExist) {
		if state.Pending != nil || !state.CheckedAt.IsZero() {
			return errors.New("statistics identity is missing")
		}
		seed = make([]byte, 32)
		if _, err = rand.Read(seed); err != nil {
			return err
		}
		if err = saveFile(seedPath, seed); err != nil {
			return err
		}
	}
	if err != nil || len(seed) != 32 {
		return errors.New("invalid statistics identity")
	}
	defer clear(seed)

	account, err := nkn.NewAccount(seed)
	if err != nil {
		return errors.New("unable to load statistics identity")
	}
	wallet, err := nkn.NewWallet(account, nil)
	if err != nil {
		return errors.New("unable to initialize statistics signer")
	}
	var nonce nonceStatus
	if err := chain.call(ctx, "getnoncebyaddr", map[string]string{"address": account.WalletAddress()}, &nonce); err != nil {
		return err
	}
	if nonce.Confirmed == nil || nonce.Queued == nil || nonce.Height == 0 || *nonce.Queued < *nonce.Confirmed {
		return errors.New("invalid statistics nonce response")
	}
	if state.Pending != nil && *nonce.Confirmed < state.Pending.Nonce {
		return errors.New("statistics node is behind the saved nonce")
	}
	if *nonce.Queued != *nonce.Confirmed {
		return nil
	}

	if state.Pending != nil && *nonce.Confirmed > state.Pending.Nonce {
		state.Pending = nil
	}

	if state.Pending == nil {
		subscriber := hex.EncodeToString(account.PubKey())
		for i, window := range windows {
			var result struct {
				ExpiresAt *uint32 `json:"expiresAt"`
			}
			if err := chain.call(ctx, "getsubscription", map[string]string{
				"topic": window.topic, "subscriber": subscriber,
			}, &result); err != nil {
				return err
			}
			if result.ExpiresAt == nil {
				return errors.New("invalid statistics subscription response")
			}
			if uint64(*result.ExpiresAt) < uint64(nonce.Height)+uint64(window.blocks-blocksPerDay) {
				state.Pending = &pending{Window: i, Nonce: *nonce.Confirmed}
				break
			}
		}
		if state.Pending == nil {
			state.CheckedAt = now
			return saveState(statePath, state)
		}
		if err := saveState(statePath, state); err != nil {
			return err
		}
	}

	window := windows[state.Pending.Window]
	raw, hash, err := sign(wallet, account.PubKey(), *state.Pending)
	if err != nil {
		return err
	}
	var accepted string
	if err := chain.call(ctx, "sendrawtransaction", map[string]string{"tx": hex.EncodeToString(raw)}, &accepted); err != nil {
		return err
	}
	if accepted != hash {
		return errors.New("statistics transaction hash was not acknowledged")
	}
	log.Printf("statistics subscription submitted for %s", window.topic)
	return nil
}

func sign(wallet *nkn.Wallet, key []byte, p pending) ([]byte, string, error) {
	window := windows[p.Window]
	tx, err := transaction.NewSubscribeTransaction(key, "", window.topic, window.blocks, "", p.Nonce, 0)
	if err != nil {
		return nil, "", errors.New("unable to prepare statistics subscription")
	}
	// A retry of the same reservation must reuse the exact transaction.
	tx.UnsignedTx.Attributes = nil
	if err := wallet.SignTransaction(tx); err != nil {
		return nil, "", errors.New("unable to sign statistics subscription")
	}
	raw, err := tx.Marshal()
	if err != nil {
		return nil, "", errors.New("unable to encode statistics subscription")
	}
	hash := tx.Hash()
	return raw, hex.EncodeToString(hash.ToArray()), nil
}

func saveState(path string, state reportState) error {
	body, err := json.Marshal(state)
	if err != nil {
		return err
	}
	return saveFile(path, body)
}

func readFile(path string, limit int64) ([]byte, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	data, err := io.ReadAll(io.LimitReader(file, limit+1))
	if err != nil || int64(len(data)) > limit {
		return nil, fmt.Errorf("invalid statistics %s file", filepath.Base(path))
	}
	return data, nil
}

func saveFile(path string, data []byte) error {
	dir := filepath.Dir(path)
	file, err := os.CreateTemp(dir, ".statistics-*")
	if err != nil {
		return err
	}
	defer os.Remove(file.Name())
	defer file.Close()
	if _, err := file.Write(data); err != nil {
		return err
	}
	if err := file.Sync(); err != nil {
		return err
	}
	if err := file.Close(); err != nil {
		return err
	}
	if err := os.Rename(file.Name(), path); err != nil {
		return err
	}
	parent, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer parent.Close()
	return parent.Sync()
}
