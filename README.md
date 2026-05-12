# zyxel-nr5103-monitor

Rust-based network watchdog for the Zyxel NR5103 5G CPE.

It can:

- log in to the router over HTTP or HTTPS
- handle the router's encrypted HTTP login flow
- periodically check external connectivity
- optionally monitor degraded 5G signal quality or fallback to 4G
- re-authenticate if the session expires
- recover connectivity via access-technology reload before rebooting
- reboot the router after repeated failures
- export metrics via OpenTelemetry (OTLP)
- run as a systemd service

## Status

Implemented core features:

- router client
- crypto support for HTTP login
- config loading from TOML
- monitor loop and recovery flow
- OpenTelemetry metrics export
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

[monitor.signal]
enabled = true
require_5g = true
min_5g_rsrp = -110
max_retries = 2

[telemetry]
service_name = "zyxel-nr5103-monitor"
endpoint = "http://localhost:4317"
export_interval = 60

[telemetry.metrics]
enabled = false
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
  - `reload` (default): temporarily switch the preferred access technology, switch it back, then reboot if the monitored condition is still unhealthy
  - `reboot`: skip the reload step and reboot immediately

#### `[monitor.reboot]`

- `min_interval`: minimum seconds between two reboot attempts
- `wait_after`: seconds to wait after issuing a reboot before connectivity checks resume

#### `[monitor.reload]`

- `switch_wait`: seconds to wait after switching the preferred access technology
- `restore_wait`: seconds to wait after switching the preferred access technology back

#### `[monitor.signal]`

- `enabled`: enable signal-quality monitoring, defaults to `false`
- `require_5g`: treat fallback to non-5G access technology as degraded, defaults to `false`
- `min_5g_rsrp`: minimum acceptable 5G RSRP in dBm before recovery starts, defaults to `-110`
- `max_retries`: number of consecutive degraded signal checks before recovery starts, defaults to `1`

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
- `monitor.signal.enabled = false`
- `monitor.signal.require_5g = false`
- `monitor.signal.min_5g_rsrp = -110`
- `monitor.signal.max_retries = 1`

#### `[telemetry]`

- `service_name`: OpenTelemetry resource `service.name` attribute
- `endpoint`: OTLP gRPC endpoint URL (e.g. `http://localhost:4317`)
- `export_interval`: seconds between metric exports

#### `[telemetry.metrics]`

- `enabled`: `true` or `false`, defaults to `false`

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

## OpenTelemetry

The monitor can export metrics via the OpenTelemetry Protocol (OTLP). All telemetry signals are **disabled by default**.

### Configuration

```toml
[telemetry]
service_name = "zyxel-nr5103-monitor"  # OTel resource service.name
endpoint = "http://localhost:4317"     # OTLP gRPC endpoint
export_interval = 60                   # seconds between metric exports

[telemetry.metrics]
enabled = true

[telemetry.traces]
enabled = false   # not yet implemented

[telemetry.logs]
enabled = false   # not yet implemented
```

#### Defaults

- `telemetry.service_name = "zyxel-nr5103-monitor"`
- `telemetry.export_interval = 60`
- `telemetry.metrics.enabled = false`
- `telemetry.traces.enabled = false`
- `telemetry.logs.enabled = false`

### Exported metrics

#### Device and system

| Name                             | Type  | Unit | Attributes          | Description                              |
| -------------------------------- | ----- | ---- | ------------------- | ---------------------------------------- |
| `zyxel.device.uptime.seconds`      | Gauge | `s`    | —                   | Device uptime in seconds                 |
| `zyxel.system.cpu.usage.percent`   | Gauge | `%`    | —                   | CPU usage percentage                     |
| `zyxel.system.memory.bytes`        | Gauge | `By`   | `state` = `total`/`free` | Total and free memory                    |

#### Cellular signal

| Name                    | Type  | Unit | Attributes              | Description                                            |
| ----------------------- | ----- | ---- | ----------------------- | ------------------------------------------------------ |
| `zyxel.cellular.signal.dbm` | Gauge | `dBm`  | `radio`, `kind`           | Signal strength in dBm (RSSI, RSRP)                   |
| `zyxel.cellular.signal.db`  | Gauge | `dB`   | `radio`, `kind`           | Signal strength in dB (RSRQ, SINR)                     |

`radio` values: `lte`, `nr_nsa`, `scc`
`kind` values: `rssi`, `rsrp`, `rsrq`, `sinr`

#### Network interfaces

| Name                            | Type    | Unit     | Attributes                                 | Description                              |
| ------------------------------- | ------- | -------- | ------------------------------------------ | ---------------------------------------- |
| `zyxel.interface.traffic.bytes`   | Counter | `By`       | `interface_type`, `interface_name`, `direction` | Traffic bytes (delta per export)         |
| `zyxel.interface.traffic.packets` | Counter | `{packet}` | `interface_type`, `interface_name`, `direction` | Traffic packets (delta per export)       |
| `zyxel.interface.errors`          | Counter | `{error}`  | `interface_type`, `interface_name`, `direction` | Interface errors (delta per export)      |
| `zyxel.interface.discards`        | Counter | `{packet}` | `interface_type`, `interface_name`, `direction` | Interface discards (delta per export)    |

`interface_type` values: `ip`, `ethernet`
`direction` values: `sent`, `received`

#### LAN ports

| Name           | Type  | Unit | Attributes | Description                              |
| -------------- | ----- | ---- | ---------- | ---------------------------------------- |
| `zyxel.lan.port.up` | Gauge | —    | `port_name`  | `1` = link up, `0` = link down           |

#### Connectivity monitoring

| Name                                  | Type      | Unit | Description                              |
| ------------------------------------- | --------- | ---- | ---------------------------------------- |
| `zyxel.monitor.connectivity.rtt.ms`     | Histogram | `ms`   | Round-trip latency of connectivity checks |
| `zyxel.monitor.connectivity.failures`   | Counter   | —    | Connectivity check failure count         |

#### Signal-quality monitoring

| Name                                          | Type    | Unit | Attributes | Description                                      |
| --------------------------------------------- | ------- | ---- | ---------- | ------------------------------------------------ |
| `zyxel.monitor.signal.degraded`                 | Counter | —    | `reason`   | Degraded signal-quality checks                   |
| `zyxel.monitor.signal.recovery.attempts`        | Counter | —    | —          | Recovery attempts triggered by degraded signal   |
| `zyxel.monitor.signal.recovery.successes`       | Counter | —    | —          | Successful recoveries triggered by signal checks |
| `zyxel.monitor.signal.recovery.failures`        | Counter | —    | —          | Failed recoveries triggered by signal checks     |

`reason` values: `missing_5g`, `weak_5g_rsrp`

#### Recovery: reboot

| Name                                | Type    | Unit | Description                              |
| ----------------------------------- | ------- | ---- | ---------------------------------------- |
| `zyxel.monitor.reboot.attempts`       | Counter | —    | Reboot recovery attempts                 |
| `zyxel.monitor.reboot.successes`      | Counter | —    | Successful reboot commands               |

#### Recovery: reload

| Name                                    | Type      | Unit | Description                              |
| --------------------------------------- | --------- | ---- | ---------------------------------------- |
| `zyxel.monitor.reload.attempts`           | Counter   | —    | Reload recovery attempts                 |
| `zyxel.monitor.reload.successes`          | Counter   | —    | Reload recoveries that restored the monitored condition |
| `zyxel.monitor.reload.failures`           | Counter   | —    | Reload recoveries that failed to restore the monitored condition |
| `zyxel.monitor.reload.duration.seconds`   | Histogram | `s`    | Total duration of reload recovery cycles |

### Privacy

The telemetry module intentionally strips sensitive identifiers (IMEI, IMSI, IP addresses, MAC addresses, session keys) before exporting any metric data.

## Chinese documentation

See [README_ZH.md](README_ZH.md).
