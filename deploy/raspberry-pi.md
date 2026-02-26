# Raspberry Pi 3 Deployment Plan

## Architecture

- **Binary**: cross-compiled on dev machine, copied to Pi via `scp`
- **PostgreSQL**: remote instance (not on the Pi)
- **Process management**: `systemd` service (auto-start on boot, restart on crash)
- **No Docker required on the Pi**

---

## Step 0 — Confirm Pi OS bitness (run on the Pi)

```bash
uname -m
# armv7l  → 32-bit OS → use target: armv7-unknown-linux-gnueabihf
# aarch64 → 64-bit OS → use target: aarch64-unknown-linux-gnu
```

---

## Step 1 — Install `cross` on dev machine

```bash
cargo install cross --git https://github.com/cross-rs/cross
```

`cross` wraps `cargo` and handles the ARM toolchain inside a Docker container.
Docker must be running on the dev machine.

---

## Step 2 — Cross-compile the binary

```bash
# From backend/ on the dev machine:

# 32-bit Pi OS:
cross build --release --target armv7-unknown-linux-gnueabihf

# 64-bit Pi OS:
cross build --release --target aarch64-unknown-linux-gnu
```

Binary lands at:
```
backend/target/<target>/release/smart_home_service
```

### sqlx offline cache

`sqlx` compile-time query checks require either:
- A live PostgreSQL reachable from the dev machine at build time (`DATABASE_URL` set), **or**
- A pre-generated offline cache: run `cargo sqlx prepare` once (from `backend/`) and commit the `.sqlx/` directory.

---

## Step 3 — Copy binary to the Pi

```bash
scp backend/target/armv7-unknown-linux-gnueabihf/release/smart_home_service \
    pi@<PI_IP>:/home/pi/smart_home/
```

---

## Step 4 — Create `.env` on the Pi

Create `/home/pi/smart_home/.env` with real values:

```env
DATABASE_URL=postgres://user:password@<REMOTE_DB_HOST>/smart_home
TUYA_CLIENT_ID=...
TUYA_CLIENT_SECRET=...
TUYA_BASE_URL=https://openapi.tuyaus.com
TUYA_DEVICE_IDS=id1,id2
POLL_INTERVAL_SECS=60
CONTROL_INTERVAL_SECS=60
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
RUST_LOG=info,sqlx=warn
```

Never commit `.env`.

---

## Step 5 — Create the systemd unit on the Pi

See `deploy/smart-home.service` in this repo.

Copy it to the Pi:

```bash
scp deploy/smart-home.service pi@<PI_IP>:/tmp/
ssh pi@<PI_IP> sudo mv /tmp/smart-home.service /etc/systemd/system/smart-home.service
```

Then enable and start:

```bash
ssh pi@<PI_IP>
sudo systemctl daemon-reload
sudo systemctl enable smart-home
sudo systemctl start smart-home
```

Check logs:

```bash
sudo journalctl -u smart-home -f
```

---

## Step 6 — Database migrations

The backend runs `sqlx migrate` automatically on startup.
Ensure the remote PostgreSQL instance is reachable from the Pi before starting the service.

---

## Redeployment (after code changes)

```bash
# 1. Cross-compile
cross build --release --target armv7-unknown-linux-gnueabihf

# 2. Copy binary
scp backend/target/armv7-unknown-linux-gnueabihf/release/smart_home_service \
    pi@<PI_IP>:/home/pi/smart_home/

# 3. Restart service
ssh pi@<PI_IP> sudo systemctl restart smart-home
```

---

## Caveats

- **Tuya HMAC-SHA256 signing is not yet implemented** — the backend will start and serve the
  API, but won't fetch or send real data to Tuya until that is implemented.
- **`reqwest` uses `rustls`** (no OpenSSL) — cross-compiles cleanly, no OpenSSL headers needed.
- **Pi 3 RAM**: 1 GB is sufficient for the Rust binary + tokio runtime.
- **`TUYA_DEVICE_IDS`**: if left empty, the polling loop runs silently on an empty device list.
