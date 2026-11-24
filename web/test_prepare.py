# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Pin checks use temporary manifests and never invoke Cargo."""

import pathlib
import tempfile
import tomllib
import unittest

from prepare import check_browser_pins


URL = 'https://github.com/eliheuer/xilem'
REV = 'a' * 40
OLD_REV = 'b' * 40


def manifest(url=URL, rev=REV):
    return (
        '# Preserve this comment.\n'
        '[package]\nname = "fixture"\nversion = "0.1.0"\n'
        '[dependencies]\n'
        f'masonry = {{ git = "{url}", rev = "{rev}", default-features = false }}\n'
        'unrelated = "1"\n'
    )


class BrowserPinsTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = pathlib.Path(temporary.name)
        self.paths = [
            self.root / 'web/Cargo.toml',
            self.root / 'web/compat/Cargo.toml',
        ]
        for path in self.paths:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(manifest())

    def contents(self):
        return [path.read_bytes() for path in self.paths]

    def test_matching_pins_leave_manifests_untouched(self):
        before = self.contents()
        timestamps = [path.stat().st_mtime_ns for path in self.paths]
        check_browser_pins(self.root, URL, REV)
        self.assertEqual(self.contents(), before)
        self.assertEqual([path.stat().st_mtime_ns for path in self.paths], timestamps)

    def test_mismatched_revision_reports_both_manifests_without_writes(self):
        for path in self.paths:
            path.write_text(manifest(rev=OLD_REV))
        before = self.contents()
        with self.assertRaises(ValueError) as error:
            check_browser_pins(self.root, URL, REV)
        self.assertEqual(self.contents(), before)
        for expected in ('web/Cargo.toml', 'web/compat/Cargo.toml', REV, '--update-pins'):
            self.assertIn(expected, str(error.exception))

    def test_mismatched_url_is_rejected(self):
        self.paths[1].write_text(manifest(url='https://example.com/other'))
        before = self.contents()
        with self.assertRaisesRegex(ValueError, 'web/compat/Cargo.toml'):
            check_browser_pins(self.root, URL, REV)
        self.assertEqual(self.contents(), before)

    def test_missing_dependency_is_rejected(self):
        self.paths[1].write_text('[dependencies]\nunrelated = "1"\n')
        before = self.contents()
        with self.assertRaisesRegex(ValueError, 'found None'):
            check_browser_pins(self.root, URL, REV)
        self.assertEqual(self.contents(), before)

    def test_explicit_update_changes_only_pins(self):
        for path in self.paths:
            path.write_text(manifest(url='https://example.com/old', rev=OLD_REV))
        check_browser_pins(self.root, URL, REV, update_pins=True)
        for path in self.paths:
            self.assertEqual(path.read_text(), manifest())
            dependency = tomllib.loads(path.read_text())['dependencies']['masonry']
            self.assertEqual(dependency, {'git': URL, 'rev': REV, 'default-features': False})
        before = self.contents()
        timestamps = [path.stat().st_mtime_ns for path in self.paths]
        check_browser_pins(self.root, URL, REV, update_pins=True)
        self.assertEqual(self.contents(), before)
        self.assertEqual([path.stat().st_mtime_ns for path in self.paths], timestamps)

    def test_valid_alternative_layout_uses_toml_validation(self):
        self.paths[1].write_text(
            f"[dependencies.masonry]\nrev = '{REV}'\ngit = '{URL}'\n"
        )
        before = self.contents()
        check_browser_pins(self.root, URL, REV)
        self.assertEqual(self.contents(), before)

    def test_unsupported_update_layout_does_not_partially_write(self):
        self.paths[0].write_text(manifest(rev=OLD_REV))
        self.paths[1].write_text(
            f"[dependencies.masonry]\nrev = '{OLD_REV}'\ngit = '{URL}'\n"
        )
        before = self.contents()
        with self.assertRaisesRegex(ValueError, 'set .* manually'):
            check_browser_pins(self.root, URL, REV, update_pins=True)
        self.assertEqual(self.contents(), before)


if __name__ == '__main__':
    unittest.main()
