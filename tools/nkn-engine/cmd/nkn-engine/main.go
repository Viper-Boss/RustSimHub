// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"context"
	"encoding/json"
	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
	"os"
	"os/signal"
	"runtime"
	"syscall"
	"time"
)

var sourceCommit = "development"

func run() int {
	if len(os.Args) == 2 && os.Args[1] == "--version" {
		_ = json.NewEncoder(os.Stdout).Encode(struct {
			Contract int    `json:"contract"`
			Source   string `json:"source_commit"`
			Go       string `json:"go"`
			SDK      bool   `json:"nkn_sdk_linked"`
		}{1, sourceCommit, runtime.Version(), false})
		return 0
	}
	if len(os.Args) != 2 || os.Args[1] != "--stdio" {
		_, _ = os.Stderr.WriteString("use --version or --stdio; NKN backend unavailable\n")
		return 2
	}
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	done := make(chan struct{})
	// This finite frontend owns only its stdio. A stalled peer cannot keep the
	// process beyond 30s; no engine/Root/identity exists to silently orphan.
	watchdog := time.AfterFunc(30*time.Second, func() { os.Exit(124) })
	defer func() { close(done); stop(); watchdog.Stop() }()
	go func() {
		select {
		case <-ctx.Done():
			_ = os.Stdin.Close()
			_ = os.Stdout.Close()
		case <-done:
		}
	}()
	beforeRead := func() error { return os.Stdin.SetReadDeadline(time.Now().Add(2 * time.Second)) }
	beforeWrite := func() error { return os.Stdout.SetWriteDeadline(time.Now().Add(2 * time.Second)) }
	err := bridge.Serve(ctx, os.Stdin, os.Stdout, beforeRead, beforeWrite)
	if ctx.Err() != nil {
		return 130
	}
	if err != nil {
		_, _ = os.Stderr.WriteString("bounded local protocol failed; NKN backend unavailable\n")
		return 2
	}
	return 0
}
func main() { os.Exit(run()) }
