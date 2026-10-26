#!/bin/bash
# Starts Runebender on the Steam Deck. Add this file to Steam as a Non-Steam Game.
# The binary comes from `steamdeck/build.sh`. If it was built against a newer glibc
# than SteamOS ships, it runs inside the build container instead.
cd "$(dirname "$0")/.."
LOG_DIR="$HOME/.local/state/runebender"
mkdir -p "$LOG_DIR"
exec >> "$LOG_DIR/launch.log" 2>&1
echo "=== $(date) args: $* WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-} DISPLAY=${DISPLAY:-}"
export PATH="$HOME/.local/bin:$PATH"
export RUNEBENDER_MODELS="$HOME/runebender/models"
export RUNEBENDER_MODELS_DIR="$HOME/runebender/models"
if [ -x "$HOME/.local/share/llama/llama-server" ]; then
    export RUNEBENDER_LLAMA_SERVER="$HOME/.local/share/llama/llama-server"
fi
# Gaming Mode runs apps under gamescope. Its Wayland socket lacks protocols that
# winit needs, so use gamescope's X11 server there instead.
case "${WAYLAND_DISPLAY:-}" in
    gamescope*) unset WAYLAND_DISPLAY; echo "gamescope: using X11 DISPLAY=${DISPLAY:-}" ;;
esac
# On X11 winit guesses the scale from the screen size, and on the Deck's 7 inch
# panel it guesses about 2.2x, which does not fit the editor. The panel is 1280x800.
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -n "${DISPLAY:-}" ]; then
    export WINIT_X11_SCALE_FACTOR="${WINIT_X11_SCALE_FACTOR:-1}"
fi
# With no font on the command line, open the default font if it is installed.
DEFAULT_FONT="${RUNEBENDER_DEFAULT_FONT:-$HOME/Fonts/VirtuaGrotesk/sources/VirtuaGrotesk.designspace}"
if [ "$#" -eq 0 ] && [ -e "$DEFAULT_FONT" ]; then
    set -- "$DEFAULT_FONT"
    echo "opening default font $DEFAULT_FONT"
fi
BIN=./steamdeck/bin/runebender
if "$BIN" --help >/dev/null 2>&1; then
    exec "$BIN" "$@"
fi
echo "binary needs the container glibc; running through distrobox ${RB_BOX:-rb}"
exec distrobox enter "${RB_BOX:-rb}" -- "$BIN" "$@"
