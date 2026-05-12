# zyxel-nr5103-monitor

Rust-based network watchdog for the Zyxel NR5103 5G CPE.

It can:

- log in to the router over HTTP or HTTPS
- handle the router's encrypted HTTP login flow
- periodically check external connectivity
- re-authenticate if the session expires
- reboot the router after repeated failures
- run as a systemd service

## Status

Implemented core features:

- router client
- crypto support for HTTP login
- config loading from TOML
- monitor loop and recovery flow
- musl cross-build support
- systemd deployment assets

## Requirements

- Rust toolchain
- `cargo`
- for musl builds on x86_64 in this repo: `clang`, `llvm-ar`, and the Rust musl target

## Configuration

The application loads config from the first available TOML source in this order:

1. `./config.toml`
2. `$HOME/.config/nr5103/config.toml` (loaded via config basename `.../config`)
3. `/etc/nr5103/config.toml` (loaded via config basename `/etc/nr5103/config`)

Example:

```toml
log_level = "info"

[router]
host = "172.16.0.1"
protocol = "http"
username = "monitor"
password = "Monitor5103"

[monitor]
interval = 15
url = "https://www.gstatic.com/generate_204"
timeout = 10
max_retries = 4
min_reboot_interval = 3600
recovery_method = "access_technology_switch_then_reboot"
access_technology_switch_wait = 15
access_technology_restore_wait = 15
```

### Config fields

- `log_level`: tracing filter, e.g. `info`, `debug`
- `router.host`: router hostname or address without protocol prefix
- `router.protocol`: optional router protocol, `http` or `https`; defaults to `http`
- `router.username`: router login username
- `router.password`: router login password
- `monitor.interval`: connectivity check interval in seconds
- `monitor.url`: URL used for external connectivity checks
- `monitor.timeout`: request timeout in seconds
- `monitor.max_retries`: consecutive failures before recovery triggers
- `monitor.min_reboot_interval`: minimum seconds between reboots
- `monitor.recovery_method`: recovery strategy, `access_technology_switch_then_reboot` (default) or `reboot`
- `monitor.access_technology_switch_wait`: seconds to wait after temporarily switching preferred access technology
- `monitor.access_technology_restore_wait`: seconds to wait after switching the preferred access technology back

## Build

Debug build:

```bash
cargo build
```

Release build:

```bash
cargo build --release
```

Run tests and lint:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## Run locally

```bash
cargo run --release
```

The application will:

1. load config
2. initialize logging
3. connect to the router
4. log in
5. fetch device information
6. start the monitor loop

Stop with `Ctrl+C`.

## Cross-compilation

Add targets:

```bash
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
```

Build for x86_64 musl:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

To make aarch64 the default target, uncomment the example in `.cargo/config.toml`.

## Deployment

Systemd assets are in `deploy/`:

- `deploy/zyxel-nr5103-monitor.service`
- `deploy/README.md`

Typical installation flow:

```bash
sudo useradd -r -s /usr/sbin/nologin monitor
sudo install -D -m 640 -o monitor config.toml /opt/zyxel-nr5103-monitor/config.toml
sudo install -D -m 755 target/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor
sudo install -m 644 deploy/zyxel-nr5103-monitor.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyxel-nr5103-monitor
```

For musl artifacts, replace the binary path with the target-specific output, for example:

```bash
target/x86_64-unknown-linux-musl/release/zyxel-nr5103-monitor
```

## Logs

```bash
journalctl -u zyxel-nr5103-monitor -f
```

## Notes

- HTTP mode uses the router's RSA/AES encrypted login flow.
- HTTPS mode skips that bootstrap and uses direct JSON requests.
- Self-signed router certificates are accepted intentionally for local-network usage.

## Chinese documentation

See [README_ZH.md](README_ZH.md).
