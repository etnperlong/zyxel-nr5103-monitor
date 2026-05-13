# AGENTS.md

## Repository shape
- Single-crate Rust project (`edition = 2024`). Runtime entrypoint is `src/main.rs`; `src/lib.rs` re-exports modules so integration tests can use the same code.
- Main boundaries: `src/client/` (router auth, crypto, DAL, cellular/device endpoints), `src/monitor/` (internet + signal watchdogs, auth refresh, reload/reboot recovery), `src/telemetry/` (OTel runtime, metric recording, privacy-preserving Zyxel DAL parsing).
- `src/main.rs` still declares local `mod client; mod config; mod monitor; mod telemetry;`, so unit tests inside those modules run once for the library and once for the binary under plain `cargo test`.

## Code structure

```
src/
├── main.rs              # tokio entrypoint, wiring: config -> client -> monitor -> telemetry
├── lib.rs               # re-exports client/config/monitor/telemetry for integration tests
├── config.rs            # TOML config types + load_config() (serde, config crate)
├── client/
│   ├── mod.rs           # ZyxelClient: HTTP/RSA crypto, session key, request dispatch
│   ├── auth.rs          # login/logout, session lifecycle
│   ├── crypto.rs        # RSA public key fetch, AES encrypt/decrypt for HTTP payloads
│   ├── dal.rs           # typed DAL get/set (GET /cgi-bin/DAL, PUT /cgi-bin/DAL)
│   ├── cellular.rs      # cellwan_status / cellwan_band typed accessors + set_cellwan_band
│   └── device.rs        # get_basic_information (DeviceInfo model/uptime/firmware)
├── monitor/
│   ├── mod.rs           # Monitor::run() select loop, recovery dispatch (reload vs reboot)
│   ├── internet.rs      # InternetMonitor: HTTP connectivity check (QualityMonitor impl)
│   ├── signal.rs        # SignalMonitor: 5G signal quality check (QualityMonitor impl)
│   └── auth.rs          # ensure_authenticated() session refresh helper
└── telemetry/
    ├── mod.rs           # re-exports metrics/otel/zyxel
    ├── otel.rs          # TelemetryRuntime init/shutdown, subscriber install, OTLP exporter build
    ├── metrics.rs       # TelemetryCollector (DAL fetch + record), MetricRecorder (OTel gauges/counters)
    └── zyxel.rs         # DAL JSON -> Rust struct parsing, privacy-preserving normalization

tests/
├── client_core.rs           # RSA+AES crypto roundtrip, login/dal request encoding
├── config_loading.rs        # config default/override/inheritance (mutates CWD + HOME)
├── device_client.rs         # device info endpoint parsing
├── monitor.rs               # monitor loop lifecycle, SIGINT shutdown, recovery flows
├── telemetry_metrics.rs     # MetricRecorder delta/baseline counter logic
├── telemetry_otel_runtime.rs # TelemetryRuntime init/shutdown edge cases
└── telemetry_zyxel_parsing.rs # DAL JSON -> struct deserialization tests
```

## Verification
- Preferred full check order: `cargo fmt --check` -> `cargo clippy -- -D warnings` -> `cargo test`.
- Focused integration tests: `cargo test --test <file_stem> <test_name>`.
- Run `tests/monitor.rs` serially when targeting that file: `cargo test --test monitor -- --test-threads=1`; several tests send `SIGINT` to the process.
- `tests/config_loading.rs` mutates current directory and `HOME`-dependent config lookup behind an in-test lock; avoid adding parallel env/current-dir assumptions there.

## Build / deploy quirks
- x86_64 musl builds rely on `.cargo/config.toml`: `CC_x86_64_unknown_linux_musl=clang`, `AR_x86_64_unknown_linux_musl=llvm-ar`, linker `rust-lld`.
- Cross-build command used by this repo: `cargo build --release --target x86_64-unknown-linux-musl`.
- `deploy/zyxel-nr5103-monitor.service` sets `WorkingDirectory=/opt/zyxel-nr5103-monitor`; deployed services normally load `/opt/zyxel-nr5103-monitor/config.toml` via the first config source.

## Config source of truth
- Trust `src/config.rs` over README/sample config for accepted fields and defaults.
- Config sources are layered in this order, later sources overriding earlier ones because `config::ConfigBuilder` adds all three: `./config.toml`, `$HOME/.config/nr5103/config.toml`, `/etc/nr5103/config.toml`.
- Actual defaults: `router.protocol=http`, `monitor.interval=60s`, `monitor.max_retries=1`, `monitor.recovery_method=reload`, `monitor.internet.url=http://www.gstatic.com/generate_204`, `monitor.internet.timeout=5s`, child `interval`/`max_retries` inherit monitor defaults, `monitor.signal.enabled=false`, `monitor.signal.require_5g=false`, `monitor.signal.min_5g_rsrp=-110`, `action.reboot.min_interval=300s`, `action.reboot.wait_after=60s`, `action.reload.switch_wait=15s`, `action.reload.restore_wait=15s`.
- `monitor.recovery_method` accepts `reload`, `reboot`, and legacy alias `access_technology_switch_then_reboot`.
- Telemetry defaults to disabled for every signal. Current keys are `[telemetry] service_name, endpoint, authorization, protocol, export_interval` and `[telemetry.metrics|traces|logs].enabled`; `protocol` is `grpc` or `http/protobuf` (`http-protobuf` alias accepted).

## Router / monitor behavior that is easy to break
- `router.protocol` only supports `http` and `https`. HTTP mode fetches `/getRSAPublickKey` and uses encrypted request/response handling; HTTPS mode skips RSA bootstrap and still accepts invalid router certs.
- Authenticated requests append `sessionkey` with `?` or `&` depending on the existing query string; DAL paths depend on that behavior.
- Allowlisted DAL OIDs live in `src/client/dal.rs`: `status`, `cellwan_status`, `cellwan_band`, and case-sensitive `Traffic_Status`.
- Reload recovery toggles preferred access technology between `NR5G-SA` and `Auto`, restores the original value, then falls back to reboot if the monitored condition remains unhealthy.
- Metrics collection is attached only when `cfg.telemetry.metrics.enabled`; telemetry collection failures in the monitor loop are logged and intentionally do not trigger recovery.

## Telemetry constraints
- `src/telemetry/zyxel.rs` intentionally normalizes DAL data without sensitive identifiers such as IMEI, IMSI, IPs, MACs, or session-key-style fields; preserve that privacy boundary.
- Traffic counters are emitted as deltas after an initial baseline sample; counter resets must not produce negative deltas.
- OTLP runtime supports metrics, traces, and logs exporters, but application trace/log instrumentation is still limited compared with metrics.

## Local files and secrets
- `config.toml` is local/ignored and may contain router credentials or OTLP auth tokens; do not commit or quote real values from it.
- Use `config.example.toml` and README snippets for documentation examples, then reconcile them against `src/config.rs` before changing docs.
