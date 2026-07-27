"""Offline contract checks for the immutable versioned-release workflow."""

from __future__ import annotations

import re
import unittest
from pathlib import Path


WORKFLOW = (
    Path(__file__).resolve().parents[2] / ".github/workflows/release-versioned.yml"
)


class VersionedReleaseWorkflowTests(unittest.TestCase):
    def test_publishes_an_existing_versioned_tag_without_tag_mutation(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")

        self.assertRegex(workflow, r"tags:\s*\n\s*- 'v\[0-9\]\*'")
        self.assertIn(
            "description: Existing immutable prerelease tag to validate and publish",
            workflow,
        )
        self.assertIn('git ls-remote --tags origin "refs/tags/$TAG^{}"', workflow)
        self.assertIn("--verify-tag", workflow)
        self.assertIn("--prerelease", workflow)

        for forbidden in (
            r"\bgit\s+tag\b",
            r"\bgit\s+push\b",
            r"\bgh\s+release\s+delete\b",
            r"--cleanup-tag\b",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotRegex(workflow, forbidden)
        self.assertNotIn("/git/refs", workflow)


if __name__ == "__main__":
    unittest.main()
