<div align="center">

<img src="data/icons/hicolor/scalable/apps/io.github.dragonGR.Dropzone.svg" width="128" height="128" alt="Dropzone Logo" />

# Dropzone

A GNOME application for quick, temporary file sharing over your local network.

</div>

Select a file, and Dropzone starts an ephemeral HTTP server on your machine, generates a single-use capability URL, and presents a local vector QR code. Any device on the same local network (phone, tablet, or another computer) can scan the code and download the file directly in a web browser. The receiving device needs no client app, account, or internet connection.

Once sharing is stopped, the server shuts down and the link is immediately invalidated.

## Features

- **No client installation**: Receivers only need a web browser.
- **Local-only streaming**: Transfers stay on the local network without touching external servers.
- **Constant memory usage**: Files are streamed through bounded buffers rather than loaded into RAM.
- **Ephemeral access**: Cryptographically random capability tokens ensure URLs cannot be guessed and expire when sharing stops.
- **GNOME native**: Built with GTK 4, Libadwaita, and vector Cairo QR rendering.

## Building and Running

### With GNOME Builder (Recommended)

1. Open **GNOME Builder**.
2. Select **Open Project...** and choose this repository.
3. Builder detects the Flatpak manifest (`build-aux/io.github.dragonGR.Dropzone.Devel.json`) and configures the GNOME 50 SDK environment automatically.
4. Click **Run** (or press `Ctrl+Shift+Space`).

### With Meson

```sh
meson setup build
ninja -C build
ninja -C build test
sudo ninja -C build install
```

### With Cargo

```sh
# Run tests
cargo test

# Check formatting and lints
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings

# Run Dropzone
cargo run
```

## Translations

Dropzone is localized using GNU gettext. Translations are stored in the `po/` directory.

To run Dropzone with a specific language (for example, Greek):

```sh
LANGUAGE=el cargo run
```

To update the translation template (`po/dropzone.pot`) after modifying user-facing strings:

```sh
ninja -C build dropzone-pot
```

## Firewall Configuration

Dropzone binds to an OS-assigned ephemeral port on your LAN IP (for example, `http://192.168.1.42:43521/s/...`).

If a phone or receiving device on the same local network cannot open the link or the connection times out, your host firewall is likely dropping incoming traffic on high unprivileged ports.

### UFW (Uncomplicated Firewall)

Allow connections from your local subnet:

```sh
sudo ufw allow from 192.168.1.0/24
```

Or allow traffic on your active wireless interface:

```sh
sudo ufw allow in on wlan0
```

### firewalld

Add your local subnet to the trusted zone:

```sh
sudo firewall-cmd --add-source=192.168.1.0/24 --zone=trusted --permanent
sudo firewall-cmd --reload
```
