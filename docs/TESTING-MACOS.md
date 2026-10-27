# Testing a Runebender DMG

Use this checklist for each release candidate and retain the results with its artifact hashes.
A successful Cargo build, signature check, or screenshot covers only part of the installation experience.
Test disposable copies of fonts and keep important source files under version control.

## Build a local candidate

From the Runebender checkout:

```sh
./scripts/build_dmg.sh
```

This builds the current working tree, converts `assets/icons/runebender.png` into a macOS icon, bundles the app and sample Shapes font, and creates a drag-to-Applications DMG.
Each invocation chooses a fresh timestamped directory under `target/macos/` and prints the paths when it succeeds.
The output includes `SHA256SUMS` and `build-info.json`, which records the source commit, dirty state, architecture, icon and lockfile hashes, and final DMG hash.
Local candidates deliberately allow uncommitted experiments.
They use ad-hoc signing and are not the public, notarized download.

Run `./scripts/build_dmg.sh --help` for options such as `--icon`, `--target`, and `--output`.
The current icon is temporary artwork supplied by Eli for packaging tests.
Replace the square source PNG with a 1024-pixel or larger PNG when the artwork changes; the next build regenerates the ICNS at all standard sizes.

## Install and use the app

1. Record the DMG path, SHA-256, build receipt, Mac architecture, macOS version, and whether this is a local or Developer ID candidate.
2. Open the DMG in Finder and inspect the application icon and Applications shortcut.
3. Quit any existing Runebender instance after saving work, then drag the app into Applications.
   Preserve the previous app if it is needed for comparison; do not overwrite a running application.
4. Eject the disk image before starting the installed copy.
5. Open Runebender from Applications, without starting it from a terminal or source checkout.
   Check the Dock icon, application name, native menu bar, normal window activation, and quit/relaunch behavior.
   Optionally choose Keep in Dock for convenient access.
6. Use File > Open on a disposable UFO and a Designspace, edit outlines and widths, undo/redo, save, close, and reopen.
   Confirm the intended changes and preservation of unrelated source data, including through a Git diff of the copies where practical.
7. Check dialogs, clipboard operations, keyboard shortcuts, resizing, rendering, and font compilation/export for the supported formats.
8. Check behavior with no installed local AI models or tools, then separately test each external tool advertised as supported.
   Missing optional tools should give a useful explanation rather than breaking core editing.
9. Replace a closed older copy with a newer candidate and verify preferences and ordinary source documents still work.
   Reopening a recently used file and preserving unsaved-document prompts also need interactive checks.

The installed app should work without Rust, Homebrew, the source checkout, or developer shell variables.
Command-line installation tests can catch missing resources, but Finder launch is needed to assess the actual user experience.
The initial bundle does not advertise Finder document associations; opening a font through File > Open is the expected route.
Support for double-clicking font sources or dropping them onto the app icon needs separate native event handling and testing.

## Public-distribution checks

Use the signed command in [RELEASING.md](RELEASING.md#signed-public-package), not the local wrapper.
Verify the app's Developer ID signature, hardened runtime, notarization acceptance, and stapled ticket, and verify the signed DMG and its ticket.
Build all resources into the bundle before signing; never modify the application contents afterward.
The packager signs Sparkle's nested executables and helpers explicitly before signing the framework and outer app.
Verify the copied application after ejecting the DMG, including `Contents/Frameworks/Sparkle.framework`.

Download the final DMG with a browser on a separate Mac or clean test environment that has not already approved this build.
Keep the download's quarantine metadata, install using Finder, eject, and launch the installed copy.
Do not clear quarantine, disable Gatekeeper, or use Open Anyway to make this test pass.
Repeat first launch offline from a fresh test state to exercise stapled notarization tickets.
A clean user account is useful for configuration testing but may still share machine-wide Gatekeeper history; a separate clean machine or restored VM gives stronger evidence.
Record any VM or hardware coverage limits rather than treating them as equivalent to every supported Mac.

Test the oldest advertised macOS version and each advertised architecture.
The Mach-O minimum version is a compatibility constraint, not proof that the app works on that OS.
Do not publish an Intel download solely because `lipo` confirms an Intel binary.

## Native updates

The alpha uses the feed and public verification key in `release.toml`.
The release version is `0.1.0-alpha.1`; its numeric `CFBundleVersion` is `1`.
Test update installation with two distinct build numbers and the same bundle identifier and signing identities.
Use disposable app copies and font sources; keep test feeds and test builds out of the public alpha feed.

1. Launch the installed candidate and confirm that Runebender > Check for Updates… is enabled.
   With the published empty alpha feed, a manual check should report that no update is available.
2. Repeat the manual check with networking unavailable and confirm that an understandable error leaves the editor usable.
3. Configure an isolated older test app to read a private test feed containing the exact newer candidate DMG and its generated EdDSA signature.
   Confirm that startup offers the newer build and that the menu can request the same update.
4. Download the update, leave a disposable font unsaved, and request installation.
   Cancel the document prompt and confirm that the editor and unsaved drawing remain open.
   Save the font, resume through Check for Updates…, and verify that the new app relaunches and the saved edits persist.
5. In a separate disposable run, exercise Discard and confirm that only the deliberately unsaved edits are discarded.
6. Confirm that the installed app has the intended version, numeric build number, Developer ID signature, and stapled ticket after replacement.
   Test that a modified archive with the original EdDSA signature is rejected before installation.

The unit test for restart decisions covers Save, Discard, and Cancel state transitions.
It does not prove Sparkle's native dialogs, helper processes, downloaded archive verification, or actual replacement and relaunch.
Record these interactive results separately before advertising automatic updates as validated.

## Automated checks and their limits

The package script checks the binary architecture and non-system dynamic-library dependencies, validates the plist and app signature, and verifies the disk image.
Its signed path additionally notarizes, staples, and runs Gatekeeper assessments, using `syspolicy_check` for the app where available and `spctl` on older hosts.
Smoke testing should mount without opening Finder, copy the app to a temporary location, eject, verify the copied signature, run `--version`, and render headless screenshots with a minimal environment.
Inspect those screenshots, including a default Gray theme font view, rather than accepting their mere existence.
These automated checks do not prove pointer input, native menus, dialogs, accessibility, saving, or the first browser-downloaded launch.

Record each check as passed, failed, or not run, with the exact tested artifact hash.
Keep local candidate results separate from Developer ID and public-download results.

On macOS 14 and later, use `syspolicy_check distribution /path/to/Runebender.app` for the app assessment; on older systems use `spctl --assess --type execute --verbose=2 /path/to/Runebender.app`.
For the DMG, use `spctl --assess --type open --context context:primary-signature --verbose=2 /path/to/Runebender.dmg`.
Run these against public signed candidates; a local ad-hoc signature is not expected to pass Developer ID distribution checks.

## Primary references

- [Apple: Packaging Mac software for distribution](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple DTS: Testing a Notarised Product](https://developer.apple.com/forums/thread/130560)
- [Apple: macOS Code Signing In Depth](https://developer.apple.com/library/archive/technotes/tn2206/_index.html), especially bundle layout and nested code
- [Apple: App icons](https://developer.apple.com/design/human-interface-guidelines/app-icons)
- [Sparkle: Testing updates](https://sparkle-project.org/documentation/#6-test-sparkle-out)
