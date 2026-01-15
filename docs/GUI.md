# QuicView GUI Dependencies

This guide covers system-level dependencies required for GUI/desktop capture on the QuicView server.

## Quick Reference

| OS | Desktop Mode | Terminal-Only Mode |
|----|--------------|-------------------|
| Ubuntu Server (headless) | Requires X11/Wayland setup | ✅ Works out-of-box |
| Ubuntu Desktop | ✅ Works out-of-box | ✅ Works out-of-box |
| Windows Server | ✅ Works out-of-box | N/A |
| macOS | ✅ Works out-of-box | N/A |

## Display Modes

QuicView server supports three display modes configured via `quicview.yaml`:

```yaml
server:
  # Display mode: auto | desktop | terminal
  # - auto: Detect display availability, fallback to terminal if none
  # - desktop: Require display, fail if unavailable
  # - terminal: Terminal-only, no screen capture
  display_mode: auto
```

---

## Ubuntu / Debian

### Option 1: Terminal-Only (Headless)

No additional dependencies required. QuicView will provide shell/terminal access via PTY.

```bash
# Just run the server
./quicview-server --config quicview.yaml
```

### Option 2: Lightweight Desktop with Xvfb

For headless servers that need to provide a GUI experience without physical display:

```bash
# Install X11 virtual framebuffer and a lightweight desktop
sudo apt update
sudo apt install -y \
    xvfb \
    xfce4 \
    xfce4-terminal \
    dbus-x11

# Install screen capture dependencies
sudo apt install -y \
    libxcb1 \
    libxcb-shm0 \
    libxcb-randr0 \
    libxcb-xfixes0

# Start virtual display (1920x1080, 24-bit color)
Xvfb :99 -screen 0 1920x1080x24 &
export DISPLAY=:99

# Start desktop session
startxfce4 &

# Now run QuicView server
./quicview-server --config quicview.yaml
```

### Option 3: Full Desktop (Physical or VNC)

For servers with a physical display or running as a VM with console:

```bash
# Install desktop environment
sudo apt install -y ubuntu-desktop-minimal
# or for lighter weight:
sudo apt install -y xfce4 xfce4-goodies

# Install capture dependencies
sudo apt install -y \
    libxcb1 \
    libxcb-shm0 \
    libxcb-randr0 \
    libxcb-xfixes0

# Reboot and log into desktop session
sudo reboot
```

### Wayland Support (Ubuntu 22.04+)

For Wayland-based desktops (GNOME on Ubuntu 22.04+):

```bash
# Install PipeWire for screen capture
sudo apt install -y \
    pipewire \
    libpipewire-0.3-0 \
    xdg-desktop-portal \
    xdg-desktop-portal-gtk

# Grant screen capture permissions
# The portal will prompt for permission on first capture
```

**Note:** Wayland requires user interaction to grant screen capture permissions via the portal dialog.

---

## RHEL / CentOS / Rocky Linux

### Terminal-Only

No additional dependencies required.

### Desktop Mode

```bash
# Install EPEL repository
sudo dnf install -y epel-release

# Install X11 and desktop
sudo dnf groupinstall -y "Server with GUI"
# or for minimal:
sudo dnf install -y xorg-x11-server-Xvfb xfce4-session

# Install capture dependencies
sudo dnf install -y \
    libxcb \
    xcb-util
```

---

## Alpine Linux (Containers)

For Docker/container deployments with GUI:

```dockerfile
FROM alpine:3.18

# Install X11 and capture dependencies
RUN apk add --no-cache \
    xvfb \
    xfce4 \
    dbus \
    libxcb \
    xcb-util

# Start script
COPY start.sh /start.sh
CMD ["/start.sh"]
```

```bash
# start.sh
#!/bin/sh
Xvfb :99 -screen 0 1920x1080x24 &
export DISPLAY=:99
sleep 1
startxfce4 &
sleep 2
exec /usr/local/bin/quicview-server --config /etc/quicview/quicview.yaml
```

---

## Windows

Windows includes all necessary dependencies for screen capture. No additional setup required.

### Windows Server Core (No Desktop Experience)

Windows Server Core does not have a desktop. Options:

1. **Install Desktop Experience feature:**
   ```powershell
   Install-WindowsFeature Server-Gui-Shell -Restart
   ```

2. **Use terminal-only mode** (once implemented)

---

## macOS

macOS requires granting permissions for screen capture and input control:

1. **Screen Recording:** System Settings → Privacy & Security → Screen Recording → Enable for QuicView
2. **Accessibility:** System Settings → Privacy & Security → Accessibility → Enable for QuicView

These prompts appear automatically on first use.

---

## Verifying Display Availability

Check if QuicView can detect a display:

```bash
# Check X11
echo $DISPLAY
xdpyinfo | head -5

# Check Wayland
echo $WAYLAND_DISPLAY
loginctl show-session $(loginctl | grep $(whoami) | awk '{print $1}') -p Type

# QuicView status endpoint (when server is running)
curl http://127.0.0.1:21110/ready
```

---

## Systemd Service Example

For running QuicView with Xvfb as a system service:

```ini
# /etc/systemd/system/quicview.service
[Unit]
Description=QuicView Remote Access Server
After=network.target

[Service]
Type=simple
User=quicview
Environment=DISPLAY=:99
ExecStartPre=/usr/bin/Xvfb :99 -screen 0 1920x1080x24 -nolisten tcp
ExecStart=/usr/local/bin/quicview-server --config /etc/quicview/quicview.yaml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable quicview
sudo systemctl start quicview
```

---

## Troubleshooting

### "No display available"

```bash
# Check if X server is running
ps aux | grep -E 'Xorg|Xvfb'

# Check DISPLAY variable
echo $DISPLAY

# Try starting Xvfb manually
Xvfb :99 -screen 0 1920x1080x24 &
export DISPLAY=:99
```

### "Permission denied" on Wayland

Wayland requires interactive permission grant. For headless automation, use X11 with Xvfb instead.

### Screen capture is blank

1. Ensure a desktop session is running (not just X server)
2. Check if any windows are open
3. Verify resolution: `xrandr`

### High CPU usage during capture

Reduce capture resolution or frame rate in config:

```yaml
server:
  capture:
    max_fps: 15
    scale: 0.5  # Half resolution
```
