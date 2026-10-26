#!/bin/bash
# Builds Runebender on the Steam Deck inside a Distrobox container.
# SteamOS has no compiler, so the container holds the toolchain and system libraries.
# Use an image whose glibc is not newer than the SteamOS host, or the binary
# only runs through `distrobox enter`. Ubuntu 24.04 (glibc 2.39) is safe.
#
#   distrobox create --yes --name rb --image docker.io/library/ubuntu:24.04
#   steamdeck/build.sh
#
# RB_BOX names the container (default rb). The target folder is target-$RB_BOX,
# so two containers never share build artifacts.
set -euo pipefail
cd "$(dirname "$0")/.."
BOX="${RB_BOX:-rb}"
distrobox enter "$BOX" -- bash -lc '
  set -euo pipefail
  if command -v apt-get >/dev/null; then
    sudo apt-get update -qq
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq build-essential pkg-config curl git \
      libfontconfig-dev libwayland-dev libxkbcommon-dev libx11-dev libxcb1-dev libvulkan1 \
      libssl-dev libglib2.0-dev libgtk-3-dev libxdo-dev
  else
    sudo pacman -Syu --noconfirm --needed base-devel pkgconf fontconfig wayland libxkbcommon \
      libx11 libxcb vulkan-icd-loader vulkan-headers mesa openssl glib2 gtk3 xdotool git curl
  fi
  if [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  # The Deck has 16 GB of RAM: keep two build jobs, as AGENTS.md asks on small hosts.
  export CARGO_BUILD_JOBS=2
  export CARGO_TARGET_DIR="target-'"$BOX"'"
  cargo build --release --locked --bin runebender
'
mkdir -p steamdeck/bin
cp "target-$BOX/release/runebender" steamdeck/bin/runebender
echo "built steamdeck/bin/runebender"
