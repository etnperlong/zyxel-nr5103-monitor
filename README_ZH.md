<div align="center">
  <img src="assets/Banner.png" alt="Zyxel NR5103 Monitor" width="100%">
</div>

# zyxel-nr5103-monitor

用 Rust 写的 Zyxel NR5103 5G CPE 网络守护程序。它会监控外网连通性和 5G 信号质量，出问题时自动恢复连接。

[English](README.md) | [中文](README_ZH.md)

## 功能概述

程序通过 HTTP 或 HTTPS 登录 Zyxel NR5103 路由器，定时做两项检查：能不能访问外网，5G 信号是否正常。如果某项检查连续失败，会尝试恢复——先重载接入技术设置，不行就重启路由器。会话过期时自动重新认证。

以 systemd 服务方式运行，开机自启，崩溃自动重启。

## 特性

- 支持 HTTP 和 HTTPS 登录，HTTP 模式使用 RSA/AES 加密
- 定期 HTTP 探测外网连通性
- 可选的 5G 信号质量监控，支持配置 RSRP 门限
- 会话过期自动重新认证
- 两级恢复策略：接入技术重载 + 重启兜底
- OpenTelemetry 指标导出（OTLP gRPC 和 HTTP/protobuf）
- TOML 配置，支持多路径叠加加载
- 支持 x86_64 和 aarch64 的静态 musl 构建
- systemd 服务部署

## 快速开始

### 环境要求

- Rust 工具链（通过 [rustup](https://rustup.rs/) 安装）
- 构建 musl 交叉编译版本需要：`clang`、`llvm-ar` 和对应的 Rust musl target

### 构建

```bash
cargo build --release
```

### 配置

复制示例配置文件，改成你路由器的信息：

```bash
cp config.example.toml config.toml
```

至少要填路由器地址和登录凭据：

```toml
[router]
host = "172.16.0.1"
username = "admin"
password = "your-password"
```

### 运行

```bash
cargo run --release
```

程序会登录路由器、读取设备信息、启动监控循环。按 `Ctrl+C` 退出。

## 配置说明

程序按以下顺序加载第一个可用的 TOML 配置：

1. `./config.toml`
2. `$HOME/.config/nr5103/config.toml`
3. `/etc/nr5103/config.toml`

后加载的配置会覆盖前面的。所有选项见 [`config.example.toml`](config.example.toml)，里面有详细注释。

### 主要配置项

| 区块 | 字段 | 默认值 | 说明 |
|------|------|--------|------|
| `[router]` | `host` | -- | 路由器 IP 地址 |
| `[router]` | `protocol` | `http` | `http` 或 `https` |
| `[monitor]` | `interval` | `60` | 检查间隔（秒） |
| `[monitor]` | `max_retries` | `1` | 连续失败几次触发恢复 |
| `[monitor]` | `recovery_method` | `reload` | `reload` 或 `reboot` |
| `[monitor.signal]` | `enabled` | `false` | 启用 5G 信号监控 |
| `[monitor.signal]` | `require_5g` | `false` | 回退到非 5G 时视为异常 |
| `[monitor.signal]` | `min_5g_rsrp` | `-110` | 最低 5G RSRP（dBm） |
| `[telemetry]` | `endpoint` | -- | OTLP 端点地址 |
| `[telemetry.metrics]` | `enabled` | `false` | 启用指标导出 |

### 恢复机制

默认的 `reload` 恢复流程：

1. 把 Preferred Access Technology 从当前值切换到 `NR5G-SA`
2. 等待一段时间，再切回原来的值
3. 如果监控目标仍然异常，重启路由器

`reboot` 方式会跳过重载步骤，直接重启。

## 部署

systemd 服务文件在 `deploy/` 目录下。安装为系统服务：

```bash
sudo useradd -r -s /usr/sbin/nologin monitor
sudo install -D -m 640 -o monitor config.toml /opt/zyxel-nr5103-monitor/config.toml
sudo install -D -m 755 target/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor
sudo install -m 644 deploy/zyxel-nr5103-monitor.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyxel-nr5103-monitor
```

查看日志：

```bash
journalctl -u zyxel-nr5103-monitor -f
```

musl 交叉编译版本需要把二进制路径换成对应目录，比如 `target/x86_64-unknown-linux-musl/release/zyxel-nr5103-monitor`。

## 交叉编译

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

要把 aarch64 设为默认 target，编辑 `.cargo/config.toml`。

## OpenTelemetry

程序支持通过 OTLP 导出监控指标。所有遥测信号默认关闭。

启用指标导出：

```toml
[telemetry]
endpoint = "http://localhost:4317"
protocol = "grpc"       # 或 "http/protobuf"
export_interval = 60

[telemetry.metrics]
enabled = true
```

遥测模块在导出前会剥离敏感标识符（IMEI、IMSI、IP 地址、MAC 地址、会话密钥）。

### 导出的指标

#### 设备与系统

| 指标名称 | 类型 | 单位 | 属性 | 说明 |
|----------|------|------|------|------|
| `zyxel.device.uptime.seconds` | Gauge | `s` | -- | 设备运行时长 |
| `zyxel.system.cpu.usage.percent` | Gauge | `%` | -- | CPU 使用率 |
| `zyxel.system.memory.bytes` | Gauge | `By` | `state` = `total`/`free` | 总内存与可用内存 |

#### 蜂窝信号

| 指标名称 | 类型 | 单位 | 属性 | 说明 |
|----------|------|------|------|------|
| `zyxel.cellular.signal.dbm` | Gauge | `dBm` | `radio`, `kind` | 信号强度（RSSI、RSRP） |
| `zyxel.cellular.signal.db` | Gauge | `dB` | `radio`, `kind` | 信号质量（RSRQ、SINR） |

`radio` 取值：`lte`、`nr_nsa`、`scc` | `kind` 取值：`rssi`、`rsrp`、`rsrq`、`sinr`

#### 网络接口

| 指标名称 | 类型 | 单位 | 属性 | 说明 |
|----------|------|------|------|------|
| `zyxel.interface.traffic.bytes` | Counter | `By` | `interface_type`, `interface_name`, `direction` | 接口流量字节数（增量） |
| `zyxel.interface.traffic.packets` | Counter | `{packet}` | `interface_type`, `interface_name`, `direction` | 接口流量包数（增量） |
| `zyxel.interface.errors` | Counter | `{error}` | `interface_type`, `interface_name`, `direction` | 接口错误数（增量） |
| `zyxel.interface.discards` | Counter | `{packet}` | `interface_type`, `interface_name`, `direction` | 接口丢包数（增量） |

`interface_type` 取值：`ip`、`ethernet` | `direction` 取值：`sent`、`received`

#### LAN 端口

| 指标名称 | 类型 | 单位 | 属性 | 说明 |
|----------|------|------|------|------|
| `zyxel.lan.port.up` | Gauge | -- | `port_name` | `1` = 链路正常，`0` = 链路断开 |

#### 连通性监控

| 指标名称 | 类型 | 单位 | 说明 |
|----------|------|------|------|
| `zyxel.monitor.connectivity.rtt.ms` | Histogram | `ms` | 连通性探测往返时延 |
| `zyxel.monitor.connectivity.failures` | Counter | -- | 连通性探测失败次数 |

#### 信号质量监控

| 指标名称 | 类型 | 单位 | 属性 | 说明 |
|----------|------|------|------|------|
| `zyxel.monitor.signal.degraded` | Counter | -- | `reason` | 信号质量劣化检测次数 |
| `zyxel.monitor.signal.recovery.attempts` | Counter | -- | -- | 由信号问题触发的恢复尝试次数 |
| `zyxel.monitor.signal.recovery.successes` | Counter | -- | -- | 由信号问题触发的恢复成功次数 |
| `zyxel.monitor.signal.recovery.failures` | Counter | -- | -- | 由信号问题触发的恢复失败次数 |

`reason` 取值：`missing_5g`、`weak_5g_rsrp`

#### 恢复：重启

| 指标名称 | 类型 | 单位 | 说明 |
|----------|------|------|------|
| `zyxel.monitor.reboot.attempts` | Counter | -- | 重启恢复尝试次数 |
| `zyxel.monitor.reboot.successes` | Counter | -- | 重启命令成功次数 |

#### 恢复：重载

| 指标名称 | 类型 | 单位 | 说明 |
|----------|------|------|------|
| `zyxel.monitor.reload.attempts` | Counter | -- | 重载恢复尝试次数 |
| `zyxel.monitor.reload.successes` | Counter | -- | 重载恢复成功次数 |
| `zyxel.monitor.reload.failures` | Counter | -- | 重载恢复失败次数 |
| `zyxel.monitor.reload.duration.seconds` | Histogram | `s` | 重载恢复过程总耗时 |

## 技术说明

- HTTP 模式会获取路由器的 RSA 公钥来加密登录凭据。HTTPS 模式跳过这一步，直接发 JSON。
- 程序接受路由器的自签名证书，这是为局域网环境刻意保留的行为。
- `config` crate 处理多路径 TOML 加载，后加载的文件会覆盖前面的。
- 指标采集失败会记录日志，但不会触发恢复。

---

*本项目与 Zyxel Group Corporation（台湾合勤集团）及其子公司无关，亦未经其认可或授权。Zyxel 为相关所有者的商标。本项目为独立的第三方作品。*

---

<div align="center">
  <sub>Built with <a href="https://opencode.ai/">OpenCode</a> &middot; AI 辅助编码</sub>
  <br>
  <sub>基于 <a href="LICENSE">MIT 许可证</a> 发布</sub>
</div>
