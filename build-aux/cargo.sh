#!/bin/sh
set -e

MESON_BUILD_ROOT="$1"
MESON_SOURCE_ROOT="$2"
OUTPUT="$3"
BUILDTYPE="$4"

export CARGO_TARGET_DIR="$MESON_BUILD_ROOT/target"
export CARGO_HOME="${CARGO_HOME:-$MESON_BUILD_ROOT/cargo-home}"

if [ "$BUILDTYPE" = "release" ]; then
    cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml" --release
    cp "$CARGO_TARGET_DIR/release/dropzone" "$OUTPUT"
else
    cargo build --manifest-path "$MESON_SOURCE_ROOT/Cargo.toml"
    cp "$CARGO_TARGET_DIR/debug/dropzone" "$OUTPUT"
fi
