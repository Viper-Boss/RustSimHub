// SPDX-License-Identifier: AGPL-3.0-only
// Node entrypoint; the browser suite additionally tests actual IndexedDB/ServiceWorker.
await import('../configurator/tests/controller.mjs');
await import('../configurator/tests/cancel.mjs');
