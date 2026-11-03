// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"context"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/Viper-Boss/RustSimHub/tools/nkn-engine/rootguard"
)

func run() int {
	if len(os.Args) != 2 || os.Args[1] != "--serve-fixed" {
		return 2
	}
	policy, e := rootguard.LoadInstalled()
	if e != nil {
		return 2
	}
	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer cancel()
	timer := time.AfterFunc(330*time.Second, cancel)
	defer timer.Stop()
	if rootguard.Serve(ctx, policy) != nil {
		return 2
	}
	return 0
}
func main() { os.Exit(run()) }
