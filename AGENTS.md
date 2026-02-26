# AGENTS.md — Smart Home Backend AI

## Project Overview

Async Rust backend that:
1. Polls Tuya API (with token auth) for sensor readings and persists them to PostgreSQL.
2. Runs a parallel background task that reads the latest sensor data, performs computations, and pushes control values back to Tuya.
3. Exposes a REST API (with OpenAPI spec) for future frontend consumption.

---

## Recommended Stack

| Concern | Crate |
|---|---|
| Async runtime | `tokio` (multi-thread) |
| HTTP server | `axum` |
| OpenAPI generation | `utoipa` + `utoipa-axum` |
| Database | `sqlx` (async, compile-time checked queries, PostgreSQL) |
| Migrations | `sqlx-cli` (`sqlx migrate`) |
| HTTP client (Tuya) | `reqwest` |
| Serialization | `serde` + `serde_json` |
| Config / env | `config` or `dotenvy` |
| Error handling | `thiserror` (library errors), `anyhow` (bin/main) |
| Logging / tracing | `tracing` + `tracing-subscriber` |
| Testing | built-in `#[tokio::test]` + `sqlx` test transactions |

---

## Build & Run Commands

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run the server
cargo run

# Check without producing a binary (fast feedback)
cargo check

# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings

# Run all tests
cargo test

# Run a single test by name (substring match)
cargo test <test_name>

# Run tests in a specific module/file
cargo test sensor::tests

# Run tests with stdout visible
cargo test -- --nocapture

# Database migrations (requires sqlx-cli: `cargo install sqlx-cli`)
sqlx migrate run
sqlx migrate revert
sqlx migrate add <migration_name>

# Prepare sqlx offline query cache (run before CI)
cargo sqlx prepare
```

---

## Project Structure (Recommended)

```
src/
  main.rs            # Entry point: builds runtime, wires up components
  config.rs          # Config struct loaded from env / config file
  db/
    mod.rs           # DB pool setup, migration runner
    models.rs        # DB row types (plain structs, not domain types)
  tuya/
    mod.rs           # Tuya API client (token management, HTTP calls)
    models.rs        # Tuya API request/response types
  sensors/
    mod.rs           # Sensor reading ingestion logic
    service.rs       # Domain service: fetch → transform → persist
  control/
    mod.rs           # Computation + control value push-back logic
    service.rs       # Background task loop
  api/
    mod.rs           # axum Router assembly, OpenAPI spec registration
    handlers.rs      # HTTP handler functions
    dto.rs           # Request/response DTOs (with utoipa schemas)
    errors.rs        # AppError type implementing IntoResponse
migrations/
  YYYYMMDDHHMMSS_init.sql
```

---

## Code Style

### Formatting & Linting
- Run `cargo fmt` before every commit. CI must pass `cargo fmt -- --check`.
- Run `cargo clippy -- -D warnings`. Zero warnings policy.
- Line length: default `rustfmt` settings (100 chars soft limit).

### Naming Conventions
- Types / Traits / Enums: `UpperCamelCase` (`SensorReading`, `TuyaClient`)
- Functions / methods / variables / modules: `snake_case`
- Constants / statics: `SCREAMING_SNAKE_CASE`
- Database columns and migration files: `snake_case`
- Environment variables: `SCREAMING_SNAKE_CASE` (e.g. `DATABASE_URL`, `TUYA_CLIENT_ID`)

### Imports
- Group imports in this order (enforced by `rustfmt`):
  1. `std` / `core`
  2. External crates
  3. Internal (`crate::`)
- Prefer explicit imports over glob imports (`use crate::sensors::SensorReading` not `use crate::sensors::*`).
- Exception: `use serde::{Deserialize, Serialize}` globs are acceptable.

### Types & Data Modeling
- Use `sqlx::FromRow` on DB model structs; keep them separate from API DTOs.
- Use `serde::{Deserialize, Serialize}` on DTOs and Tuya API types.
- Prefer `Uuid` (from `uuid` crate) for primary keys over `i64`.
- Use `chrono::DateTime<Utc>` for all timestamps; store as `TIMESTAMPTZ` in Postgres.
- Wrap domain primitives in newtypes when type confusion would be dangerous
  (e.g., `struct DeviceId(String)` vs raw `String`).
- Use `Option<T>` for nullable fields; never use sentinel values.

### Error Handling
- Define a per-module `Error` enum with `#[derive(thiserror::Error, Debug)]`.
- In `main.rs` / top-level async tasks use `anyhow::Result` for convenience.
- HTTP layer: define `AppError(anyhow::Error)` implementing `axum::response::IntoResponse`
  returning appropriate HTTP status codes and JSON error bodies.
- Never use `.unwrap()` or `.expect()` in production paths; reserve them for tests and
  initialization panics where failure is unrecoverable by design.
- Use `?` for propagation; add context with `.context("...")` (anyhow) or map errors explicitly.

### Async & Concurrency
- All I/O must be async (`tokio`). Never block the async executor (no `std::thread::sleep`,
  no synchronous file I/O in async context).
- Use `tokio::spawn` for the background control-loop task; store the `JoinHandle` and await
  it on graceful shutdown.
- Shared state across tasks: prefer `Arc<Mutex<T>>` for simple cases; use message passing
  (`tokio::sync::mpsc`) for producer/consumer patterns.
- DB pool (`sqlx::PgPool`) is `Clone + Send + Sync`; pass it via `axum::extract::State`.

### Logging
- Use `tracing::info!`, `tracing::warn!`, `tracing::error!` macros throughout.
- Add structured fields: `tracing::info!(device_id = %id, "Reading fetched")`.
- Never use `println!` for runtime logging; `eprintln!` is acceptable only for startup errors
  before the tracing subscriber is initialized.

### Testing
- Unit tests live in a `#[cfg(test)] mod tests { ... }` block at the bottom of the file
  they test.
- Integration tests live in `tests/`.
- Async tests use `#[tokio::test]`.
- DB tests: use `sqlx::test` attribute which provides an isolated transaction rolled back after
  each test — no manual cleanup needed.
- Mock Tuya HTTP responses with `wiremock` or `httpmock`.
- Do not commit tests that require real Tuya credentials or a live Postgres instance without
  an explicit feature flag.

### OpenAPI
- Annotate all handler functions and DTOs with `utoipa` macros (`#[utoipa::path]`,
  `#[derive(ToSchema)]`).
- The generated `openapi.json` should be served at `GET /api-docs/openapi.json`.
- Keep the spec up to date; breaking changes to the API require a version bump.

---

## Environment Variables

| Variable | Description |
|---|---|
| `DATABASE_URL` | Postgres connection string (`postgres://user:pass@host/db`) |
| `TUYA_CLIENT_ID` | Tuya Open API client ID |
| `TUYA_CLIENT_SECRET` | Tuya Open API client secret |
| `TUYA_BASE_URL` | Tuya API base URL (region-specific) |
| `SERVER_HOST` | Bind address (default `0.0.0.0`) |
| `SERVER_PORT` | Bind port (default `8080`) |
| `RUST_LOG` | Tracing filter (e.g. `info,sqlx=warn`) |

Store secrets in `.env` (gitignored). Never commit `.env`.

---

## Agent Rules

- **Never install any binaries.** Do not run `apt-get`, `snap`, `brew`, `cargo install`, `npm install -g`, or any other package/binary installer. If a required tool is missing, stop and ask the user to install it, or ask whether they want you to find a workaround instead.