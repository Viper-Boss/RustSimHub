// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	control "../../control"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"time"
)

func run() error {
	if len(os.Args) == 1 {
		fmt.Println(`{"plan_only":true,"normal_client_required":true,"network_requests":0}`)
		return nil
	}
	if len(os.Args) != 3 || os.Args[1] != "--run" {
		return fmt.Errorf("usage: control [--run /private/config.json]")
	}
	raw, err := io.ReadAll(io.LimitReader(os.Stdin, control.MaxReply+1))
	if err != nil || len(raw) > control.MaxReply {
		return fmt.Errorf("invalid_input")
	}
	client, err := control.Load(os.Args[2])
	if err != nil {
		return err
	}
	defer client.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	result, err := control.Run(ctx, client, raw)
	if err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(result)
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err.Error())
		os.Exit(2)
	}
}
