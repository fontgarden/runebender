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

    def test_release_checks_follow_the_jobs_for_the_candidate(self):
        # Names from the GitHub response for dfa8ace, including the two matrix jobs.
        names = ["current stable / Linux compatibility", "formatting and repository hygiene",
                 "browser editor", "minimum Rust / macos-latest", "minimum Rust / ubuntu-latest",
                 "security advisories"]
        run = {"id": 42, "head_sha": "chosen", "event": "push",
               "status": "completed", "conclusion": "success"}
        runs = [{"workflow_runs": [dict(run, id=43, event="schedule")]},
                {"workflow_runs": [run]}]
        jobs = [{"name": name, "conclusion": "success"} for name in names]
        with patch.object(release, "run", side_effect=[
                json.dumps(runs), json.dumps([{"jobs": jobs[:2]}, {"jobs": jobs[2:]}])]) as api:
            release.required_checks("chosen")
            self.assertIn("/runs/42/jobs?", api.call_args.args[-1])
        for conclusion in ("failure", "skipped", None):
            with self.subTest(conclusion=conclusion):
                jobs[2]["conclusion"] = conclusion
                with patch.object(release, "run", side_effect=[
                        json.dumps(runs), json.dumps([{"jobs": jobs}])]):
                    with self.assertRaisesRegex(ValueError, "browser editor"):
                        release.required_checks("chosen")

    def test_an_old_success_cannot_hide_a_failed_or_pending_run(self):
        earlier = {"id": 1, "head_sha": "chosen", "event": "push",
                   "status": "completed", "conclusion": "success"}
        for status, conclusion in (("in_progress", None), ("completed", "failure")):
            newer = dict(earlier, id=2, status=status, conclusion=conclusion)
            with patch.object(release, "run", return_value=json.dumps([
                    {"workflow_runs": [newer, earlier]}])):
                with self.assertRaisesRegex(ValueError, "successful completed CI"):
                    release.required_checks("chosen")

    def test_ci_for_another_commit_does_not_count(self):
        runs = [{"id": 1, "head_sha": "different", "event": "push", "conclusion": "success"}]
        with patch.object(release, "run", return_value=json.dumps([{"workflow_runs": runs}])):
            with self.assertRaisesRegex(ValueError, "successful completed CI"):
                release.required_checks("chosen")


if __name__ == "__main__":
    unittest.main()
