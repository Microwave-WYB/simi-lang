#!/usr/bin/env python3
"""Package one Simi binary for an immutable Git-hash release."""

from __future__ import annotations

import hashlib
import sys
import tarfile
import zipfile
from pathlib import Path


def main() -> None:
    if len(sys.argv) != 4:
        raise SystemExit("usage: package-release.py TARGET PLATFORM SHA")

    target, platform, sha = sys.argv[1:]
    if len(sha) != 40 or any(character not in "0123456789abcdef" for character in sha):
        raise SystemExit("SHA must be a full lowercase 40-character Git commit hash")

    executable = "simi.exe" if platform == "windows" else "simi"
    source = Path("target") / target / "release" / executable
    if not source.is_file():
        raise SystemExit(f"built binary does not exist: {source}")

    output = Path("dist")
    output.mkdir(exist_ok=True)
    stem = f"simi-{sha}-{target}"

    if platform == "windows":
        archive = output / f"{stem}.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            bundle.write(source, executable)
    else:
        archive = output / f"{stem}.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            bundle.add(source, arcname=executable)

    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum = archive.with_name(f"{archive.name}.sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8")
    print(archive)
    print(checksum)


if __name__ == "__main__":
    main()
