#!/bin/sh
# Isolated HTTPS read-only validation. Root offline owner key never enters the edge UID.
set -eu
umask 077
core=/opt/rustsimhub/dev-edge-p1/rustsimhub-core
ctl=/opt/rustsimhub/dev-edge-p1/simhubctl
edge=/opt/rustsimhub/dev-edge-p1/rustsimhub-edge
source_hash=$(cat /opt/rustsimhub/dev-edge-p1/source.sha256)
for binary in "$core" "$ctl" "$edge";do "$binary" --build-info | grep -q "\"source_archive_sha256\":\"$source_hash\"";done
test -f /opt/rustsimhub/.development-users-installed
command -v openssl >/dev/null
command -v timeout >/dev/null
command -v bash >/dev/null
owner_base=$(mktemp -d /var/tmp/rustsimhub-edge-owner-XXXXXXXX)
core_base=$(mktemp -d /var/tmp/rustsimhub-edge-core-XXXXXXXX)
edge_base=$(mktemp -d /var/tmp/rustsimhub-edge-client-XXXXXXXX)
runtime=$(mktemp -d /run/rustsimhub-edge-control-XXXXXXXX)
printf '%s\n%s\n%s\n%s\n' "$owner_base" "$core_base" "$edge_base" "$runtime" > /var/tmp/rustsimhub-edge-smoke-evidence
"$core" --inbox-prepare-fixture "$owner_base"
cp -a "$owner_base/state" "$owner_base/identity" "$core_base/"
chown -R rustsimhub-core:rustsimhub-core "$core_base"
chown rustsimhub-core:rustsimhub-ipc "$runtime";chmod 2750 "$runtime"
chown rustsimhub-edge:rustsimhub-edge "$edge_base"
runuser -u rustsimhub-edge -- "$edge" --init-tls "$edge_base/tls" 10.55.0.2 > "$owner_base/tls-created.json"
origin=https://10.55.0.2:8443
printf '{"listen":"10.55.0.2:8443","origin":"%s","core_socket":"%s/core.sock","core_uid":%s,"tls_directory":"%s/tls"}\n' "$origin" "$runtime" "$(id -u rustsimhub-core)" "$edge_base" > "$edge_base/config.json"
chown rustsimhub-edge:rustsimhub-edge "$edge_base/config.json";chmod 0600 "$edge_base/config.json"
core_unit=rustsimhub-edge-core-p1
edge_unit=rustsimhub-edge-https-p1
cleanup(){ systemctl stop "$edge_unit" "$core_unit" >/dev/null 2>&1 || true; }
trap cleanup EXIT HUP INT TERM
for unit in "$core_unit" "$edge_unit";do test "$(systemctl is-active "$unit" 2>/dev/null || true)" != active;done
systemctl reset-failed "$core_unit" "$edge_unit" >/dev/null 2>&1 || true
systemd-run --unit "$core_unit" -p User=rustsimhub-core -p Group=rustsimhub-core -p SupplementaryGroups=rustsimhub-ipc \
 -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateTmp=yes -p PrivateDevices=yes \
 -p "BindPaths=$core_base $runtime" -p "ReadWritePaths=$core_base $runtime" \
 -p RuntimeMaxSec=180 -p TimeoutStopSec=20 -p MemoryMax=64M -p TasksMax=16 -p LimitCORE=0 \
 "$core" --serve-call-observing "$runtime/core.sock" "$(id -u rustsimhub-edge)" "$core_base/state" "$core_base/identity"
count=0;while ! test -S "$runtime/core.sock";do count=$((count+1));test "$count" -lt 15;sleep 1;done
systemd-run --unit "$edge_unit" -p User=rustsimhub-edge -p Group=rustsimhub-edge -p SupplementaryGroups=rustsimhub-ipc \
 -p NoNewPrivileges=yes -p ProtectSystem=strict -p ProtectHome=yes -p PrivateTmp=yes -p PrivateDevices=yes \
 -p "BindReadOnlyPaths=$edge_base $runtime" -p "InaccessiblePaths=/run/dbus -$core_base -$owner_base" \
 -p RuntimeMaxSec=160 -p TimeoutStopSec=5 -p MemoryMax=48M -p TasksMax=8 -p LimitCORE=0 \
 "$edge" --serve "$edge_base/config.json"
cert=$edge_base/tls/cert.pem
http(){
 method=$1;path=$2;origin_header=$3;body_file=$4
 body_length=0
 if test "$body_file" != none;then body_length=$(wc -c < "$body_file");fi
 { printf '%s %s HTTP/1.1\r\nHost: 10.55.0.2:8443\r\nConnection: close\r\n' "$method" "$path"
   if test "$method" = POST;then printf 'Origin: %s\r\nContent-Type: application/json\r\nX-RustSimHub-Request: 2\r\nContent-Length: %s\r\n' "$origin_header" "$body_length";fi
   printf '\r\n'
   if test "$body_file" != none;then cat "$body_file";fi
 } > "$owner_base/request.http"
 timeout 15 openssl s_client -connect 10.55.0.2:8443 -CAfile "$cert" -verify_ip 10.55.0.2 -verify_return_error -quiet -ign_eof -ignore_unexpected_eof < "$owner_base/request.http" > "$owner_base/response.http" 2> "$owner_base/tls-client.log"
 sed -n '1,/^\r$/p' "$owner_base/response.http" > "$owner_base/response-headers.http"
 code=$(sed -n '1s/^HTTP\/1\.1 \([0-9][0-9][0-9]\).*/\1/p' "$owner_base/response-headers.http")
 length=$(awk 'tolower($1)=="content-length:" {gsub("\r","",$2);print $2}' "$owner_base/response-headers.http")
 case "$length" in ''|*[!0-9]*) return 1;; esac
 test "$length" -le 65536
 header_size=$(wc -c < "$owner_base/response-headers.http")
 total_size=$(wc -c < "$owner_base/response.http")
 test "$total_size" -eq "$((header_size+length))"
 tail -c "$length" "$owner_base/response.http" > "$owner_base/http-body"
}
count=0;while ! http GET / "$origin" none;do count=$((count+1));test "$count" -lt 10;sleep 1;done
test "$code" = 200
cp "$owner_base/http-body" "$owner_base/page.html";grep -q RustSimHub "$owner_base/page.html"
http GET /app.js "$origin" none;test "$code" = 200
cp "$owner_base/http-body" "$owner_base/app.js"
cp "$owner_base/response-headers.http" "$owner_base/headers.txt"
grep -qi 'content-security-policy:' "$owner_base/headers.txt"
grep -qi 'cache-control: no-store' "$owner_base/headers.txt"
http GET /proof.js "$origin" none;test "$code" = 200;cp "$owner_base/http-body" "$owner_base/proof.js"
if timeout 3 openssl s_client -connect 10.55.0.2:8443 -no-CAfile -no-CApath -no-CAstore -verify_return_error -quiet < "$owner_base/request.http" > "$owner_base/untrusted-client.log" 2>&1;then echo 'Unexpected untrusted certificate accepted' >&2;exit 1;fi
timeout 3 bash -c 'exec 3<>/dev/tcp/10.55.0.2/8443; printf "GET / HTTP/1.1\r\nHost: 10.55.0.2:8443\r\n\r\n" >&3; head -c 16 <&3' > "$owner_base/plaintext.log" 2>&1 || true
if grep -q 'HTTP/' "$owner_base/plaintext.log";then echo 'Unexpected plaintext HTTP accepted' >&2;exit 1;fi
seq=0
post(){
 seq=$((seq+1))
 printf '{"protocol":2,"id":"smoke-%s","command":%s}\n' "$seq" "$1" > "$owner_base/request.json"
 http POST /api/v2 "$origin" "$owner_base/request.json";test "$code" = 200
 cp "$owner_base/http-body" "$owner_base/response.json"
 grep -q "\"id\":\"smoke-$seq\"" "$owner_base/response.json"
 sed -n 's/^.*"result":\(.*\)}$/\1/p' "$owner_base/response.json" > "$owner_base/result.json"
 test -s "$owner_base/result.json"
}
post '{"op":"identity"}'
pin=$(cat "$owner_base/device.pubhex")
login(){
 "$ctl" keygen "$owner_base/$1" > "$owner_base/client-public.json"
 public=$(sed -n 's/.*"public_key":\(\[[^]]*\]\).*/\1/p' "$owner_base/client-public.json")
 test -n "$public"
 post "{\"op\":\"challenge\",\"public_key\":$public}"
 challenge=$(sed -n 's/.*"id":"\([a-f0-9]*\)".*/\1/p' "$owner_base/result.json")
 test "${#challenge}" = 64
 "$ctl" sign "$owner_base/$1" "$pin" login < "$owner_base/result.json" > "$owner_base/signature.json"
 signature=$(cat "$owner_base/signature.json")
 post "{\"op\":\"authenticate\",\"challenge_id\":\"$challenge\",\"signature\":$signature}"
 token=$(sed -n 's/.*"token":"\([a-f0-9]*\)".*/\1/p' "$owner_base/result.json")
 test "${#token}" = 64
}
login owner-key
owner_token=$token
post "{\"op\":\"health\",\"token\":\"$owner_token\"}";grep -q '"ok":true' "$owner_base/result.json"
count=0;while :;do
 post "{\"op\":\"line_status\",\"token\":\"$owner_token\"}"
 if grep -q '"status":"ready"' "$owner_base/result.json";then break;fi
 count=$((count+1));test "$count" -lt 20;sleep 1
done
cp "$owner_base/result.json" "$owner_base/line.json"
post "{\"op\":\"network_status\",\"token\":\"$owner_token\"}"
grep -q '"hardware_mutations":false' "$owner_base/result.json"
grep -q '"kind":"wifi"' "$owner_base/result.json"
grep -q '"kind":"ethernet"' "$owner_base/result.json"
cp "$owner_base/result.json" "$owner_base/network.json"
post "{\"op\":\"inbox\",\"token\":\"$owner_token\",\"after\":null,\"limit\":16}";grep -q '"messages":' "$owner_base/result.json"
cp "$owner_base/result.json" "$owner_base/inbox.json"
post "{\"op\":\"call_status\",\"token\":\"$owner_token\"}";grep -q '"active_calls":0' "$owner_base/result.json";grep -q '"control_api":false' "$owner_base/result.json"
post "{\"op\":\"prepare_sms\",\"token\":\"$owner_token\",\"line_epoch\":2,\"idempotency_key\":\"read-mode-denied\",\"recipient\":\"+12025550123\",\"text\":\"synthetic\"}";grep -q '"error":' "$owner_base/result.json"
login status-key
post "{\"op\":\"network_status\",\"token\":\"$token\"}";grep -q '"hardware_mutations":false' "$owner_base/result.json"
post "{\"op\":\"inbox\",\"token\":\"$token\",\"after\":null,\"limit\":16}";grep -q '"error":' "$owner_base/result.json"
public=$(sed -n 's/.*"public_key":\(\[[^]]*\]\).*/\1/p' "$owner_base/client-public.json")
post "{\"op\":\"revoke\",\"token\":\"$owner_token\",\"public_key\":$public}";grep -q '"ok":true' "$owner_base/result.json"
post "{\"op\":\"network_status\",\"token\":\"$token\"}";grep -q '"error":' "$owner_base/result.json"
http POST /api/v2 https://foreign.invalid "$owner_base/request.json"
test "$code" = 403
runuser -u rustsimhub-edge -- test ! -r "$core_base/state/store.sqlite3"
runuser -u rustsimhub-edge -- test ! -r "$core_base/identity/identity.seed"
runuser -u rustsimhub-edge -- test ! -r "$owner_base/owner-key/client.seed"
runuser -u rustsimhub-edge -- test ! -r /dev/snd/pcmC0D0p
if printf '{"protocol":2,"id":"wrong-uid","command":{"op":"identity"}}' | "$core" --request "$runtime/core.sock" > "$owner_base/wrong-uid.log" 2>&1;then echo 'Unexpected wrong control UID accepted' >&2;exit 1;fi
for unit in "$core_unit" "$edge_unit";do
 pid=$(systemctl show "$unit" -p MainPID --value)
 test "$pid" -gt 1
 awk '/VmRSS|Threads/' "/proc/$pid/status" > "$owner_base/$unit-resources.txt"
done
cleanup
test ! -S "$runtime/core.sock"
runuser -u rustsimhub-core -- "$core" --inbox-verify-fixture "$core_base"
for path in /proc/asound/card0/pcm*/sub*/status;do test "$(cat "$path")" = closed;done
systemctl is-active nassimhub-agent ModemManager NetworkManager rmtfs
echo 'Distinct-UID HTTPS, trusted TLS, core auth, scope, revocation, network read and shutdown passed; no hardware writes'
