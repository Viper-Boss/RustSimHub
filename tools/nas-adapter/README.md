# NAS bridge client

Linux Go client for the explicitly enabled RustSimHub bridge. This is an adapter client, not the full old NAS application or a protocol 1.5 payload implementation.

Configuration (private directory 700, JSON file 600):

```json
{"url":"https://LOCAL_DEVICE_ADDRESS:PORT","ca_file":"/private/cert.pem","device_public_key":"DEVICE_PUBLIC_KEY_HEX","core_id":"OWNER_APPROVED_ALIAS","binding_epoch":"OWNER_APPROVED_EPOCH_HEX","key_file":"/private/nas-seed"}
```

The seed is the separately approved NAS key, 32 raw bytes; never the device or owner key. It must be in a private directory with mode 600 and belong to the running user. TLS verifies the configured CA and hostname, with no proxy, redirects or automatic request retries.

Build with CGO_ENABLED=0. For tests, set RSH_GOLDEN_FILE to the committed compat legacy-golden.json; these vectors came from the original Go proto implementation and use synthetic keys/data only.

The bridge now has multiple Go source files; build the directory rather than
passing only `main.go`. No Go module or third-party Go dependency is added.
From the repository root on Linux:

```sh
export GO111MODULE=off
export RSH_GOLDEN_FILE="$PWD/crates/compat/src/legacy-golden.json"
go test -count=1 -v ./tools/nas-adapter
go vet ./tools/nas-adapter
CGO_ENABLED=0 go build -o /tmp/rustsimhub-nas-adapter ./tools/nas-adapter
```

Pure-model, signing-vector and loopback TLS tests also run on other platforms;
private credentials and the CLI remain Linux-only and fail closed elsewhere.
The Linux CLI test uses synthetic private configuration and a loopback fixture,
not an actual NAS or device. Cross-compilation does not execute that test.
On Linux, `cargo test -p rustsimhub-compat --locked` also runs these Go fixtures
and requires Go on PATH. That test disables module downloads, uses a 60-second
fixture deadline, and checks that the real Linux CLI fixture actually ran.

```text
rustsimhub-nas-adapter session /private/config.json
rustsimhub-nas-adapter read /private/config.json
```

Read accepts JSON on stdin: `{"token":"PRIVATE_SESSION","query":{"entity":"status","after":null,"limit":1,"record_id":null}}`. Capture output privately; authorized SMS replies can contain communication data. Reads use adapter_api=1/data_format=2. Writes, legacy PCM, PQ and old updates are not implemented. See docs/nas-online-bridge.md for device-bound proof bytes and owner mapping.

Reads accept only `status`, `sms`, `sms_detail` and `calls`, validate their typed
responses and preserve the existing envelope. Unknown versions, fields and
invalid/oversized replies are rejected without publishing partial output.
Failures distinguish transport, HTTP authorization, disabled compatibility,
unimplemented operations, unavailable backends and invalid contracts; they do not
print remote error text. The client stores no cursor or business records and
does not retry. Consumers commit a page/cursor atomically and deduplicate by
stable IDs. See [support matrix and field mapping](../../docs/nas-adapter/README.md)
for the exact missing-value, pagination, binary-data and integration boundaries.
