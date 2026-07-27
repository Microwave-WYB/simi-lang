"""Offline fixtures for deterministic release asset packaging."""

from __future__ import annotations

import hashlib
import importlib.util
import os
import tarfile
import tempfile
import unittest
import zipfile
from contextlib import contextmanager
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SHA = "a" * 40
TARGETS = {
    "linux": "x86_64-unknown-linux-gnu",
    "windows": "x86_64-pc-windows-msvc",
}


@contextmanager
def working_directory(path: Path):
    previous = Path.cwd()
    os.chdir(path)
    try:
        yield
    finally:
        os.chdir(previous)


def load_packager():
    spec = importlib.util.spec_from_file_location(
        "package_release", ROOT / "scripts/package-release.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGER = load_packager()


class PackageReleaseTests(unittest.TestCase):
    def create_inputs(self, root: Path, platform: str) -> tuple[Path, Path]:
        target = TARGETS[platform]
        executable = "simi.exe" if platform == "windows" else "simi"
        binary = root / "target" / target / "release" / executable
        binary.parent.mkdir(parents=True)
        binary.write_bytes(b"Simi executable fixture\n")
        binary.chmod(0o755)

        vsix = root / "release-input" / "simi-vscode.vsix"
        vsix.parent.mkdir()
        vsix.write_bytes(b"VSIX fixture\n")

        readme = root / "release" / "README.md"
        readme.parent.mkdir()
        readme.write_text("Release README fixture\n", encoding="utf-8")
        (root / "LICENSE").write_text("License fixture\n", encoding="utf-8")
        return binary, vsix

    def package(self, root: Path, platform: str, vsix: Path) -> None:
        with working_directory(root), patch.object(
            PACKAGER.sys,
            "argv",
            [
                "package-release.py",
                TARGETS[platform],
                platform,
                SHA,
                str(vsix.relative_to(root)),
            ],
        ):
            PACKAGER.main()

    def assert_checksum(self, artifact: Path) -> None:
        checksum = artifact.with_name(f"{artifact.name}.sha256")
        self.assertEqual(
            checksum.read_text(encoding="utf-8"),
            f"{hashlib.sha256(artifact.read_bytes()).hexdigest()}  {artifact.name}\n",
        )

    def test_packages_named_assets_with_contents_and_checksums(self) -> None:
        for platform, target in TARGETS.items():
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                binary, vsix = self.create_inputs(root, platform)
                self.package(root, platform, vsix)

                extension = ".zip" if platform == "windows" else ".tar.gz"
                archive = root / "dist" / f"simi-{SHA}-{target}{extension}"
                extension_asset = root / "dist" / f"simi-vscode-{SHA}-{target}.vsix"
                self.assertEqual(
                    sorted(path.name for path in (root / "dist").iterdir()),
                    sorted(
                        (
                            archive.name,
                            f"{archive.name}.sha256",
                            extension_asset.name,
                            f"{extension_asset.name}.sha256",
                        )
                    ),
                )
                self.assertTrue(archive.is_file())
                self.assertEqual(extension_asset.read_bytes(), vsix.read_bytes())
                self.assert_checksum(archive)
                self.assert_checksum(extension_asset)

                executable = "simi.exe" if platform == "windows" else "simi"
                expected_contents = {
                    executable: binary.read_bytes(),
                    "README.md": b"Release README fixture\n",
                    "LICENSE": b"License fixture\n",
                }
                if platform == "windows":
                    with zipfile.ZipFile(archive) as bundle:
                        self.assertEqual(bundle.namelist(), list(expected_contents))
                        actual_contents = {
                            name: bundle.read(name) for name in bundle.namelist()
                        }
                else:
                    with tarfile.open(archive, "r:gz") as bundle:
                        self.assertEqual(bundle.getnames(), list(expected_contents))
                        actual_contents = {
                            name: bundle.extractfile(name).read()
                            for name in bundle.getnames()
                        }
                self.assertEqual(actual_contents, expected_contents)

    def test_archive_bytes_do_not_depend_on_source_timestamps(self) -> None:
        for platform, target in TARGETS.items():
            with self.subTest(platform=platform), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                binary, vsix = self.create_inputs(root, platform)
                self.package(root, platform, vsix)
                extension = ".zip" if platform == "windows" else ".tar.gz"
                archive = root / "dist" / f"simi-{SHA}-{target}{extension}"
                original_archive = archive.read_bytes()

                for source in (binary, vsix, root / "release/README.md", root / "LICENSE"):
                    os.utime(source, (1_700_000_000, 1_700_000_000))
                self.package(root, platform, vsix)

                self.assertEqual(archive.read_bytes(), original_archive)
                self.assert_checksum(archive)


if __name__ == "__main__":
    unittest.main()
