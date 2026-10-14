# NAS bridge client

Linux Go client for the explicitly enabled RustSimHub bridge. This is an adapter client, not the full old NAS application or a protocol 1.5 payload implementation.

Configuration (private directory 700, JSON file 600):

```json
{"url":"https://LOCAL_DEVICE_ADDRESS:PORT","ca_file":"/private/cert.pem","device_public_key":"DEVICE_PUBLIC_KEY_HEX","core_id":"OWNER_APPROVED_ALIAS","binding_epoch":"OWNER_APPROVED_EPOCH_HEX","key_file":"/private/nas-seed"}
```

The seed is the separately approved NAS key, 32 raw bytes; never the device or owner key. It must be in a private directory with mode 600 and belong to the running user. TLS verifies the configured CA and hostname, with no proxy, redirects or automatic request retries.

Build with CGO_ENABLED=0. For tests, set RSH_GOLDEN_FILE to the committed compat legacy-golden.json; these vectors came from the original Go proto implementation and use synthetic keys/data only.

```text
rustsimhub-nas-adapter session /private/config.json
rustsimhub-nas-adapter read /private/config.json
```

Read accepts JSON on stdin: `{"token":"PRIVATE_SESSION","query":{"entity":"status","after":null,"limit":1,"record_id":null}}`. Capture output privately; authorized SMS replies can contain communication data. Reads use adapter_api=1/data_format=2. Writes, legacy PCM, PQ and old updates are not implemented. See docs/nas-online-bridge.md for device-bound proof bytes and owner mapping.
