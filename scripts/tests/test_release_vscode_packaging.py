"""Offline contract checks for bundled VS Code release packaging."""

from __future__ import annotations

import json
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "editors/vscode/package.json"
VSCODE_JUSTFILE = ROOT / "editors/vscode/justfile"
ROOT_JUSTFILE = ROOT / "justfile"


class ReleaseVscodePackagingTests(unittest.TestCase):
    def test_release_only_path_validates_committed_parser_without_regeneration(self) -> None:
        manifest = json.loads(PACKAGE.read_text(encoding="utf-8"))
        scripts = manifest["scripts"]
        vscode_justfile = VSCODE_JUSTFILE.read_text(encoding="utf-8")
        root_justfile = ROOT_JUSTFILE.read_text(encoding="utf-8")

        self.assertEqual(scripts["prepackage"], "npm test")
        self.assertIn("check:generated-parser", scripts["test"])
        self.assertIn("scripts/check-generated-parser.mjs", scripts["check:generated-parser"])
        self.assertEqual(
            scripts["package:release"],
            "npm run check:release-parser && vsce package",
        )
        self.assertNotIn("generate", scripts["check:release-parser"])
        self.assertIn("npm run package:release", vscode_justfile)
        self.assertIn("just editors vscode package-bundled", root_justfile)

    def test_release_mode_does_not_broadly_disable_validation(self) -> None:
        manifest = json.loads(PACKAGE.read_text(encoding="utf-8"))
        scripts = manifest["scripts"]
        vscode_justfile = VSCODE_JUSTFILE.read_text(encoding="utf-8")

        self.assertEqual(scripts["package"], "vsce package")
        self.assertIn('cd "{{component}}" && npm run package\n', vscode_justfile)
        for source in (*scripts.values(), vscode_justfile):
            self.assertNotIn("--ignore-scripts", source)
            self.assertNotIn("npm_config_ignore_scripts", source)


if __name__ == "__main__":
    unittest.main()
