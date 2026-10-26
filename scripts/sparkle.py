#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Fetch the pinned official Sparkle tools; never download code from a Cargo build."""

import base64
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def configuration():
    """Read committed public update settings, validating the verification key."""
    config = tomllib.loads((ROOT / "release.toml").read_text())
    key = base64.b64decode(config["sparkle"]["public_key"], validate=True)
    if len(key) != 32:
        raise ValueError("Sparkle public_key must be a base64-encoded Ed25519 public key")
    if not config["sparkle"]["feed_url"].startswith("https://"):
        raise ValueError("The update feed must use HTTPS")
    if not 1 <= config["macos"]["build_number"] <= 9999:
        raise ValueError("macOS build_number must be an increasing integer from 1 to 9999")
    return config


def framework():
    """Return a verified vendor cache containing Sparkle.framework and its tools."""
    config = configuration()["sparkle"]
    version = config["version"]
    cache = ROOT / "target" / "vendor" / f"Sparkle-{version}"
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / f"Sparkle-{version}.tar.xz"
    if not archive.exists():
        url = f"https://github.com/sparkle-project/Sparkle/releases/download/{version}/{archive.name}"
        with urllib.request.urlopen(url, timeout=120) as response:
            data = response.read(64 * 1024 * 1024 + 1)
        if len(data) > 64 * 1024 * 1024:
            raise RuntimeError("Sparkle archive exceeded the download limit")
        if hashlib.sha256(data).hexdigest() != config["sha256"]:
            raise RuntimeError("Sparkle archive SHA-256 does not match release.toml")
        archive.write_bytes(data)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != config["sha256"]:
        raise RuntimeError(f"Modified Sparkle archive: {archive}")
    extracted = cache / "distribution"
    if not extracted.exists():
        # Python's data filter rejects traversal and links escaping the extraction root.
        extracted.mkdir()
        with tarfile.open(archive) as source:
            source.extractall(extracted, filter="data")
    bundled = extracted / "Sparkle.framework"
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(bundled)], check=True)
    return extracted


def sign_framework(path, identity):
    """Sign each nested executable and bundle, from the inside out."""
    signatures = {b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca"}
    files = set()
    bundles = []
    for item in path.rglob("*"):
        if item.is_symlink():
            continue
        if item.is_file():
            with item.open("rb") as stream:
                if stream.read(4) in signatures:
                    files.add(item)
        elif item.suffix in (".app", ".xpc", ".framework"):
            bundles.append(item)
    command = ["codesign", "--force", "--sign", identity, "--preserve-metadata=identifier,entitlements"]
    if identity != "-":
        command += ["--options", "runtime", "--timestamp"]
    for item in sorted(files, key=lambda p: len(p.parts), reverse=True):
        subprocess.run([*command, str(item)], check=True)
    for item in sorted(bundles, key=lambda p: len(p.parts), reverse=True) + [path]:
        subprocess.run([*command, str(item)], check=True)


if __name__ == "__main__":
    print(framework())
