# Deployment

## Install

```bash
# Copy binary
sudo install -D -m 755 target/release/zyxel-nr5103-monitor /opt/zyxel-nr5103-monitor/zyxel-nr5103-monitor

# Copy config
sudo install -D -m 640 -o monitor config.toml /opt/zyxel-nr5103-monitor/config.toml

# Create user (optional)
sudo useradd -r -s /sbin/nologin monitor

# Install service
sudo install -m 644 deploy/zyxel-nr5103-monitor.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zyxel-nr5103-monitor
```

## Logs

```bash
journalctl -u zyxel-nr5103-monitor -f
```
