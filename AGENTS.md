# AGENTS.md

## Repository shape
- Single-crate Rust project (`edition = 2024`). Real runtime entrypoint is `src/main.rs`; `src/lib.rs` re-exports the same modules for integration tests.
- Main boundaries: `src/client/` (router auth, crypto, DAL, device endpoints), `src/monitor.rs` (watchdog loop / reboot flow), `src/telemetry/` (safe DAL parsing, OpenTelemetry runtime, metrics).

## Verification that matches this repo
- Preferred Rust verification order: `cargo clippy -- -D warnings` then `cargo test`.
- `cargo test` runs the `src/client/*` unit tests twice: once under `src/lib.rs` and once under `src/main.rs`, because the binary still declares `mod client; mod config; mod monitor; mod telemetry;` locally.
- Focused integration test pattern: `cargo test --test <file_stem> <test_name>`.
- For `tests/monitor.rs`, use `cargo test --test monitor -- --test-threads=1` when running that file directly; multiple tests send `SIGINT` (`kill -INT`) to the test process.

## Build / deploy quirks
- x86_64 musl builds rely on `.cargo/config.toml`: `CC_x86_64_unknown_linux_musl=clang`, `AR_x86_64_unknown_linux_musl=llvm-ar`, linker `rust-lld`.
- Cross-build command in this repo: `cargo build --release --target x86_64-unknown-linux-musl`.
- `deploy/zyxel-nr5103-monitor.service` sets `WorkingDirectory=/opt/zyxel-nr5103-monitor`, so deployed services normally load `/opt/zyxel-nr5103-monitor/config.toml` via the first config search path.

## Config source-of-truth notes
- Hard-coded config search order in `src/config.rs`: `./config.toml` -> `$HOME/.config/nr5103/config.toml` -> `/etc/nr5103/config.toml`.
- `src/config.rs` is the source of truth for omitted-field defaults, not `README.md` or the sample `config.toml`.
- Actual monitor defaults are: `interval=60s`, `url=http://www.gstatic.com/generate_204`, `timeout=5s`, `max_retries=1`, `min_reboot_interval=300s`.
- Telemetry config already exists in code even though docs/sample config lag it:
  - `[telemetry] service_name, endpoint, export_interval`
  - `[telemetry.metrics].enabled`
  - `[telemetry.traces].enabled`
  - `[telemetry.logs].enabled`
  - all telemetry signals default to disabled

## Router / telemetry behavior that is easy to break
- `router.protocol` only supports `http` and `https`. HTTP mode fetches the RSA key and uses the encrypted login flow; HTTPS mode skips that bootstrap.
- Allowlisted DAL access lives in `src/client/dal.rs`; current OIDs are `status`, `cellwan_status`, `cellwan_band`, and `Traffic_Status` (case-sensitive on the last one).
- `main.rs` only attaches `TelemetryCollector` when `cfg.telemetry.metrics.enabled` is true. Metrics failures are intentionally isolated from connectivity/reboot behavior in `src/monitor.rs`.
- `src/telemetry/zyxel.rs` is intentionally privacy-preserving: normalized telemetry drops sensitive identifiers such as IMEI, IMSI, IPs, MACs, and session-key-style fields. Preserve that boundary.

## Current in-progress feature context
- Ongoing feature tracking lives in `.hive/features/02_opentelemetry-telemetry-monitoring/`.
- Completed there: DAL support, safe telemetry parsers, OTel runtime init/shutdown, and metric collection (tasks 02-05).
- Pending there: trace/log instrumentation, tests-after cleanup, and docs/sample config updates (tasks 06-08). If you change telemetry behavior or docs, reconcile with both `src/config.rs` and that `.hive` plan.
