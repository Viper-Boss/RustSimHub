//go:build nkn_sdk_candidate

// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"encoding/json"
	"io"
	"log"
	"os"
	"os/signal"
	"runtime"
	"syscall"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/managed"
)

var sourceCommit = "development"

func pipe(file *os.File) (*os.File, error) {
	info, e := file.Stat()
	if e != nil || info.Mode()&os.ModeNamedPipe == 0 {
		return nil, managed.ErrLaunch
	}
	fd, e := syscall.Dup(int(file.Fd()))
	if e != nil {
		return nil, e
	}
	syscall.CloseOnExec(fd)
	if e = syscall.SetNonblock(fd, true); e != nil {
		syscall.Close(fd)
		return nil, e
	}
	return os.NewFile(uintptr(fd), "owned-managed-pipe"), nil
}
func run() int {
	// The SDK is process-owned. Upstream diagnostics must never copy raw RPC,
	// ciphertext, business bytes, endpoints or private launch data into logs.
	log.SetOutput(io.Discard)
	watchdog := time.AfterFunc(315*time.Second, func() { os.Exit(124) })
	defer watchdog.Stop()
	if len(os.Args) == 2 && os.Args[1] == "--version" {
		json.NewEncoder(os.Stdout).Encode(map[string]any{"contract": 1, "source_commit": sourceCommit, "go": runtime.Version(), "nkn_sdk_linked": true, "root_launch_required": true, "isolated_fixture_build": managed.FixtureBuild()})
		return 0
	}
	if len(os.Args) != 2 || os.Args[1] != "--root-stdio" {
		return 2
	}
	inheritedCredential := os.NewFile(3, "root-credential")
	if inheritedCredential == nil {
		return 2
	}
	credential, e := pipe(inheritedCredential)
	inheritedCredential.Close()
	if e != nil {
		return 2
	}
	defer credential.Close()
	control := os.NewFile(4, "root-control")
	if control == nil {
		return 2
	}
	defer control.Close()
	lease, e := managed.LoadRootLease(credential, control)
	if e != nil {
		return 2
	}
	defer lease.Close()
	// Do not leave inherited duplicates alive after their owned copies exist.
	control.Close()
	input, e := pipe(os.Stdin)
	if e != nil {
		return 2
	}
	defer input.Close()
	output, e := pipe(os.Stdout)
	if e != nil {
		return 2
	}
	defer output.Close()
	os.Stdin.Close()
	os.Stdout.Close()
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, syscall.SIGTERM, syscall.SIGINT)
	defer signal.Stop(signals)
	go func() {
		select {
		case <-signals:
			lease.Close()
		case <-lease.Context().Done():
		}
		time.AfterFunc(5*time.Second, func() { os.Exit(124) })
	}()
	if managed.Run(lease, input, output) != nil {
		return 2
	}
	return 0
}
func main() { os.Exit(run()) }
