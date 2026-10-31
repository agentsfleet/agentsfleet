"""Package verified binaries and toolbox without rebuilding their contents."""

from pathlib import Path
import tarfile

from .model import BINARIES, toolbox_digest


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
    with tarfile.open(archives / "agentsfleet-runner-bundle-linux-amd64.tar.gz", "w:gz") as handle:
        for path in files:
            handle.add(path, arcname=path.name, recursive=False)
