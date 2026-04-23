# Deployment

## Install

```bash
# Create service user
sudo useradd -r -s /usr/sbin/nologin monitor

# Copy config
sudo install -D -m 640 -o monitor config.toml /opt/zyxel-nr5103-monitor/config.toml

# Copy a native build
sudo install -D -m 755 target/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor

# Or copy a musl cross-compiled build
# sudo install -D -m 755 target/x86_64-unknown-linux-musl/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor
# sudo install -D -m 755 target/aarch64-unknown-linux-musl/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor

# Install service
sudo install -m 644 deploy/zyxel-nr5103-monitor.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyxel-nr5103-monitor
```

## Logs

```bash
journalctl -u zyxel-nr5103-monitor -f
```
