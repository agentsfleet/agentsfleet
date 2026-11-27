"""Package verified binaries, toolbox and guest set without rebuilding their contents."""

from pathlib import Path
import tarfile

from .model import BINARIES, GUEST_ARTIFACTS, GUEST_PROGRAMS, toolbox_digest


def package(root: Path) -> None:
    archives = root / "archives"
    archives.mkdir(exist_ok=True)
    for name in BINARIES:
        binary = root / "bin" / name
        binary.chmod(0o755)
        with tarfile.open(archives / f"{name}.tar.gz", "w:gz") as handle:
            handle.add(binary, arcname=name, recursive=False)
    digest = toolbox_digest(root)
    files = [root / "bin" / "agentsfleet-runner-linux-amd64"]
    files += [root / "toolbox" / f"toolbox-{digest}.{suffix}" for suffix in (
        "erofs", "json", "json.sig", "sbom.spdx.json", "scan.json",
    )]
    files.append(root / "toolbox" / "release.pub")
    # An artifact upload drops the executable bit, as it does for the binaries.
    for name in GUEST_PROGRAMS:
        (root / "toolbox" / name).chmod(0o755)
    files += [root / "toolbox" / name for name in GUEST_ARTIFACTS + ("guest.json", "guest.json.sig")]
    with tarfile.open(archives / "agentsfleet-runner-bundle-linux-amd64.tar.gz", "w:gz") as handle:
        for path in files:
            handle.add(path, arcname=path.name, recursive=False)
