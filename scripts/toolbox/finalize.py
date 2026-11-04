#!/usr/bin/env python3
"""Finalize only the disposable root identified by the builder's chroot hook."""

from dataclasses import dataclass
import os
from pathlib import Path
import shutil
import subprocess
import sys

from finalize_payload import (
    capture_notices, owned_paths, preserve_notices, reconcile, require_removed, snapshot,
)
from package_records import NAME, ReleasePackage, archive_packages, installed_packages
from scan_inventory import COMMITMENT_PATH, Inventory, canonical


APPROVED_PURGE = frozenset({
    "passwd", "apt", "debian-archive-keyring", "dpkg",
    "libapt-pkg7.0", "sqv", "perl", "libperl5.40",
})
DATABASE = Path("/var/lib/dpkg")
STATE_PATHS = (DATABASE, Path("/var/lib/apt"), Path("/var/cache/apt"))
IDENTITIES = tuple(Path("/etc") / name for name in ("passwd", "group", "shadow", "gshadow"))
VENDORS = (Path("/usr/local/bin/uv"), Path("/usr/local/bin/uvx"))
TOOLS = ("sh", "cat", "ls", "git", "python3", "node", "rg", "jq", "curl", "perl", "uv", "uvx")


@dataclass(frozen=True)
class RemovalPlan:
    installed: dict
    retained: dict
    archives: dict
    purge: tuple[str, ...]

    @classmethod
    def parse(cls, status, provenance, purge_text, required_text):
        purge, required = purge_text.split(), required_text.split()
        for names in (purge, required):
            if not names or len(names) != len(set(names)):
                raise ValueError("package inputs must be nonempty and unique")
            if any(NAME.fullmatch(name) is None for name in names):
                raise ValueError("package inputs need Debian package names")
        if set(purge) != APPROVED_PURGE:
            raise ValueError("purge must name exactly the approved eight packages")
        if set(purge) & set(required):
            raise ValueError("purge overlaps required runtime packages")
        installed = installed_packages(status)
        names = {package.identity.name for package in installed.values()}
        if not set(purge).issubset(names) or not set(required).issubset(names):
            raise ValueError("installed root is missing a removal or required package")
        archives = archive_packages(provenance)
        if ({key: package.identity for key, package in archives.items()}
                != {key: package.identity for key, package in installed.items()}):
            raise ValueError("captured archive provenance differs from installed packages")
        retained = {key: package for key, package in installed.items() if package.identity.name not in purge}
        order = tuple(sorted(set(purge) - {"dpkg"})) + ("dpkg",)
        return cls(installed, retained, archives, order)

    def verify_status(self, status):
        current = installed_packages(status)
        if current != self.retained:
            missing = sorted(set(self.retained) - set(current))
            extra = sorted(set(current) - set(self.retained))
            changed = sorted(key for key in set(current) & set(self.retained)
                             if current[key] != self.retained[key])
            raise ValueError(f"unexpected final installed set or source versions: "
                             f"missing={missing}, extra={extra}, changed={changed}")

    @property
    def installed_identities(self):
        return {key: package.identity for key, package in self.installed.items()}

    @property
    def retained_identities(self):
        return {key: package.identity for key, package in self.retained.items()}


def verify_disposable_root(expected):
    root = Path("/").stat()
    if os.geteuid() != 0 or expected != f"{root.st_dev}:{root.st_ino}":
        raise ValueError("finalizer must run inside the builder's identified disposable root")


def retained_inventory(plan, notices):
    status = (DATABASE / "status").read_text(encoding="utf-8")
    plan.verify_status(status)
    info = {}
    for path in sorted((DATABASE / "info").iterdir()):
        if path.suffix in {".list", ".md5sums"}:
            info[path.name] = path.read_text(encoding="utf-8")
    packages = [
        ReleasePackage(plan.archives[key], notices[Path(identity.notice_path)].sha256)
        for key, identity in plan.retained_identities.items()
    ]
    return Inventory.create(packages, status, info)


def verify_runtime():
    for name in TOOLS:
        if shutil.which(name) is None:
            raise ValueError(f"required runtime command is missing: {name}")
    subprocess.run(["python3", "-c", "import hashlib, json, pathlib, ssl"], check=True)
    subprocess.run(["perl", "-MGit", "-MJSON::PP", "-MIPC::Open2", "-e", "1"], check=True)


def finalize(expected_root, provenance, output, purge, required):
    verify_disposable_root(expected_root)
    status = (DATABASE / "status").read_text(encoding="utf-8")
    plan = RemovalPlan.parse(status, Path(provenance).read_text(encoding="utf-8"), purge, required)
    kept, removed = owned_paths(DATABASE / "info", plan.installed_identities, plan.retained)
    special = set(IDENTITIES + VENDORS)
    special.update(path for path in Path("/etc/alternatives").iterdir() if path.is_symlink())
    expected = snapshot(kept | special)
    if not special.issubset(expected):
        raise ValueError(f"missing required identities, vendors or alternatives: {sorted(special - set(expected))}")
    removed_payload = snapshot(removed)
    notices = capture_notices(plan.retained_identities)
    for group in (plan.purge[:-1], plan.purge[-1:]):
        subprocess.run(["dpkg", "--purge", "--force-depends", "--force-remove-essential", *group],
                       check=True, stdout=sys.stderr)
    exceptions = preserve_notices(notices)
    inventory = retained_inventory(plan, notices)
    reconcile(expected, exceptions)
    for path in STATE_PATHS:
        if path.exists():
            shutil.rmtree(path)
    reconcile(expected, exceptions)
    require_removed(removed_payload)
    verify_runtime()
    commitment = Path("/") / COMMITMENT_PATH
    with commitment.open("x", encoding="ascii") as handle:
        handle.write(inventory.commitment() + "\n")
    commitment.chmod(0o644)
    with Path(output).open("xb") as handle:
        handle.write(canonical(inventory.payload()))
    Path(output).chmod(0o644)


if __name__ == "__main__":
    if len(sys.argv) != 6:
        raise SystemExit("usage: finalize.py ROOT_ID PROVENANCE OUTPUT PURGE REQUIRED")
    finalize(*sys.argv[1:])
