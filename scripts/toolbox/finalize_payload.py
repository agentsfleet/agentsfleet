"""Independent realized-file expectations captured before package removal."""

from dataclasses import dataclass
import os
from pathlib import Path
import stat

from package_records import info_identity
from scan_inventory import file_sha256


@dataclass(frozen=True)
class FilePayload:
    mode: int
    sha256: str


@dataclass(frozen=True)
class LinkPayload:
    mode: int
    target: str
    resolved_sha256: str | None


@dataclass(frozen=True)
class DirectoryPayload:
    mode: int


@dataclass(frozen=True)
class Notice:
    contents: bytes
    mode: int
    sha256: str


def payload(path, include_directory=False):
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return None
    permissions = stat.S_IMODE(mode)
    if stat.S_ISLNK(mode):
        digest = file_sha256(path) if path.is_file() else None
        return LinkPayload(permissions, os.readlink(path), digest)
    if stat.S_ISREG(mode):
        return FilePayload(permissions, file_sha256(path))
    if include_directory and stat.S_ISDIR(mode):
        return DirectoryPayload(permissions)
    return None


def owned_paths(directory, identities, retained):
    kept, removed = set(), set()
    for listing in sorted(directory.glob("*.list")):
        owner = info_identity(listing.name, identities)
        destination = kept if owner.key in retained else removed
        for name in listing.read_text(encoding="utf-8").splitlines():
            path = Path(name)
            if not path.is_absolute() or ".." in path.parts or "\0" in name:
                raise ValueError(f"invalid installed file path: {name!r}")
            destination.add(path)
    return kept, removed - kept


def snapshot(paths):
    result = {}
    for path in sorted(paths):
        realized = payload(path)
        if realized is not None:
            result[path] = realized
    return result


def capture_notices(identities):
    notices = {}
    for identity in identities.values():
        path = Path(identity.notice_path)
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"missing or empty retained copyright: {path}")
        notices[path] = Notice(
            path.read_bytes(), stat.S_IMODE(path.stat().st_mode), file_sha256(path)
        )
    return notices


def preserve_notices(notices):
    exceptions = {}
    for path, notice in notices.items():
        if not path.is_file():
            if path.parent.is_symlink():
                path.parent.unlink()
                path.parent.mkdir(mode=0o755)
                path.parent.chmod(0o755)
                exceptions[path.parent] = DirectoryPayload(0o755)
            elif not path.parent.exists():
                path.parent.mkdir(mode=0o755)
                path.parent.chmod(0o755)
            if path.is_symlink():
                path.unlink()
            with path.open("xb") as handle:
                handle.write(notice.contents)
            path.chmod(notice.mode)
            exceptions[path] = FilePayload(notice.mode, notice.sha256)
        if not path.is_file() or file_sha256(path) != notice.sha256:
            raise ValueError(f"retained copyright changed: {path}")
    return exceptions


def reconcile(expected, exceptions):
    for path, original in expected.items():
        wanted = exceptions.get(path, original)
        actual = payload(path, include_directory=isinstance(wanted, DirectoryPayload))
        if actual != wanted:
            raise ValueError(f"retained payload changed: {path}: {wanted!r} -> {actual!r}")
    for path, wanted in exceptions.items():
        actual = payload(path, include_directory=isinstance(wanted, DirectoryPayload))
        if actual != wanted:
            raise ValueError(f"preserved notice changed: {path}")


def require_removed(expected):
    for path in expected:
        if path.exists() or path.is_symlink():
            raise ValueError(f"removed package payload remains: {path}")
