// SPDX-License-Identifier: AGPL-3.0-only
//go:build !linux

package control

func Load(string) (*Client, error) { return nil, invalid }
