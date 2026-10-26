#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Prepare signed update metadata and stage a GitHub alpha release from tested files."""

import argparse
import base64
import hashlib
import html
import json
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request
import xml.etree.ElementTree as ET

import sparkle

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "fontgarden/runebender"
NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", NS)


def run(*command):
    return subprocess.check_output(list(map(str, command)), text=True).strip()


def artifact(directory, local_test=False):
    receipt = json.loads((directory / "build-info.json").read_text())
    dmg = directory / receipt["artifact"]
    if dmg.parent != directory or not dmg.is_file():
        raise ValueError("The receipt must name one DMG inside its artifact directory")
    if hashlib.sha256(dmg.read_bytes()).hexdigest() != receipt["artifact_sha256"]:
        raise ValueError("The artifact no longer matches its tested checksum")
    if not receipt.get("updates", {}).get("enabled"):
        raise ValueError("This artifact does not include the updater")
    if not local_test and (receipt["source_dirty"] or receipt["signing"] != "notarized"):
        raise ValueError("Public releases require a clean-source, notarized artifact")
    if not local_test:
        run("codesign", "--verify", "--deep", "--strict", directory / "volume/Runebender.app")
        run("xcrun", "stapler", "validate", dmg)
        for name in ("app-notarization.json", "dmg-notarization.json"):
            if json.loads((directory / name).read_text()).get("status") != "Accepted":
                raise ValueError(f"Notarization has not passed: {name}")
    return receipt, dmg


def prepare(args):
    directory = args.directory.resolve()
    receipt, dmg = artifact(directory, args.local_test)
    config = sparkle.configuration()["sparkle"]
    if receipt["updates"]["public_key"] != config["public_key"]:
        raise ValueError("The artifact embeds a different Sparkle verification key")
    vendor = sparkle.framework()
    public_key = run(vendor / "bin/generate_keys", "--account", config["keychain_account"], "-p")
    if public_key != config["public_key"]:
        raise ValueError("The Keychain signing key does not match the app's public key")
    version = receipt["version"]
    tag = f"v{version}"
    notes = (ROOT / f"docs/releases/{version}.md").read_text()
    (directory / "release-notes.md").write_text(notes)
    # A fragment makes generate_appcast embed these notes instead of fetching remote HTML.
    (directory / dmg.with_suffix(".html").name).write_text(
        '<pre style="white-space:pre-wrap">' + html.escape(notes) + "</pre>\n"
    )
    prefix = f"https://github.com/{REPOSITORY}/releases/download/{tag}/"
    subprocess.run([
        str(vendor / "bin/generate_appcast"), "--account", config["keychain_account"],
        "--download-url-prefix", prefix, "--link", "https://runebender.org/download.html",
        "--embed-release-notes", "--maximum-deltas", "0", str(directory),
    ], check=True)
    feed = directory / "appcast.xml"
    tree = ET.parse(feed)
    item = tree.find("./channel/item")
    if item is None or item.findtext(f"{{{NS}}}version") != str(receipt["build_number"]):
        raise ValueError("Generated appcast does not match the candidate's build number")
    enclosure = item.find("enclosure")
    if enclosure is None or enclosure.get("url") != prefix + dmg.name:
        raise ValueError("Generated update URL does not match the candidate")
    signature = enclosure.get(f"{{{NS}}}edSignature", "")
    if len(base64.b64decode(signature, validate=True)) != 64:
        raise ValueError("Generated appcast is missing a valid-length EdDSA signature")
    item.find(f"{{{NS}}}shortVersionString").text = version
    tree.write(feed, encoding="utf-8", xml_declaration=True)
    manifest = {
        "schema_version": 1, "available": not args.local_test, "version": version,
        "tag": tag, "channel": "alpha", "source_commit": receipt["source_commit"],
        "build_number": receipt["build_number"],
        "notes_url": f"https://github.com/{REPOSITORY}/releases/tag/{tag}",
        "artifacts": [{
            "platform": "macos", "architecture": "arm64" if "aarch64" in receipt["target"] else "x86_64",
            "url": prefix + dmg.name, "sha256": receipt["artifact_sha256"],
            "size": dmg.stat().st_size, "minimum_os": receipt["minimum_macos"],
        }],
    }
    (directory / "release.json").write_text(json.dumps(manifest, indent=2) + "\n")
    artifact(directory, args.local_test)  # Metadata generation must not change the DMG bytes.
    print(f"Prepared {'LOCAL TEST' if args.local_test else 'public candidate'} metadata in {directory}")




def verify_tag_target(tag, commit, required=False):
    result = subprocess.run(["gh", "api", f"repos/{REPOSITORY}/git/ref/tags/{tag}"],
                            text=True, capture_output=True)
    if result.returncode:
        if not required and "404" in result.stderr + result.stdout:
            return
        raise ValueError(f"Cannot verify release tag {tag}: {result.stderr.strip()}")
    obj = json.loads(result.stdout)["object"]
    for _ in range(4):
        if obj["type"] == "commit":
            if obj["sha"] != commit:
                raise ValueError(f"Tag {tag} points at a different commit")
            return
        if obj["type"] != "tag":
            break
        obj = json.loads(run("gh", "api", f"repos/{REPOSITORY}/git/tags/{obj['sha']}"))["object"]
    raise ValueError("The release tag does not resolve to a commit")


def required_checks(commit):
    checks = json.loads(run("gh", "api", f"repos/{REPOSITORY}/commits/{commit}/check-runs"))["check_runs"]
    required = {"formatting and repository hygiene", "ubuntu-latest", "macos-latest", "security advisories"}
    latest = {}
    for check in sorted(checks, key=lambda check: check["id"]):
        if check["head_sha"] == commit:
            latest[check["name"]] = check["conclusion"]
    missing = {name for name in required if latest.get(name) != "success"}
    if missing:
        raise ValueError(f"Release SHA is missing passing CI: {sorted(missing)}")


def draft(args):
    directory = args.directory.resolve()
    receipt, dmg = artifact(directory)
    manifest = json.loads((directory / "release.json").read_text())
    if not manifest["available"] or manifest["artifacts"][0]["sha256"] != receipt["artifact_sha256"]:
        raise ValueError("Run prepare on this public candidate before creating a draft")
    if not args.installation_tested:
        raise ValueError("Complete docs/TESTING-MACOS.md on this artifact, then pass --installation-tested")
    commit = receipt["source_commit"]
    required_checks(commit)
    tag = manifest["tag"]
    verify_tag_target(tag, commit)
    # A draft is recoverable. Never overwrite an existing release or its assets.
    subprocess.run([
        "gh", "release", "create", tag, str(dmg), str(directory / "SHA256SUMS"),
        str(directory / "release.json"), str(directory / "appcast.xml"),
        "--repo", REPOSITORY, "--target", commit, "--draft", "--prerelease",
        "--title", f"Runebender {receipt['version']}",
        "--notes-file", str(directory / "release-notes.md"),
    ], check=True)


def publish(args):
    directory = args.directory.resolve()
    receipt, dmg = artifact(directory)
    manifest = json.loads((directory / "release.json").read_text())
    tag = manifest["tag"]
    required_checks(receipt["source_commit"])
    verify_tag_target(tag, receipt["source_commit"])
    website = args.website.resolve()
    feed = website / "public/updates/alpha/appcast.xml"
    if not feed.parent.is_dir() or not (website / "src/data").is_dir():
        raise ValueError("The website checkout has not been prepared for release metadata")
    previous = ET.parse(feed) if feed.exists() else None
    if previous is not None:
        for old in previous.findall("./channel/item"):
            if int(old.findtext(f"{{{NS}}}version")) >= receipt["build_number"]:
                raise ValueError("The website feed already has this build or a newer one")
    release = json.loads(run("gh", "release", "view", tag, "--repo", REPOSITORY,
                             "--json", "isDraft,isPrerelease,targetCommitish,assets"))
    if not release["isDraft"] or not release["isPrerelease"] or release["targetCommitish"] != receipt["source_commit"]:
        raise ValueError("The remote draft is not the reviewed prerelease at the selected SHA")
    # Verify the uploaded bytes before making the release public.
    remote_asset = next((a for a in release["assets"] if a["name"] == dmg.name), None)
    if remote_asset is None:
        raise ValueError("The draft has no DMG asset")
    remote_bytes = subprocess.check_output(["gh", "api", "-H", "Accept: application/octet-stream",
        f"repos/{REPOSITORY}/releases/assets/{remote_asset['apiUrl'].rsplit('/', 1)[-1]}"])
    if hashlib.sha256(remote_bytes).hexdigest() != receipt["artifact_sha256"]:
        raise ValueError("The uploaded DMG does not match the tested file")
    subprocess.run(["gh", "release", "edit", tag, "--repo", REPOSITORY, "--draft=false", "--latest=false"], check=True)
    verify_tag_target(tag, receipt["source_commit"], required=True)
    with urllib.request.urlopen(manifest["artifacts"][0]["url"], timeout=120) as response:
        public_hash = hashlib.sha256(response.read()).hexdigest()
    if public_hash != receipt["artifact_sha256"]:
        raise ValueError("Public download verification failed; website metadata was not changed")
    new_tree = ET.parse(directory / "appcast.xml")
    if feed.exists():
        previous = ET.parse(feed)
        channel = new_tree.find("channel")
        for old in previous.findall("./channel/item"):
            build = int(old.findtext(f"{{{NS}}}version"))
            if build >= receipt["build_number"]:
                raise ValueError("The website feed already has this build or a newer one; do not move it backward")
            channel.append(old)
    new_tree.write(feed, encoding="utf-8", xml_declaration=True)
    shutil.copy2(directory / "release.json", website / "src/data/release.json")
    print("Public download verified. Website source updated; build, review, commit and deploy the website next.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "draft", "publish"):
        command = commands.add_parser(name)
        command.add_argument("directory", type=Path, help="completed package output directory")
        if name == "prepare":
            command.add_argument("--local-test", action="store_true", help="generate non-publishable test metadata")
        elif name == "draft":
            command.add_argument("--installation-tested", action="store_true")
        else:
            command.add_argument("--website", type=Path, required=True, help="runebender-dot-org checkout")
    args = parser.parse_args()
    try:
        {"prepare": prepare, "draft": draft, "publish": publish}[args.command](args)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Release stopped: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
