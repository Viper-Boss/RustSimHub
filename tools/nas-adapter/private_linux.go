// SPDX-License-Identifier: AGPL-3.0-only
package main

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"syscall"
)

func private(path string, maximum int64) ([]byte, error) {
	if !filepath.IsAbs(path) {
		return nil, errors.New("private_file_required")
	}
	resolved, e := filepath.EvalSymlinks(path)
	if e != nil || resolved != path {
		return nil, errors.New("private_file_required")
	}
	parent, e := os.Stat(filepath.Dir(path))
	if e != nil || !parent.IsDir() || parent.Mode().Perm()&0077 != 0 {
		return nil, errors.New("private_file_required")
	}
	ps, ok := parent.Sys().(*syscall.Stat_t)
	if !ok || int(ps.Uid) != os.Geteuid() {
		return nil, errors.New("private_file_required")
	}
	fd, e := syscall.Open(path, syscall.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK, 0)
	if e != nil {
		return nil, errors.New("private_file_required")
	}
	f := os.NewFile(uintptr(fd), path)
	defer f.Close()
	m, e := f.Stat()
	if e != nil || !m.Mode().IsRegular() || m.Mode().Perm()&0077 != 0 || m.Size() > maximum {
		return nil, errors.New("private_file_required")
	}
	st, ok := m.Sys().(*syscall.Stat_t)
	if !ok || int(st.Uid) != os.Geteuid() || st.Nlink != 1 {
		return nil, errors.New("private_file_required")
	}
	raw, err := io.ReadAll(io.LimitReader(f, maximum+1))
	if err != nil || int64(len(raw)) > maximum {
		return nil, errors.New("private_file_required")
	}
	return raw, nil
}
