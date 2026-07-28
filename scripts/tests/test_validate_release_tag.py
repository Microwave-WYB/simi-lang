"""Offline fixtures for immutable versioned release tags."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def load_validator():
    spec = importlib.util.spec_from_file_location(
        "validate_release_tag", ROOT / "scripts/validate-release-tag.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


VALIDATOR = load_validator()


class ValidateReleaseTagTests(unittest.TestCase):
    def test_accepts_supported_prerelease_channels(self) -> None:
        for tag in (
            "v0.1.0-alpha.1",
            "v1.2.3-beta.4",
            "v42.0.7-rc.9",
        ):
            with self.subTest(tag=tag):
                VALIDATOR.validate_tag(tag)

    def test_rejects_malformed_and_ruleset_glob_only_tags(self) -> None:
        for tag in (
            "v1-anything",
            "v1.2.3",
            "v01.2.3-alpha.1",
            "v1.02.3-beta.1",
            "v1.2.03-rc.1",
            "v1.2.3-preview.1",
            "v1.2.3-alpha.01",
            "v1.2.3-alpha.1-extra",
            "release-v1.2.3-alpha.1",
        ):
            with self.subTest(tag=tag):
                with self.assertRaises(ValueError):
                    VALIDATOR.validate_tag(tag)


if __name__ == "__main__":
    unittest.main()
