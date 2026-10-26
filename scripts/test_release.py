#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT

"""Release gates must fail before an unsigned or changed artifact can be published."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import release


class ReleaseGateTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.directory = Path(self.scratch.name)
        (self.directory / "candidate.dmg").write_bytes(b"candidate")
        self.receipt = {
            "artifact": "candidate.dmg", "artifact_sha256": hashlib.sha256(b"candidate").hexdigest(),
            "source_dirty": True, "signing": "local", "updates": {"enabled": True},
        }
        self.write_receipt()

    def write_receipt(self):
        (self.directory / "build-info.json").write_text(json.dumps(self.receipt))

    def test_local_build_is_not_a_public_candidate(self):
        with patch.object(release, "run") as command:
            with self.assertRaisesRegex(ValueError, "clean-source, notarized"):
                release.artifact(self.directory)
            command.assert_not_called()

    def test_clean_looking_ad_hoc_build_still_cannot_pass(self):
        self.receipt["source_dirty"] = False
        self.write_receipt()
        with self.assertRaisesRegex(ValueError, "notarized"):
            release.artifact(self.directory)

    def test_changed_archive_is_rejected_even_in_local_test_mode(self):
        (self.directory / "candidate.dmg").write_bytes(b"different bytes")
        with self.assertRaisesRegex(ValueError, "checksum"):
            release.artifact(self.directory, local_test=True)

    def test_explicit_local_testing_does_not_claim_notarization(self):
        receipt, _ = release.artifact(self.directory, local_test=True)
        self.assertEqual(receipt["signing"], "local")
        self.assertTrue(receipt["source_dirty"])

    def test_missing_updater_is_not_a_release_candidate(self):
        self.receipt["updates"]["enabled"] = False
        self.write_receipt()
        with self.assertRaisesRegex(ValueError, "does not include the updater"):
            release.artifact(self.directory, local_test=True)

    def test_new_failed_ci_run_cannot_be_hidden_by_an_older_success(self):
        names = ["formatting and repository hygiene", "ubuntu-latest", "macos-latest", "security advisories"]
        checks = [{"id": i, "name": name, "head_sha": "chosen", "conclusion": "success"}
                  for i, name in enumerate(names)]
        checks.append({"id": 100, "name": "security advisories", "head_sha": "chosen", "conclusion": "failure"})
        with patch.object(release, "run", return_value=json.dumps({"check_runs": checks})):
            with self.assertRaisesRegex(ValueError, "security advisories"):
                release.required_checks("chosen")

    def test_ci_for_another_commit_does_not_count(self):
        checks = [{"id": 1, "name": "macos-latest", "head_sha": "different", "conclusion": "success"}]
        with patch.object(release, "run", return_value=json.dumps({"check_runs": checks})):
            with self.assertRaisesRegex(ValueError, "macos-latest"):
                release.required_checks("chosen")


if __name__ == "__main__":
    unittest.main()
