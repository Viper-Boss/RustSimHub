#!/bin/sh
set -eu
docker run --rm --user 1000:1000 rust@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa cargo clippy --version
