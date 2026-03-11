#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-aarch64-unknown-linux-musl}"

case "$TARGET" in
  aarch64-unknown-linux-gnu)
    LINKER="aarch64-linux-gnu-gcc"
    NEED_LINKER="yes"
    ;;
  armv7-unknown-linux-gnueabihf)
    LINKER="arm-linux-gnueabihf-gcc"
    NEED_LINKER="yes"
    ;;
  aarch64-unknown-linux-musl)
    # Static binary (no glibc dependency) to avoid GLIBC version mismatch on DietPi.
    LINKER="rust-lld"
    NEED_LINKER="no"
    ;;
  *)
    echo "Unsupported target: $TARGET"
    echo "Supported: aarch64-unknown-linux-gnu, armv7-unknown-linux-gnueabihf, aarch64-unknown-linux-musl"
    exit 1
    ;;
esac

if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is required"
  exit 1
fi

if [[ "$NEED_LINKER" == "yes" ]] && ! command -v "$LINKER" >/dev/null 2>&1; then
  echo "Missing linker: $LINKER"
  echo "Install toolchain in WSL, e.g.:"
  if [[ "$TARGET" == "aarch64-unknown-linux-gnu" ]]; then
    echo "  sudo apt-get install -y gcc-aarch64-linux-gnu"
  else
    echo "  sudo apt-get install -y gcc-arm-linux-gnueabihf"
  fi
  exit 1
fi

rustup target add "$TARGET"

TARGET_ENV="$(echo "$TARGET" | tr '[:lower:]-' '[:upper:]_')"
export "CARGO_TARGET_${TARGET_ENV}_LINKER=${LINKER}"

cd "$ROOT_DIR"
cargo build --release --target "$TARGET"

echo "Build done:"
echo "  $ROOT_DIR/target/$TARGET/release/nanohat-oled-rs"
