#!/usr/bin/env python3
"""Package one Simi CLI archive and matching self-contained VSIX."""

from __future__ import annotations

import gzip
import hashlib
import shutil
import sys
import tarfile
import zipfile
from pathlib import Path


def archive_mode(archived_name: str) -> int:
    """Return the stable Unix mode retained by an archive entry."""
    return 0o755 if archived_name in {"simi", "simi.exe"} else 0o644


def write_tar_archive(
    archive: Path, bundled_files: tuple[tuple[Path, str], ...]
) -> None:
    """Write a reproducible gzip-compressed tar archive."""
    with archive.open("wb") as archive_file:
        with gzip.GzipFile(
            filename="", mode="wb", fileobj=archive_file, mtime=0
        ) as compressed:
            with tarfile.open(fileobj=compressed, mode="w") as bundle:
                for path, archived_name in bundled_files:
                    info = bundle.gettarinfo(str(path), arcname=archived_name)
                    info.uid = 0
                    info.gid = 0
                    info.uname = ""
                    info.gname = ""
                    info.mtime = 0
                    info.mode = archive_mode(archived_name)
                    with path.open("rb") as source_file:
                        bundle.addfile(info, source_file)


def write_zip_archive(
    archive: Path, bundled_files: tuple[tuple[Path, str], ...]
) -> None:
    """Write a reproducible ZIP archive."""
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for path, archived_name in bundled_files:
            info = zipfile.ZipInfo(archived_name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = archive_mode(archived_name) << 16
            bundle.writestr(info, path.read_bytes())


def write_checksum(artifact: Path) -> Path:
    """Write and return the SHA-256 checksum sidecar for *artifact*."""
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    checksum = artifact.with_name(f"{artifact.name}.sha256")
    checksum.write_text(f"{digest}  {artifact.name}\n", encoding="utf-8")
    return checksum


def main() -> None:
    if len(sys.argv) != 5:
        raise SystemExit("usage: package-release.py TARGET PLATFORM SHA VSCODE_VSIX")

    target, platform, sha, vsix_argument = sys.argv[1:]
    if len(sha) != 40 or any(character not in "0123456789abcdef" for character in sha):
        raise SystemExit("SHA must be a full lowercase 40-character Git commit hash")

    if platform not in {"linux", "macos", "windows"}:
        raise SystemExit(f"unsupported platform: {platform}")

    executable = "simi.exe" if platform == "windows" else "simi"
    source = Path("target") / target / "release" / executable
    vsix = Path(vsix_argument)
    readme = Path("release/README.md")
    license_file = Path("LICENSE")
    for required in (source, vsix, readme, license_file):
        if not required.is_file():
            raise SystemExit(f"release input does not exist: {required}")

    bundled_files = (
        (source, executable),
        (readme, "README.md"),
        (license_file, "LICENSE"),
    )

    output = Path("dist")
    output.mkdir(exist_ok=True)
    stem = f"simi-{sha}-{target}"

    if platform == "windows":
        archive = output / f"{stem}.zip"
        write_zip_archive(archive, bundled_files)
    else:
        archive = output / f"{stem}.tar.gz"
        write_tar_archive(archive, bundled_files)

    vsix_output = output / f"simi-vscode-{sha}-{target}.vsix"
    shutil.copyfile(vsix, vsix_output)

    for artifact in (archive, vsix_output):
        checksum = write_checksum(artifact)
        print(artifact)
        print(checksum)


if __name__ == "__main__":
    main()
