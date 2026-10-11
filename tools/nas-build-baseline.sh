#!/bin/sh
set -u
printf '\nDOCKER\n'; docker ps --format '{{.Names}} {{.Image}}'
printf '\nAVAILABLE IMAGES\n'; docker images --format '{{.Repository}}:{{.Tag}}'
printf '\nBUILD SPACE\n'; df -h /var/tmp
printf '\nDEVICE USB REACHABILITY\n'; timeout 3 sh -c 'nc -z 10.55.0.2 22' 2>/dev/null || true
