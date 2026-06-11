# Dropzone

Dropzone is a GNOME application for temporary file sharing over the local network.

It starts an ephemeral HTTP server on an OS-assigned port, generates a cryptographically random capability URL, displays a local QR code and copyable link, and streams selected files directly to any receiving device with a web browser. The receiving device requires no installation or account.

When sharing is stopped, the server is torn down and the capability URL is invalidated immediately.

## Requirements

- Rust (stable toolchain)
- GTK 4 (>= 4.16 recommended, 4.22+ supported)
- Libadwaita (>= 1.6 recommended, 1.9+ supported)

## Building and Running

Build the application with Cargo:

```sh
cargo build
```

Run unit and integration tests:

```sh
cargo test
```

Run formatting and lint checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

Run Dropzone locally:

```sh
cargo run
```

## Architecture Overview

- `src/share/`: Session capability tokens (256-bit CSPRNG), opaque file identifiers, and share lifecycle state.
- `src/network/`: Local network interface discovery and candidate ranking (RFC 1918 private IPv4 selection, excluding loopback, link-local, and container bridges).
- `src/server/`: Ephemeral Axum HTTP server, bounded-buffer file streaming (`tokio-util::io::ReaderStream`), RFC 6266 / RFC 5987 `Content-Disposition` header construction, and HTML escaping.
- `src/qr/`: Local vector QR code rendering using Cairo directly into a GTK `DrawingArea`.
- `src/window.rs`: Libadwaita application window with native `AdwStatusPage`, `AdwClamp`, `AdwToastOverlay`, and file chooser portal integration.
- `web/`: Plain semantic HTML and CSS receiver page with no JavaScript or external assets.

## License

Dropzone is licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later). See [LICENSE](LICENSE) for the full license text.
