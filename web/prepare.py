#!/usr/bin/env python3
"""Prepare a browser-only manifest patch for the pinned Xilem adapter.

The adapter crate `xilem_masonry` is copied from the exact git revision the desktop
build uses, and its manifest is rewritten so it carries no unconditional multi-thread
Tokio runtime, which cannot compile for WebAssembly. Its Rust source is unchanged.
"""
import argparse
import copy
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tomllib


def check_browser_pins(root, url, rev, *, update_pins=False):
    """Validate both manifests before writing anything, unless updates are explicit."""
    updates = []
    errors = []
    for relative in ('web/Cargo.toml', 'web/compat/Cargo.toml'):
        manifest = root / relative
        text = manifest.read_text()
        document = tomllib.loads(text)
        dependency = document.get('dependencies', {}).get('masonry')
        if isinstance(dependency, dict) and (
            dependency.get('git') == url and dependency.get('rev') == rev
        ):
            continue
        expected = f'masonry git = "{url}", rev = "{rev}"'
        if not update_pins:
            errors.append(f'{relative}: expected {expected}; found {dependency!r}')
            continue

        # Preserve comments and all unrelated manifest text.
        # TOML parsing, not the substitution, determines whether this is a valid update.
        updated, count = re.subn(
            r'^masonry = \{ git = "[^"]+", rev = "[0-9a-f]+"',
            lambda _: f'masonry = {{ git = "{url}", rev = "{rev}"',
            text,
            flags=re.MULTILINE,
        )
        expected_document = copy.deepcopy(document)
        if isinstance(dependency, dict):
            expected_document['dependencies']['masonry'].update(git=url, rev=rev)
        if count != 1 or tomllib.loads(updated) != expected_document:
            errors.append(
                f'{relative}: cannot safely update this dependency layout; '
                f'set [dependencies] {expected} manually'
            )
            continue
        updates.append((manifest, updated))

    if errors:
        hint = (
            'No browser manifests were changed. After intentionally changing the desktop pin, '
            'run `python3 web/prepare.py --update-pins`, then refresh web/Cargo.lock '
            'as described in web/README.md.'
        )
        raise ValueError('\n'.join(errors + [hint]))
    for manifest, updated in updates:
        manifest.write_text(updated)
        print(f'Pinned {manifest.relative_to(root)} to {url}@{rev[:8]}', file=sys.stderr)


def prepare(root, *, update_pins=False):
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=root
    ))
    pkg = next(p for p in metadata['packages'] if p['name'] == 'xilem_masonry')
    match = re.fullmatch(
        r'git\+(?P<url>[^?]+)\?rev=(?P<rev>[0-9a-f]+)#(?P<commit>[0-9a-f]+)',
        pkg['source'] or '',
    )
    if not match:
        raise ValueError(
            f"xilem_masonry must come from a pinned git revision, not {pkg['source']!r}"
        )
    url, rev = match['url'], match['rev']
    check_browser_pins(root, url, rev, update_pins=update_pins)
    source = pathlib.Path(pkg['manifest_path']).parent
    workspace = tomllib.loads((source.parent / 'Cargo.toml').read_text())
    version = (
        workspace['package']['version'] if 'package' in workspace
        else workspace['workspace']['package']['version']
    )
    deps = workspace['workspace']['dependencies']

    def plain(name):
        dep = deps[name]
        return dep if isinstance(dep, str) else dep['version']

    dest = root / 'web/target/xilem_masonry'
    shutil.copytree(source, dest, dirs_exist_ok=True)
    # Upstream source is unchanged. Only its unconditional desktop runtime feature is removed.
    (dest / 'Cargo.toml').write_text(f'''[package]
name = "xilem_masonry"
version = "{version}"
edition = "2024"
license = "Apache-2.0"
publish = false
[features]
default = []
[dependencies]
xilem_core = {{ git = "{url}", rev = "{rev}" }}
masonry = {{ git = "{url}", rev = "{rev}", default-features = false }}
smallvec = "{plain('smallvec')}"
tracing = "{plain('tracing')}"
tokio = {{ version = "1.50", features = ["rt", "time", "sync"] }}
usvg = "{plain('usvg')}"
''')
    print(
        f'Prepared pinned Xilem widget adapter {version} from {url}@{rev[:8]} '
        'with a single-thread browser runtime'
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--update-pins',
        action='store_true',
        help='explicitly synchronize tracked browser manifests with the desktop Xilem pin',
    )
    args = parser.parse_args()
    try:
        prepare(pathlib.Path(__file__).resolve().parent.parent, update_pins=args.update_pins)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'prepare.py: {error}\n')


if __name__ == '__main__':
    main()
