#!/bin/sh
set -eu
root=$(docker info --format '{{.DockerRootDir}}')
printf 'DOCKER ROOT\n%s\n' "$root"
df -h "$root"
printf '\nDATA VOLUMES\n'; df -h /vol1 /vol2 /volume1 2>/dev/null || true
