// SPDX-License-Identifier: AGPL-3.0-only
//go:build linux

package control

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"syscall"
)

// Every path component is opened relative to its pinned parent, no symlinks.
// The containing directory and file must be private and owned by the caller.
func private(path string, max int64) ([]byte, error) {
	if !filepath.IsAbs(path) || filepath.Clean(path) != path {
		return nil, invalid
	}
	parts := strings.Split(strings.TrimPrefix(path, "/"), "/")
	fd, err := syscall.Open("/", syscall.O_RDONLY|syscall.O_DIRECTORY|syscall.O_CLOEXEC, 0)
	if err != nil {
		return nil, invalid
	}
	defer func() { syscall.Close(fd) }()
	for _, part := range parts[:len(parts)-1] {
		next, e := syscall.Openat(fd, part, syscall.O_RDONLY|syscall.O_DIRECTORY|syscall.O_NOFOLLOW|syscall.O_CLOEXEC, 0)
		if e != nil {
			return nil, invalid
		}
		syscall.Close(fd)
		fd = next
	}
	var parent syscall.Stat_t
	if syscall.Fstat(fd, &parent) != nil || parent.Mode&0077 != 0 || int(parent.Uid) != os.Geteuid() {
		return nil, invalid
	}
	child, e := syscall.Openat(fd, parts[len(parts)-1], syscall.O_RDONLY|syscall.O_NOFOLLOW|syscall.O_NONBLOCK|syscall.O_CLOEXEC, 0)
	if e != nil {
		return nil, invalid
	}
	file := os.NewFile(uintptr(child), path)
	defer file.Close()
	var before, after syscall.Stat_t
	if syscall.Fstat(child, &before) != nil || before.Mode&syscall.S_IFMT != syscall.S_IFREG || before.Mode&07077 != 0 || int(before.Uid) != os.Geteuid() || before.Nlink != 1 || before.Size > max {
		return nil, invalid
	}
	raw, e := io.ReadAll(io.LimitReader(file, max+1))
	if e != nil || int64(len(raw)) != before.Size || int64(len(raw)) > max || syscall.Fstat(child, &after) != nil || before.Dev != after.Dev || before.Ino != after.Ino || before.Size != after.Size || before.Mode != after.Mode || before.Uid != after.Uid || before.Nlink != after.Nlink || before.Mtim != after.Mtim || before.Ctim != after.Ctim {
		return nil, invalid
	}
	return raw, nil
}

func Load(path string) (*Client, error) {
	raw, err := private(path, 8192)
	if err != nil {
		return nil, invalid
	}
	var config struct {
		URL   string `json:"url"`
		CA    string `json:"ca_file"`
		Token string `json:"token_file"`
	}
	if StrictJSON(raw, &config) != nil {
		return nil, invalid
	}
	ca, err := private(config.CA, 65536)
	if err != nil {
		return nil, invalid
	}
	token, err := private(config.Token, 64)
	if err != nil {
		return nil, invalid
	}
	return New(config.URL, string(token), ca)
}
