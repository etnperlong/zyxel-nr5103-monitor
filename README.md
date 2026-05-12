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
recovery_method = "reload"

[monitor.reboot]
min_interval = 3600
wait_after = 60

[monitor.reload]
switch_wait = 15
restore_wait = 15
```

### Config fields

#### Top-level

- `log_level`: tracing filter such as `info` or `debug`

#### `[router]`

- `host`: router hostname or address, without `http://` or `https://`
- `protocol`: `http` or `https`, defaults to `http`
- `username`: router login username
- `password`: router login password

#### `[monitor]`

- `interval`: seconds between connectivity checks
- `url`: URL used for external connectivity checks
- `timeout`: request timeout in seconds
- `max_retries`: number of consecutive failures before recovery starts
- `recovery_method`: recovery flow to use:
  - `reload` (default): temporarily switch the preferred access technology, switch it back, then reboot if the connection is still down
  - `reboot`: skip the reload step and reboot immediately

#### `[monitor.reboot]`

- `min_interval`: minimum seconds between two reboot attempts
- `wait_after`: seconds to wait after issuing a reboot before connectivity checks resume

#### `[monitor.reload]`

- `switch_wait`: seconds to wait after switching the preferred access technology
- `restore_wait`: seconds to wait after switching the preferred access technology back

#### Defaults

- `monitor.interval = 60`
- `monitor.url = "http://www.gstatic.com/generate_204"`
- `monitor.timeout = 5`
- `monitor.max_retries = 1`
- `monitor.recovery_method = "reload"`
- `monitor.reboot.min_interval = 300`
- `monitor.reboot.wait_after = 60`
- `monitor.reload.switch_wait = 15`
- `monitor.reload.restore_wait = 15`

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
