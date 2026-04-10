# Kuiper-rs — Copilot Instructions

## Architecture Overview

The workspace consists of three distinct layers that must NOT be mixed:

| Layer | Crate(s) | Interface |
|---|---|---|
| **HTTP API** | `resource-server` | REST/WebSocket (`Invoke-RestMethod`, `curl`, etc.) |
| **Coordinator** | `coordinator` | Internal background service; subscribes to WS events from resource-server and performs reconciliation |
| **Standalone CLI** | `kr` | Reads/writes the `FileSystemStore` **directly** at `KUIPER_STORE_PATH` — completely bypasses the HTTP API |

## `kr` is NOT an API client

`kr` (the binary built from the `kr` crate) operates **directly on the file store** and has **nothing to do with the HTTP API**. It is a standalone maintenance/development tool.

**Rules:**
- **Never call `kr` from API validation or integration test scripts.** Any script that validates the HTTP API must use only HTTP calls (`Invoke-RestMethod`, `curl`, or equivalent). `kr` must not be invoked, even indirectly.
- The only permitted use of `kr` in scripts is **bootstrapping** (`kr define`) to write a `ResourceDefinition` into the store *before* the resource-server is started — i.e., in pre-server setup steps that are explicitly guarded so they do not run when the server is already live (e.g. gated behind `-SkipDefine`).
- Reconciliation (hard-delete of soft-deleted records) is the **coordinator's** responsibility. Do not invoke `kr reconcile` as a proxy for coordinating — start the coordinator service instead.

## Soft-Delete Model

- `DELETE` on a resource sets `deletionTimestamp` (i64 microseconds) — this is a **soft-delete**.
- Soft-deleted records are **visible** in `GET` and `LIST` responses.
- `deletionTimestamp` and `uid` are **immutable** once set; subsequent `PUT`/SET requests must preserve them.
- **Hard-delete** (removing the file from the store) is performed by the coordinator during its reconciliation pass, not by the API.

## Opt-in Reconciliation

`KuiperRuntimeBuilder::with_reconciliation()` registers `ReconcileCommand` on the `"reconcile"` command name. It is **not** part of the default builder and must **never** be called in `resource-server`. It is called in:
- `coordinator/src/main.rs` — coordinator runtime
- `kr/src/main.rs` — CLI runtime

The resource-server must never expose a reconcile endpoint.
