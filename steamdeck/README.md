# Runebender on the Steam Deck

The Steam Deck runs Linux, so Runebender runs on it as a Non-Steam Game.
It shows up in Gaming Mode with the other games, with its own library art.
[The Steam Deck plan](../docs/STEAM-DECK.md) explains the controller mapping and the local models.

## Files

| File | Purpose |
| --- | --- |
| `build.sh` | Builds the binary in the `rb` Distrobox container and copies it to `bin/` |
| `run.sh` | Starts the binary. This is the file Steam launches. |
| `install_steam.py` | Writes the shortcut into Steam and copies the art into Steam's grid folder |
| `art/` | Portrait, landscape, hero, and icon images for the Steam library |
| `controller_neptune.vdf` | The Steam Input layout for the Deck's controls |
| `runebender.desktop`, `open-virtua-grotesk.desktop` | Launchers for the Plasma desktop and app menu |

## One time setup on the Deck

1. Hold the power button and pick **Switch to Desktop**.
2. Open **Konsole** and set a password: `passwd`
3. Turn on SSH: `sudo systemctl enable --now sshd`
4. Find the Deck's address: `ip -4 addr show wlan0 | grep inet`

## Build it

SteamOS has no compiler and its root file system is read-only.
The build runs in an Arch container, and the binary runs on the host.

```sh
rsync -az --exclude target --exclude .git ./ deck@<address>:~/Games/runebender/
ssh deck@<address>
distrobox create --yes --name rb --image docker.io/library/ubuntu:24.04
cd ~/Games/runebender && steamdeck/build.sh
```

The first build installs packages, the Rust 1.96 toolchain, and every crate.
It takes a long time on the Deck.
Later builds reuse the container and the `target/` folder.

## Add it to Steam

With Steam closed on the Deck:

```sh
steam -shutdown
cd ~/Games/runebender && python3 steamdeck/install_steam.py
```

Then go back to Gaming Mode.
Runebender is in the library under **Non-Steam**.
Open its controller settings and pick the **Runebender** layout before the first launch.

## The controller layout

`install_steam.py` does not install the layout yet.
Copy it by hand into Steam's config folder for your user:

```sh
C=~/.local/share/Steam/steamapps/common/"Steam Controller Configs"/<your user id>/config
mkdir -p "$C/runebender" && cp steamdeck/controller_neptune.vdf "$C/runebender/"
```

| Control | Does |
| --- | --- |
| Right trackpad | Moves the pointer. Press to click. |
| Right trigger | Click, or drag while moving the pad |
| Left trigger | Hold to pan (Space) |
| Left trackpad | Scroll to zoom. Press for the context menu. |
| Right stick | Slow pointer for fine work |
| Left stick, D-pad | Arrow keys: nudge the selection |
| A | Enter |
| B | Escape, back to the overview |
| X | Select tool. Hold for the Measure tool. |
| Y | Pen tool. Hold for the Hyper pen. |
| Left bumper | Undo |
| Right bumper | Redo |
| L4 (upper back left) | Zoom to fit |
| L5 (lower back left) | Save |
| R4 (upper back right) | Hold for Shift |
| R5 (lower back right) | Hold for Ctrl |
| View button | Tab |
| Menu button | On-screen keyboard |
