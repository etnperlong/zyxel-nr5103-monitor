# zyxel-nr5103-monitor

一个面向 Zyxel NR5103 5G CPE 的 Rust 网络守护程序。

它可以：

- 通过 HTTP 或 HTTPS 登录路由器
- 处理路由器在 HTTP 模式下的加密登录流程
- 定期检查外网连通性
- 在会话失效时重新认证
- 在多次失败后自动重启路由器
- 以 systemd 服务方式运行

## 当前状态

核心功能已完成：

- 路由器客户端
- HTTP 登录加密支持
- TOML 配置加载
- 监控循环与恢复流程
- musl 交叉编译支持
- systemd 部署文件

## 环境要求

- Rust 工具链
- `cargo`
- 如果要在本仓库中构建 x86_64 musl 版本，还需要：`clang`、`llvm-ar` 和对应的 Rust musl target

## 配置方式

程序会按以下顺序读取第一个可用的 TOML 配置：

1. `./config.toml`
2. `$HOME/.config/nr5103/config.toml`（通过不带扩展名的 `.../config` 路径加载）
3. `/etc/nr5103/config.toml`（通过不带扩展名的 `/etc/nr5103/config` 路径加载）

示例配置：

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

### 配置字段说明

#### 顶层配置

- `log_level`：日志级别，例如 `info`、`debug`

#### `[router]`

- `host`：路由器地址，不包含 `http://` 或 `https://` 前缀
- `protocol`：可选 `http` 或 `https`，默认值为 `http`
- `username`：路由器用户名
- `password`：路由器密码

#### `[monitor]`

- `interval`：连通性检查间隔，单位秒
- `url`：用于检测外网连通性的 URL
- `timeout`：请求超时时间，单位秒
- `max_retries`：连续失败多少次后触发恢复逻辑
- `recovery_method`：恢复策略：
  - `reload`（默认）：先临时切换 Preferred Access Technology，再切换回来；如果仍未恢复连通性，则回退到重启
  - `reboot`：跳过 reload 步骤，直接重启

#### `[monitor.reboot]`

- `min_interval`：两次重启之间的最小间隔，单位秒
- `wait_after`：执行重启后等待多少秒，再恢复连通性检查

#### `[monitor.reload]`

- `switch_wait`：临时切换 Preferred Access Technology 后等待的秒数
- `restore_wait`：切换回原有 Preferred Access Technology 后等待的秒数

#### 默认值

- `monitor.interval = 60`
- `monitor.url = "http://www.gstatic.com/generate_204"`
- `monitor.timeout = 5`
- `monitor.max_retries = 1`
- `monitor.recovery_method = "reload"`
- `monitor.reboot.min_interval = 300`
- `monitor.reboot.wait_after = 60`
- `monitor.reload.switch_wait = 15`
- `monitor.reload.restore_wait = 15`

## 构建

调试构建：

```bash
cargo build
```

发布构建：

```bash
cargo build --release
```

运行测试与静态检查：

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## 本地运行

```bash
cargo run --release
```

程序启动后会：

1. 加载配置
2. 初始化日志
3. 连接路由器
4. 执行登录
5. 读取设备信息
6. 启动监控循环

按 `Ctrl+C` 可退出。

## 交叉编译

先安装 target：

```bash
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
```

构建 x86_64 musl 版本：

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

如果你想把 aarch64 设为默认 target，可以取消 `.cargo/config.toml` 中示例配置的注释。

## 部署

systemd 相关文件位于 `deploy/`：

- `deploy/zyxel-nr5103-monitor.service`
- `deploy/README.md`

典型安装流程：

```bash
sudo useradd -r -s /usr/sbin/nologin monitor
sudo install -D -m 640 -o monitor config.toml /opt/zyxel-nr5103-monitor/config.toml
sudo install -D -m 755 target/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor
sudo install -m 644 deploy/zyxel-nr5103-monitor.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyxel-nr5103-monitor
```

如果使用 musl 交叉编译产物，请把二进制路径换成目标平台对应目录，例如：

```bash
target/x86_64-unknown-linux-musl/release/zyxel-nr5103-monitor
```

## 查看日志

```bash
journalctl -u zyxel-nr5103-monitor -f
```

## 说明

- HTTP 模式会使用路由器的 RSA/AES 加密登录流程。
- HTTPS 模式不会走这套加密引导，而是直接发送普通请求。
- 程序会接受路由器的自签名证书，这是为局域网环境刻意保留的行为。

## English documentation

See [README.md](README.md).
