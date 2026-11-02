// SPDX-License-Identifier: AGPL-3.0-only
package rootguard

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"syscall"
)

// Root, not a restarted Core or browser counter, assigns persistent SDK
// generations. Allocation only happens during an explicit authorized prepare.
// An interrupted pending write is preserved and requires reconciliation.
func nextGeneration() (uint32, error) {
	dir := filepath.Dir(seedPath)
	if rootDirectory(dir) != nil {
		return 0, ErrDenied
	}
	path := dir + "/generation"
	pending := dir + "/.generation.pending"
	if _, e := os.Lstat(pending); !os.IsNotExist(e) {
		return 0, ErrDenied
	}
	var previous uint32
	if _, e := os.Lstat(path); e == nil {
		raw, e := rootFile(path, 4, true)
		if e != nil || len(raw) != 4 {
			return 0, ErrDenied
		}
		previous = binary.BigEndian.Uint32(raw)
	} else if !os.IsNotExist(e) {
		return 0, ErrDenied
	}
	if previous == ^uint32(0) {
		return 0, ErrDenied
	}
	next := previous + 1
	fd, e := syscall.Open(pending, syscall.O_WRONLY|syscall.O_CREAT|syscall.O_EXCL|syscall.O_NOFOLLOW|syscall.O_CLOEXEC, 0600)
	if e != nil {
		return 0, ErrDenied
	}
	f := os.NewFile(uintptr(fd), "owned-root-generation")
	defer f.Close()
	var raw [4]byte
	binary.BigEndian.PutUint32(raw[:], next)
	if n, e := f.Write(raw[:]); e != nil || n != 4 {
		f.Close()
		return 0, ErrDenied
	}
	if f.Sync() != nil || f.Close() != nil {
		return 0, ErrDenied
	}
	if os.Rename(pending, path) != nil {
		return 0, ErrDenied
	}
	d, e := os.Open(dir)
	if e != nil {
		return 0, ErrDenied
	}
	defer d.Close()
	if d.Sync() != nil {
		return 0, ErrDenied
	}
	read, e := rootFile(path, 4, true)
	if e != nil || len(read) != 4 || binary.BigEndian.Uint32(read) != next {
		return 0, ErrDenied
	}
	return next, nil
}
