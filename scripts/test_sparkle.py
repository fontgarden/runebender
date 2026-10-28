#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Vendor extraction publishes only a complete, signature-verified framework."""

import hashlib
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import sparkle


class SparkleCacheTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.cache = self.root / "target/vendor/Sparkle-test"
        self.cache.mkdir(parents=True)
        self.archive = self.cache / "Sparkle-test.tar.xz"
        with tarfile.open(self.archive, "w:xz") as archive:
            data = b"fixture"
            item = tarfile.TarInfo("Sparkle.framework/Versions/B/Sparkle")
            item.size = len(data)
            archive.addfile(item, io.BytesIO(data))
        config = {"sparkle": {"version": "test", "sha256": hashlib.sha256(self.archive.read_bytes()).hexdigest()}}
        root_patch = patch.object(sparkle, "ROOT", self.root)
        config_patch = patch.object(sparkle, "configuration", return_value=config)
        root_patch.start()
        config_patch.start()
        self.addCleanup(root_patch.stop)
        self.addCleanup(config_patch.stop)

    def test_failed_signature_never_publishes_extraction(self):
        with patch.object(sparkle.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "codesign")):
            with self.assertRaises(subprocess.CalledProcessError):
                sparkle.framework()
        self.assertFalse((self.cache / "distribution").exists())
        self.assertEqual(list(self.cache.glob("extract-*")), [])

    def test_invalid_cache_is_preserved_and_rebuilt_from_verified_archive(self):
        stale = self.cache / "distribution"
        stale.mkdir()
        (stale / "partial").write_text("interrupted")
        replies = [subprocess.CompletedProcess([], 1, stderr="invalid bundle"), subprocess.CompletedProcess([], 0)]
        with patch.object(sparkle.subprocess, "run", side_effect=replies) as verify:
            path = sparkle.framework()
        self.assertTrue((path / "Sparkle.framework/Versions/B/Sparkle").is_file())
        self.assertEqual(verify.call_count, 2)
        self.assertEqual(len(list(self.cache.glob("distribution-invalid-*/partial"))), 1)

    def test_corrupt_archive_cannot_be_used_to_repair_cache(self):
        self.archive.write_bytes(b"changed")
        with patch.object(sparkle.subprocess, "run") as verify:
            with self.assertRaisesRegex(RuntimeError, "Modified Sparkle archive"):
                sparkle.framework()
        verify.assert_not_called()


if __name__ == "__main__":
    unittest.main()
