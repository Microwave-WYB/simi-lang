#!/usr/bin/env python3
"""Package one Simi CLI archive and matching self-contained VSIX."""

from __future__ import annotations

import hashlib
import shutil
import sys
import tarfile
import zipfile
from pathlib import Path


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
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            for path, archived_name in bundled_files:
                bundle.write(path, archived_name)
    else:
        archive = output / f"{stem}.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            for path, archived_name in bundled_files:
                bundle.add(path, arcname=archived_name)

    vsix_output = output / f"simi-vscode-{sha}-{target}.vsix"
    shutil.copyfile(vsix, vsix_output)

    for artifact in (archive, vsix_output):
        digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
        checksum = artifact.with_name(f"{artifact.name}.sha256")
        checksum.write_text(f"{digest}  {artifact.name}\n", encoding="utf-8")
        print(artifact)
        print(checksum)


if __name__ == "__main__":
    main()
