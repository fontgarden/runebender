# Steam Deck plan

Written on October 21, 2026.
This is a plan, not a record of finished work.
Nothing in it was run on a Steam Deck yet.
Check each claim marked "unverified" on the device before you build on it.

## 1. Verdict

Runebender can run on a Steam Deck, and the Deck's controls can drive it.
The work splits into three layers, and each layer is useful on its own.

| Layer | What it gives | Code in Runebender |
| --- | --- | --- |
| 0. Run it | Runebender builds and runs on SteamOS desktop mode | None |
| 1. Fit it | The window, keys, and hit targets fit a 1280x800 handheld | Small |
| 2. Native gamepad | Runebender reads the controller itself, with analog nudging and on-screen hints | Medium |

Layer 0 and layer 1 cover most of the goal.
Steam Input already maps every button, trackpad, and gyro to keyboard and mouse events, with chords, layers, and radial menus.
Runebender dispatches almost everything through one keyboard action table.
Layer 2 is the stretch goal and depends on one open question (section 5.3).

## 2. What the Deck is

| Property | Value |
| --- | --- |
| CPU | 4 Zen 2 cores, 8 threads, x86_64 |
| GPU | AMD Van Gogh, RDNA 2, 8 compute units, Vulkan through RADV |
| Memory | 16 GB LPDDR5, shared between CPU and GPU (UMA) |
| Screen | 1280x800, 7 inches, touchscreen |
| OS | SteamOS 3, Arch based, read-only root, KDE Plasma desktop mode |
| Desktop session | Wayland by default from SteamOS 3.8, X11 before that |
| Controls | ABXY, D-pad, two sticks, two trackpads, two triggers, two bumpers, four back buttons, Steam and menu buttons, gyro |
| Power | 4 to 15 W handheld, more when docked |

The root file system is read-only.
`sudo steamos-readonly disable` makes it writable, and a system update discards those changes.
Everything in this plan stays in `/home/deck`, in a Distrobox container, or in a Steam shortcut.

## 3. How Runebender meets the Deck today

The repository survey found these facts.

- All platform-specific code is `#[cfg(target_os = "macos")]`.
  There is no Linux, Wayland, or X11 code, and none is needed.
  The pinned winit 0.30 builds with X11 and Wayland, and wgpu 28 picks Vulkan.
- On Linux the root view is the in-window Masonry menu bar in `src/application/widgets/menu_shell.rs`, 34 px tall.
- Every shortcut lives in `ACTIONS` in `src/application/actions.rs`.
  `action_for_key` parses `CmdOrCtrl+`, `Shift+`, `Alt+`, one character, or `Escape`.
  It does not parse named keys such as arrows, F-keys, Tab, or Page Down.
- The canvas handles arrows as nudges, Space as pan, and scroll as zoom only.
  A two-finger scroll never pans.
- There is no gamepad, joystick, or touch code and no such dependency.
  The pinned `ui-events-winit` 0.3 turns touches into pointer events, so a touchscreen tap acts as a click.
- The default window is 1336x840 logical pixels, which is larger than 1280x800.
  The minimum width is about 804 px with both docks open.
- Hit targets are 19 to 28 px, and splitters are 8 to 16 px.
  The design system has an unused `ControlSize::Touch` of 44 px.
- Chat runs a supervised `llama-server` child with `-ngl 99`, `-c 32768`.
  The binary comes from the app bundle, `RUNEBENDER_LLAMA_SERVER`, or PATH.
  The pinned llama.cpp build is `b11756`, bundled for macOS only.
- Chat models are folders under `$RUNEBENDER_MODELS`, `~/Runebender/models`, or `~/.runebender/models`.
  The Virtua sketch model looks in `~/runebender/models` (lowercase) and runs through a Python venv from `font-garden-lab`.
  On a case-sensitive file system these are two folders, so set the environment variables on the Deck.
- The live MCP socket is a Unix socket under `$TMPDIR/runebender-live-*/session.sock`.
  There is no TCP listener.

## 4. Environment setup over SSH

This is a procedure.
Steps 4.1 and 4.2 happen on the Deck's own screen.
The rest can run from the laptop.

### 4.1 Turn on SSH (on the Deck)

1. Hold the power button and choose Switch to Desktop.
2. Open Konsole.
3. Run `passwd` and set a password for the `deck` user.
4. Run `sudo systemctl enable --now sshd`.
5. Run `ip -4 addr show` and note the Wi-Fi address.

The sshd unit lives on the read-only root and survives updates.
`steamdeck.local` does not resolve from this laptop (tested today), so use the address.

### 4.2 Trust the laptop (from the laptop)

1. Run `ssh-copy-id deck@<address>`.
2. Add a `Host deck` entry to `~/.ssh/config` with the address and user.
3. Make sure that `ssh deck uname -a` prints a SteamOS kernel.

### 4.3 Build toolchain in a container

The scripts in [`steamdeck/`](../steamdeck/README.md) do this section and the next one.
`steamdeck/build.sh` builds in the container, and `steamdeck/install_steam.py` adds the shortcut and art to Steam.
The steps below describe what they do.

SteamOS has no compiler, no headers, and no `pkg-config`.
Do not install them with `pacman` on the host.
SteamOS ships Distrobox and Podman, so put the toolchain in an Arch container in `/home/deck`.

1. On the Deck, run `distrobox create --yes --name rb --image docker.io/library/ubuntu:24.04`.
   An Arch image builds too, but its glibc 2.44 is newer than the SteamOS 2.41, so that binary only runs through `distrobox enter`.
2. Run `distrobox enter rb`.
3. Inside the container, install the packages that `.github/workflows/ci.yml` installs, plus `build-essential pkg-config curl git`.
   This list mirrors `.github/workflows/ci.yml`. Prune what the Deck build does not ask for.
4. Install rustup into `~/.cargo`.
   `rust-toolchain.toml` pins Rust 1.96, and rustup installs it on the first build.
5. Make sure that `vulkaninfo --summary` inside the container names the Van Gogh GPU.
   Distrobox shares `/dev/dri` and the Wayland socket by default (unverified on SteamOS).

The Deck has 16 GB of RAM, so the repository rule applies: set `CARGO_BUILD_JOBS=2` and never overlap Cargo commands.
A clean release build of Xilem, Vello, and wgpu on four Zen 2 cores takes a long time, probably 20 to 40 minutes (unverified).
Two alternatives shorten the loop:

- Build on the Deck once, then `rsync` source changes and rebuild incrementally.
- Add an artifact upload to the Ubuntu CI job and `scp` the binary.
  The Ubuntu binary needs only glibc, fontconfig, wayland, xkbcommon, and the Vulkan loader, which the SteamOS host has.

### 4.4 Run Runebender on the host

The binary built in the container links against libraries that SteamOS also ships.
Copy it to `~/.local/bin/runebender` and run it from the host first.
If it fails to load a library, run it through `distrobox enter rb -- runebender` instead, or use `distrobox-export --app`.

Then add it to Steam:

1. In desktop mode, open Steam, choose Add a Non-Steam Game, and browse to the binary or a wrapper script.
2. Open the shortcut's Controller Settings and pick a layout.
3. Test the layout in Game Mode as well as desktop mode (section 5.1 explains why).

Set `RUNEBENDER_MODELS`, `RUNEBENDER_MODELS_DIR`, and `RUNEBENDER_LLAMA_SERVER` in the wrapper script, because Steam does not read the shell profile.

### 4.5 Local models

llama.cpp publishes Linux Vulkan binaries for each build (`llama-<build>-bin-ubuntu-vulkan-x64.zip`).
They run on the SteamOS host without a container.

1. Download the Vulkan build that matches `b11756` in `scripts/bundle_llama.py`, or a newer one.
2. Unpack it into `~/.local/share/llama` and set `RUNEBENDER_LLAMA_SERVER` to `llama-server` in that folder.
3. Copy model folders from the laptop with `rsync`, smallest first.
4. Add a `model.json` with `"context": 8192` to each folder, because the default 32768 context costs memory the Deck does not have.
5. Run `llama-bench` on each model before you run it in Runebender.

Which laptop models fit:

| Model folder | Size on disk | On the Deck |
| --- | --- | --- |
| qwen3-1.7b | 1.7 GB | Fits, fully on GPU |
| qwen3-4b, qwen3.5-4b | 2.3 to 2.7 GB | Fits, fully on GPU, the best first test |
| qwen3-8b | 4.7 GB | Fits, likely fully on GPU |
| gpt-oss-20b | 11 GB | Marginal: fits in RAM, probably not in the GPU budget, only 3.6B active parameters so speed can still be usable |
| gemma-4-26b-a4b, qwen3.6-27b | 16 GB | Does not fit |
| qwen3.6-35b-a3b | 21 GB | Does not fit |

Speed expectations come from the llama.cpp Vulkan scoreboard, Llama 2 7B Q4_0:

| Device | Prompt tokens/s | Generation tokens/s |
| --- | --- | --- |
| Steam Deck, flash attention on | 164 | 18 |
| Apple M4 Max (this laptop is an M4 Pro, so lower) | 557 to 725 | 72 to 75 |

A 4B model at Q4 lands near 30 tokens/s, and the 8B near 15 (estimated from the 7B row, unverified).
Two memory facts matter and both are unverified on SteamOS:

- The amdgpu driver caps GPU-visible system memory (GTT) at about half of RAM by default, so about 8 GB.
  Larger models need partial offload with a smaller `-ngl`, or a kernel parameter change that lives on the read-only root.
- Plasma, Steam, and Runebender take 3 to 4 GB, so the model plus context budget is about 11 GB.

A Mozilla.ai benchmark from July 2026 ran Qwen3.5 9B Q4_K_M on a Deck OLED with llama.cpp Vulkan, and skipped 27B for memory.
It also measured a 1.6x prompt-processing gain from a newer Vulkan shader compiler, so prefer a recent llama.cpp build.
ROCm does not support this GPU, and Vulkan is the right backend.

The Virtua sketch model needs the `font-garden-lab` venv and `img2bez`.
Set that up in the container after chat works, as a second step.

### 4.6 MCP from the laptop to the Deck

The live socket is local to the Deck, but the MCP host speaks stdio.
Point a Claude Code MCP entry at `ssh deck /home/deck/.local/bin/runebender mcp --live`.
The SSH shell and the GUI session must share the same `TMPDIR` and user, or `editor_sessions` finds nothing.
The GUI session on SteamOS sets `TMPDIR`, so make sure that the wrapper script exports the same path (unverified).

## 5. Controls

### 5.1 Layer 0: Steam Input only

Steam Input turns controller input into keyboard and mouse events through uinput, so Runebender sees a normal keyboard and mouse.
This works today with no code.

Two facts shape it:

- In Game Mode (gamescope), the shortcut's own layout applies.
- In desktop mode, non-Steam apps always get the global Desktop layout (Valve bug steam-for-linux#8904, reported 2022, still open as far as the search shows).
  So either edit the Desktop layout or run Runebender from Game Mode.

Proposed first layout, built only from shortcuts that exist in `actions.rs` today:

| Control | Sends | Runebender action |
| --- | --- | --- |
| Right trackpad | Mouse | Pointer |
| Right trackpad click | Left click | Click |
| Right trigger | Left click | Click or drag with a stick |
| Left trigger | Space (hold) | Pan |
| Right stick | Mouse, slow | Fine pointer |
| Left stick | Arrow keys, repeat | Nudge selection |
| D-pad | Arrow keys | Nudge selection |
| A | Enter | Confirm (needs a binding, see 5.2) |
| B | Escape | Overview |
| X | V | Select tool |
| Y | P | Pen tool |
| Left bumper | Ctrl+Z | Undo |
| Right bumper | Ctrl+Shift+Z | Redo |
| L4 | Ctrl+S | Save |
| L5 | Ctrl+0 | Zoom to fit |
| R4, R5 | Mode shift | Hold for a second layer: Knife, Measure, Ellipse, Rectangle, Hyper Pen |
| Left trackpad | Radial menu | Tools and the most used Edit items |
| Gyro | Mouse, while right trigger is half pressed | Precise drags |
| Touchscreen | Pointer | Taps act as clicks already |

Scroll wheel on a trackpad edge gives zoom, because scroll zooms in Runebender.

### 5.2 Layer 1: make the editor fit a handheld

These changes are small and help every Linux desktop, not only the Deck.

1. Clamp the default window to the monitor, or start maximized when the screen is smaller than the default.
   Today the window manager clamps 1336x840 on its own, and the result is untested.
2. Teach `action_for_key` named keys: arrows, Tab, Enter, Page Up and Page Down, F-keys.
   Then every nudge, zoom, and glyph-navigation action can have a shortcut, and Steam Input can send it.
3. Add shortcuts for actions a controller needs but that have none today: zoom in and out, next and previous glyph, next and previous tool, toggle each dock.
4. Add a `handheld` interface preset that uses `ControlSize::Touch` for rows and widens splitters.
   Select it from the config file or `RUNEBENDER_UI_PRESET`.
   The design-system tests keep sizes in the token files.
5. Make a two-finger scroll pan and keep pinch for zoom.
   `ui-events-winit` already delivers `PointerGesture::Pinch`.
6. Make sure that the in-window menu bar opens with the keyboard, so a controller can reach every menu item.
7. Record the Steam Input layout as a file in `assets/steamdeck/` with the install steps, so the user can import it.

### 5.3 Layer 2: native gamepad support

Native support reads the controller in Runebender.
It gives analog nudging with velocity, pressure-like trigger control, on-screen button hints, and a layout that does not depend on Steam.

How the controller appears on Linux:

- With Steam running, Steam Input shows a virtual Xbox 360 pad (USB id 28DE:11FF) through uinput when the layout uses a gamepad template.
  That pad lacks the Steam and menu buttons and the four back buttons.
- Without Steam, the kernel `hid-steam` driver shows a "Steam Deck" device with every button, and it turns the trackpad pointer off.
  While Steam runs, the kernel removes that device.
- Raw hidraw access shows every button and keeps the trackpads working, but it needs udev rules and must re-disable lizard mode every second.
  GNOME's libmanette chose this path.

Recommended path: read the virtual Xbox 360 pad with the `gilrs` crate (evdev backend, needs `libudev`).
It is one dependency and works for any gamepad on any Linux desktop. Steam Input still handles the back buttons and trackpads through layer 0.
The two layers combine: Steam Input sends keys and mouse for some controls, and Runebender reads the pad for analog ones.

Open question: Masonry and Xilem have no gamepad event.
A `gilrs` poll thread must feed events into the app through the winit event-loop proxy, as an `AppAction`, or through a Masonry `Action`.
Confirm how the pinned `masonry_winit` accepts external messages before you start this layer.
If it needs a change in the `eliheuer/xilem` fork, the layer costs more than the rest of the plan.

The hidraw path is a later option if the back buttons must reach Runebender directly.

## 6. Order of work

1. Setup over SSH (section 4.1 to 4.4) and a first run of the unchanged editor.
   This proves layer 0 and finds the Linux faults that `docs/RELEASING.md` already expects: dialogs, input, and GPU.
2. Local models (section 4.5) with `llama-bench` numbers for each model that fits.
3. The Steam Input layout (section 5.1), tested in Game Mode and desktop mode.
4. Layer 1 changes (section 5.2), each a small commit with a headless screenshot at 1280x800.
5. Decide layer 2 after the open question in section 5.3 is answered.

## 7. Results so far

Recorded on October 21, 2026.

- The editor builds on the Deck in a Distrobox container and runs on the SteamOS host from an Ubuntu 24.04 build.
  The release build takes 18 to 24 minutes on the Deck.
- Runebender is a Non-Steam Game with library art, and it launches from Gaming Mode and from the desktop.
- Gaming Mode runs apps on gamescope's X11 server, so the binary needs `libxkbcommon-x11`, which SteamOS has.
  On X11 winit guessed a 2.2x scale, so the launcher pins `WINIT_X11_SCALE_FACTOR=1`.
- With no layout chosen, Steam used its "Gamepad with camera controls" template, and the pointer and D-pad did nothing useful.
  The layout in `steamdeck/controller_neptune.vdf` goes in Steam's `config/runebender/` folder, and the user selects it once.
- The Deck drops off Wi-Fi when it idles in Gaming Mode, so long SSH jobs need `setsid nohup` and a watcher that retries.

## 8. Risks

- The Ubuntu Clippy job fails at the current commit, so the Linux build is not clean yet.
- `rfd` uses the XDG portal on Linux, and the Open dialog is untested under Plasma.
- The Wayland session on SteamOS 3.8 is new, and winit under gamescope is untested.
- Launching a Distrobox app from Game Mode works for emulators, but it is unverified for a Vulkan app that opens its own window.
- A 1.5x display scale drops the logical width to 853 px, right at the editor's minimum.
- Chat under memory pressure can push Runebender into swap, and SteamOS has no swap file by default.

## Sources

- [Performance of llama.cpp with Vulkan, scoreboard](https://github.com/ggml-org/llama.cpp/discussions/10879)
- [Mozilla.ai: benchmarking local LLM servers, including a Steam Deck OLED](https://blog.mozilla.ai/benchmarking-local-llm-servers-llama-cpp-llamafile-lm-studio-and-ollama/)
- [Steam Deck, HID, and libmanette adventures](https://blogs.gnome.org/alicem/2024/10/24/steam-deck-hid-and-libmanette-adventures/)
- [Non-Steam apps always use the Desktop layout in desktop mode, steam-for-linux#8904](https://github.com/ValveSoftware/steam-for-linux/issues/8904)
- [How to enable SSH on the Steam Deck](https://pimylifeup.com/steam-deck-ssh/)
- [gilrs crate](https://docs.rs/crate/gilrs/latest)
- [SteamOS 3.8 preview, Plasma with Wayland by default](https://www.linux.org/threads/phoronix-steamos-3-8-preview-preps-for-steam-machine-kde-plasma-desktop-with-wayland-by-default.64089/)
