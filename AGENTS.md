# AGENTS.md — Smart Home

## Project Overview

Async Rust backend that:
1. Polls Tuya API (with token auth) for sensor readings and persists them to PostgreSQL.
2. Runs a parallel background task that reads the latest sensor data, performs computations, and pushes control values back to Tuya.
3. Exposes a REST API (with OpenAPI spec) for future frontend consumption.

Frontend tech stack is TBD — `frontend/` is a placeholder.

---

## Repository Structure

```
smart_home/
├── backend/       # Rust async backend (Axum, sqlx, Tuya API)
├── frontend/      # Frontend app — tech stack TBD
├── docker-compose.yml
├── .env           # Local secrets (gitignored)
└── .envrc         # direnv: auto-loads .env into shell
```

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
| Config / env | `dotenvy` |
| Error handling | `thiserror` (library errors), `anyhow` (bin/main) |
| Logging / tracing | `tracing` + `tracing-subscriber` |
| Testing | built-in `#[tokio::test]` + `sqlx` test transactions |

---

## Build & Run Commands

All backend commands must be run from the `backend/` directory:

```bash
cd backend

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

# Database migrations (requires sqlx-cli)
sqlx migrate run
sqlx migrate revert
sqlx migrate add <migration_name>

# Prepare sqlx offline query cache (run before CI)
cargo sqlx prepare
```

Install sqlx-cli (once, without OpenSSL dependency):
```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
```

---

## Project Structure

```
backend/
  src/
    main.rs            # Entry point: builds runtime, wires up components
    config.rs          # Config struct loaded from env vars
    reading_cache.rs   # Shared in-memory cache: latest reading per (device_id, SensorType)
    db/
      mod.rs           # DB pool setup, migration runner
      models.rs        # DB row types (SensorReading, SensorType enum)
    tuya/
      mod.rs           # Tuya API client (token management, HTTP calls)
      models.rs        # Tuya API request/response types
    sensors/
      mod.rs           # Re-exports
      service.rs       # Domain service: fetch → transform → persist → cache
    control/
      mod.rs           # Re-exports
      service.rs       # Background task loop: reads cache, pushes control values
    api/
      mod.rs           # axum Router assembly, OpenAPI spec registration
      handlers.rs      # HTTP handler functions
      dto.rs           # Request/response DTOs (with utoipa schemas)
      errors.rs        # AppError type implementing IntoResponse
  migrations/
    20260226000000_init.sql

frontend/
  .gitkeep             # Placeholder — tech stack TBD
```

---

## Database Schema

```sql
CREATE TYPE sensor_type AS ENUM (
    'temperature', 'humidity', 'door_open',
    'power_consumption', 'relay_state', 'temperature_setpoint'
);

CREATE TABLE sensor_readings (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    device_id   TEXT        NOT NULL,
    sensor_type sensor_type NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    value       BIGINT      NOT NULL,
    CONSTRAINT uq_readings_device_type_time
        UNIQUE (device_id, sensor_type, recorded_at)
);
```

**Value encoding:**
- Numeric: `round(real_value * 100)` as `i64` (e.g. 21.45 °C → 2145)
- Boolean: `false` → 0, `true` → 1

**Index:** `(device_id, sensor_type, recorded_at DESC)` — covers scatter chart queries, time range filters, and latest-value lookups.

---

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/sensors/latest` | Latest reading per `(device_id, sensor_type)` |
| `GET` | `/sensors/{device_id}/{sensor_type}` | Time-series data; optional `?from=&to=` (RFC3339) |
| `GET` | `/sensors/{device_id}/{sensor_type}/latest` | Single latest value |
| `GET` | `/api-docs/openapi.json` | OpenAPI spec |

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
| `TUYA_DEVICE_IDS` | Comma-separated list of device IDs to poll |
| `POLL_INTERVAL_SECS` | Sensor polling interval in seconds (default `60`) |
| `CONTROL_INTERVAL_SECS` | Control loop interval in seconds (default `30`) |
| `RUST_LOG` | Tracing filter (e.g. `info,sqlx=warn`) |

Store secrets in `.env` at the repo root (gitignored). Never commit `.env`.

---

## Agent Rules

- **Never install any binaries.** Do not run `apt-get`, `snap`, `brew`, `cargo install`, `npm install -g`, or any other package/binary installer. If a required tool is missing, stop and ask the user to install it, or ask whether they want you to find a workaround instead.
- **Never use `unsafe` Rust.** Do not write `unsafe` blocks, functions, or traits under any circumstances. If a use case appears to require `unsafe`, stop and discuss the alternatives with the user.
