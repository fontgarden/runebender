#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Build a standalone macOS app and a drag-to-Applications disk image."""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import sys

import sparkle


ROOT = Path(__file__).resolve().parent.parent
TARGETS = {
    "aarch64-apple-darwin": ("arm64", "apple-silicon"),
    "x86_64-apple-darwin": ("x86_64", "intel"),
}


def install_icon(source, resources, output):
    """Copy an ICNS, or convert a square PNG using Apple's icon tools."""
    destination = resources / "Runebender.icns"
    if source.suffix.lower() == ".icns":
        shutil.copy2(source, destination)
        return
    dimensions = run("sips", "-g", "pixelWidth", "-g", "pixelHeight", source, capture=True)
    width = re.search(r"pixelWidth: (\d+)", dimensions)
    height = re.search(r"pixelHeight: (\d+)", dimensions)
    if not width or not height or width[1] != height[1] or int(width[1]) < 1024:
        raise RuntimeError("The PNG application icon must be square and at least 1024 pixels wide")
    iconset = output / "Runebender.iconset"
    iconset.mkdir()
    for size in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            suffix = "@2x" if scale == 2 else ""
            pixels = size * scale
            run(
                "sips", "-z", pixels, pixels, source,
                "--out", iconset / f"icon_{size}x{size}{suffix}.png",
                capture=True,
            )
    run("iconutil", "--convert", "icns", "--output", destination, iconset)


def run(*command, capture=False, **kwargs):
    """Run argument vectors directly, never through a shell."""
    print("+", " ".join(map(str, command)), flush=True)
    return subprocess.run(
        list(map(str, command)), check=True, text=True,
        stdout=subprocess.PIPE if capture else None, **kwargs,
    ).stdout


def notarize(path, profile, output):
    response = run(
        "xcrun", "notarytool", "submit", path, "--keychain-profile", profile,
        "--wait", "--output-format", "json", capture=True,
    )
    output.write_text(response)
    result = json.loads(response)
    if result.get("status") != "Accepted":
        request_id = result.get("id")
        if request_id:
            run(
                "xcrun", "notarytool", "log", request_id,
                "--keychain-profile", profile, output.with_suffix(".log.json"),
            )
        raise RuntimeError(f"Notarization was not accepted; see {output}")


def package(args):
    if platform.system() != "Darwin":
        raise RuntimeError("Packaging requires macOS and Apple's command-line tools")
    if args.identity and not args.identity.startswith("Developer ID Application:"):
        raise RuntimeError("Use a Developer ID Application signing identity")
    if bool(args.identity) != bool(args.notary_profile):
        raise RuntimeError("--identity and --notary-profile must be supplied together")
    if args.icon and not args.icon.is_file():
        raise RuntimeError(f"Icon does not exist: {args.icon}")
    if args.icon and args.icon.suffix.lower() not in (".icns", ".png"):
        raise RuntimeError("--icon must be a macOS .icns file or a square PNG")
    if args.identity and args.without_updates:
        raise RuntimeError("Public packages must include the updater; --without-updates is for local tests")
    config = sparkle.configuration()
    vendor = None if args.without_updates else sparkle.framework()

    metadata = json.loads(run(
        "cargo", "metadata", "--locked", "--no-deps", "--format-version", "1",
        cwd=ROOT, capture=True,
    ))
    package_info = next(p for p in metadata["packages"] if p["name"] == "runebender")
    version = package_info["version"]
    rustc = run("rustc", "-vV", cwd=ROOT, capture=True)
    host = re.search(r"^host: (.+)$", rustc, re.MULTILINE)[1]
    target = args.target or host
    if target not in TARGETS:
        raise RuntimeError(f"Unsupported macOS target: {target}")
    if args.identity and target not in config["macos"]["targets"]:
        raise RuntimeError("This architecture has not been enabled for public release in release.toml")
    arch, label = TARGETS[target]
    mode = "notarized" if args.identity else "local"
    name = f"Runebender-{version}-macos-{label}-{mode}"
    target_dir = Path(metadata["target_directory"])
    started = datetime.now(timezone.utc)
    stamp = started.strftime("%Y%m%dT%H%M%S%fZ")
    output = (args.output or target_dir / "macos" / f"{name}-{stamp}").resolve()
    if output.exists():
        raise RuntimeError(f"Output already exists; choose a new --output directory: {output}")

    commit = run("git", "rev-parse", "HEAD", cwd=ROOT, capture=True).strip()
    changes = run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT, capture=True)
    if args.identity and changes:
        raise RuntimeError("Build public packages from a clean committed checkout, not a dirty working tree")

    command = ["cargo", "build", "--release", "--locked", "--bin", "runebender"]
    if vendor:
        command += ["--features", "macos-updater"]
    binary_dir = target_dir
    if args.target:
        command += ["--target", target]
        binary_dir /= target
    env = os.environ.copy()
    if vendor:
        env["SPARKLE_FRAMEWORK_PATH"] = str(vendor)
    try:
        memory = int(run(
            "sysctl", "-n", "hw.memsize", capture=True, stderr=subprocess.PIPE,
        ).strip())
    except subprocess.CalledProcessError:
        print("Cannot read physical memory; conservatively limiting Cargo to two jobs.")
        memory = 0
    if memory < 24 * 1024**3:
        env["CARGO_BUILD_JOBS"] = "2"
    run(*command, cwd=ROOT, env=env)
    binary = binary_dir / "release" / "runebender"

    # Refuse local Homebrew or checkout dependencies instead of shipping a broken app.
    dependencies = run("otool", "-L", binary, capture=True).splitlines()[1:]
    for dependency in dependencies:
        library = dependency.strip().split(" (", 1)[0]
        if vendor and library == "@rpath/Sparkle.framework/Versions/B/Sparkle":
            continue
        if not library.startswith(("/usr/lib/", "/System/Library/")):
            raise RuntimeError(f"Non-system dynamic dependency needs bundling: {library}")
    run("lipo", binary, "-verify_arch", arch)
    builds = run("xcrun", "vtool", "-show-build", binary, capture=True)
    minimum = re.search(r"^\s*minos ([0-9.]+)$", builds, re.MULTILINE)
    if not minimum:
        raise RuntimeError("Cannot determine the binary's minimum macOS version")
    minimum_os = minimum[1]
    if vendor:
        minimum_os = max(
            (minimum_os, config["sparkle"]["minimum_macos"]),
            key=lambda text: tuple(map(int, text.split("."))),
        )
    numeric_version = re.fullmatch(r"(\d+\.\d+\.\d+)(?:-[0-9A-Za-z.-]+)?", version)
    if not numeric_version:
        raise RuntimeError(f"Unsupported release version: {version}")
    if args.identity:
        after_build = run("git", "status", "--porcelain", "--untracked-files=all", cwd=ROOT, capture=True)
        after_commit = run("git", "rev-parse", "HEAD", cwd=ROOT, capture=True).strip()
        if after_build or after_commit != commit:
            raise RuntimeError("Release source changed while compiling; select and validate a new commit")

    output.mkdir(parents=True)
    staging = output / "volume"
    app = staging / "Runebender.app"
    contents = app / "Contents"
    resources = contents / "Resources"
    executable = contents / "MacOS" / "runebender"
    resources.mkdir(parents=True)
    executable.parent.mkdir()
    shutil.copy2(binary, executable)
    executable.chmod(0o755)
    info = {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": "Runebender",
        "CFBundleName": "Runebender",
        "CFBundleExecutable": "runebender",
        "CFBundleIdentifier": "org.runebender.Runebender",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": numeric_version[1],
        "CFBundleVersion": str(config["macos"]["build_number"]),
        "RunebenderReleaseVersion": version,
        "LSMinimumSystemVersion": minimum_os,
        "NSHighResolutionCapable": True,
        "NSSupportsAutomaticGraphicsSwitching": True,
        "NSHumanReadableCopyright": "Copyright 2026 the Runebender Authors",
    }
    if args.icon:
        install_icon(args.icon, resources, output)
        info["CFBundleIconFile"] = "Runebender.icns"
    if vendor:
        info.update({
            "SUFeedURL": config["sparkle"]["feed_url"],
            "SUPublicEDKey": config["sparkle"]["public_key"],
            "SUEnableAutomaticChecks": True,
            "SUAutomaticallyUpdate": False,
            "SUAllowsAutomaticUpdates": False,
            "SUScheduledCheckInterval": 86400,
        })
        frameworks = contents / "Frameworks"
        frameworks.mkdir()
        # ditto preserves the framework's version and helper symlinks.
        run("ditto", vendor / "Sparkle.framework", frameworks / "Sparkle.framework")
        sparkle.sign_framework(frameworks / "Sparkle.framework", args.identity or "-")
        shutil.copy2(vendor / "LICENSE", resources / "Sparkle-LICENSE.txt")
    with (contents / "Info.plist").open("wb") as file:
        plistlib.dump(info, file)
    run("plutil", "-lint", contents / "Info.plist")
    for source in ("LICENSE-APACHE", "LICENSE-MIT", "assets/fonts/VirtuaGrotesk-OFL.txt"):
        shutil.copy2(ROOT / source, resources / Path(source).name)
    # The sample shapes the Local AI Shapes tool trains from, at the path
    # the editor looks in beside the executable.
    shapes = Path("assets/font-sources/static-fonts/Shapes.ufo")
    shutil.copytree(ROOT / shapes, resources / shapes.relative_to("assets"))
    (resources / "README.txt").write_text(
        "Runebender is an experimental font editor. Work on copies of important fonts.\n"
        "Documentation: https://runebender.org\n"
        "Source and issues: https://github.com/fontgarden/runebender\n"
        "Local AI models and their runtimes are installed separately.\n"
    )
    # A local build uses ad-hoc signing. This does not establish a trusted developer identity.
    identity = args.identity or "-"
    signing = ["codesign", "--force", "--sign", identity]
    if args.identity:
        signing += ["--options", "runtime", "--timestamp"]
    run(*signing, app)
    run("codesign", "--verify", "--deep", "--strict", "--verbose=2", app)

    if args.identity:
        archive = output / "notarization.zip"
        run("ditto", "-c", "-k", "--keepParent", app, archive)
        notarize(archive, args.notary_profile, output / "app-notarization.json")
        run("xcrun", "stapler", "staple", app)
        run("xcrun", "stapler", "validate", app)
        archive.unlink()

    (staging / "Applications").symlink_to("/Applications")
    (staging / "Read me.txt").write_text(
        "Drag Runebender.app to Applications, then eject this disk image.\n"
        "Launch Runebender from Applications and use File > Open to open a font.\n"
        "Runebender is experimental; work on copies of important fonts.\n"
        "Documentation: https://runebender.org\n"
        + ("\nLOCAL TEST BUILD: ad-hoc signed, not notarized or ready for public distribution.\n"
           if args.unsigned else "")
    )
    dmg = output / f"{name}.dmg"
    run(
        "hdiutil", "create", "-volname", "Runebender", "-srcfolder", staging,
        "-ov", "-format", "UDZO", dmg,
    )
    if args.identity:
        run(
            "codesign", "--sign", identity, "--timestamp",
            "--identifier", "org.runebender.Runebender.dmg", dmg,
        )
        notarize(dmg, args.notary_profile, output / "dmg-notarization.json")
        run("xcrun", "stapler", "staple", dmg)
        run("xcrun", "stapler", "validate", dmg)
        if shutil.which("syspolicy_check"):
            run("syspolicy_check", "distribution", app)
        else:
            run("spctl", "--assess", "--type", "execute", "--verbose=2", app)
        run(
            "spctl", "--assess", "--type", "open", "--context",
            "context:primary-signature", "--verbose=2", dmg,
        )
    run("hdiutil", "verify", dmg)
    checksum = run("shasum", "-a", "256", dmg.name, cwd=output, capture=True)
    (output / "SHA256SUMS").write_text(checksum)
    receipt = {
        "schema_version": 1,
        "started_at": started.isoformat(),
        "version": version,
        "source_commit": commit,
        "source_dirty": bool(changes),
        "source_changes": changes.splitlines(),
        "rustc": rustc.strip(),
        "target": target,
        "minimum_macos": minimum_os,
        "build_number": config["macos"]["build_number"],
        "updates": {"enabled": bool(vendor), **config["sparkle"]} if vendor else {"enabled": False},
        "signing": mode,
        "icon_sha256": hashlib.sha256(args.icon.read_bytes()).hexdigest() if args.icon else None,
        "cargo_lock_sha256": hashlib.sha256((ROOT / "Cargo.lock").read_bytes()).hexdigest(),
        "artifact": dmg.name,
        "artifact_sha256": checksum.split()[0],
    }
    (output / "build-info.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"\nCreated {dmg}\nApp bundle: {app}\nSHA-256: {checksum.strip()}")
    if args.unsigned:
        print("Local test build only; Developer ID signing and notarization are still required.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    signing = parser.add_mutually_exclusive_group(required=True)
    signing.add_argument(
        "--unsigned", action="store_true",
        help="make an ad-hoc signed local test build without contacting Apple",
    )
    signing.add_argument("--identity", help="full Developer ID Application certificate name")
    parser.add_argument("--notary-profile", help="notarytool Keychain profile for public builds")
    parser.add_argument("--target", choices=TARGETS, help="default: this Mac's Rust host target")
    parser.add_argument(
        "--icon", type=Path, default=ROOT / "assets/icons/runebender.png",
        help="application icon: square PNG (1024px or larger) or ICNS; default: assets/icons/runebender.png",
    )
    parser.add_argument("--output", type=Path, help="new output directory; never overwrites a build")
    parser.add_argument(
        "--without-updates", action="store_true",
        help="local-only developer build without Sparkle; public packages always include updates",
    )
    args = parser.parse_args()
    try:
        package(args)
    except (OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Packaging failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
