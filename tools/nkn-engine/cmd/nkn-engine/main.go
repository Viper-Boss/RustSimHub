// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"context"
	"encoding/json"
	"os"
	"os/signal"
	"runtime"
	"syscall"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/bridge"
)

var sourceCommit = "development"

// Inherited os.Stdin/os.Stdout are not necessarily registered with Go's
// poller. Duplicate ONLY these owned pipe endpoints and register nonblocking
// files; otherwise SetReadDeadline can fail before any real frame is served.
func pipe(file *os.File) (*os.File, error) {
	info, err := file.Stat()
	if err != nil {
		return nil, err
	}
	if info.Mode()&os.ModeNamedPipe == 0 {
		return nil, bridge.ErrFrame
	}
	fd, err := syscall.Dup(int(file.Fd()))
	if err != nil {
		return nil, err
	}
	syscall.CloseOnExec(fd)
	if err = syscall.SetNonblock(fd, true); err != nil {
		_ = syscall.Close(fd)
		return nil, err
	}
	owned := os.NewFile(uintptr(fd), "owned-local-stdio-pipe")
	if owned == nil {
		_ = syscall.Close(fd)
		return nil, bridge.ErrFrame
	}
	return owned, nil
}

func run() int {
	// Includes version/help output if the parent stops reading its own pipe.
	watchdog := time.AfterFunc(30*time.Second, func() { os.Exit(124) })
	defer watchdog.Stop()
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
	input, err := pipe(os.Stdin)
	if err != nil {
		return 2
	}
	defer input.Close()
	output, err := pipe(os.Stdout)
	if err != nil {
		return 2
	}
	defer output.Close()
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	done := make(chan struct{})
	// This finite frontend owns only its stdio. A stalled peer cannot keep the
	// process beyond 30s; no engine/Root/identity exists to silently orphan.
	defer func() { close(done); stop() }()
	go func() {
		select {
		case <-ctx.Done():
			_ = input.Close()
			_ = output.Close()
		case <-done:
		}
	}()
	beforeRead := func() error { return input.SetReadDeadline(time.Now().Add(2 * time.Second)) }
	beforeWrite := func() error { return output.SetWriteDeadline(time.Now().Add(2 * time.Second)) }
	err = bridge.Serve(ctx, input, output, beforeRead, beforeWrite)
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
