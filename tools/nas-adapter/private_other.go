//go:build !linux

// SPDX-License-Identifier: AGPL-3.0-only
package main

import "errors"

// Pure read/TLS tests may run elsewhere. Private credentials and the CLI remain
// Linux-only; do not substitute weaker mode or ownership checks on another OS.
func private(string, int64) ([]byte, error) {
	return nil, errors.New("linux_private_files_required")
}
