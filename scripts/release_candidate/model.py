"""Strict receipts and payload verification at the artifact boundary."""

from dataclasses import asdict, dataclass
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
from typing import Literal


BINARIES = (
    "agentsfleetd-rs-linux-amd64", "agentsfleetd-rs-linux-arm64",
    "agentsfleetd-darwin-arm64", "agentsfleet-runner-linux-amd64",
)
GATES = ("fly", "metal", "dashboard", "verify", "qa", "acceptance", "cli")
IMAGE_REPOSITORY = "ghcr.io/agentsfleet/agentsfleetd"


def record(value: object, fields: set[str] | None = None) -> dict:
    if not isinstance(value, dict) or not all(isinstance(k, str) for k in value):
        raise ValueError("expected a JSON object")
    if fields is not None and value.keys() != fields:
        raise ValueError(f"unexpected receipt fields: {value.keys() ^ fields}")
    return value


def string(value: object, pattern: str) -> str:
    if not isinstance(value, str) or re.fullmatch(pattern, value) is None:
        raise ValueError(f"invalid receipt value: {value!r}")
    return value


def positive(value: object) -> int:
    if type(value) is not int or value < 1:
        raise ValueError("expected a positive integer")
    return value


def safe_path(value: object) -> str:
    name = string(value, r"[a-zA-Z0-9_./-]+")
    path = PurePosixPath(name)
    if path.is_absolute() or ".." in path.parts or str(path) != name:
        raise ValueError(f"unsafe payload path: {name}")
    return name


def load(path: Path) -> object:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def save(path: Path, value: object) -> None:
    with path.open("w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")


def sha256(path: Path) -> str:
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


@dataclass(frozen=True)
class File:
    sha256: str
    size: int

    @classmethod
    def parse(cls, raw: object) -> "File":
        data = record(raw, {"sha256", "size"})
        return cls(string(data["sha256"], r"[0-9a-f]{64}"), positive(data["size"]))


@dataclass(frozen=True)
class Source:
    repository: str
    version: str
    source_sha: str
    source_tree: str
    run_head_sha: str
    run_id: int
    run_attempt: int
    profile: str

    @classmethod
    def parse(cls, raw: object) -> "Source":
        data = record(raw, set(cls.__dataclass_fields__))
        return cls(
            string(data["repository"], r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+"),
            string(data["version"], r"[0-9]+\.[0-9]+\.[0-9]+"),
            string(data["source_sha"], r"[0-9a-f]{40}"),
            string(data["source_tree"], r"[0-9a-f]{40}"),
            string(data["run_head_sha"], r"[0-9a-f]{40}"),
            positive(data["run_id"]), positive(data["run_attempt"]),
            string(data["profile"], r"dist"),
        )


@dataclass(frozen=True)
class Candidate:
    source: Source
    files: dict[str, File]
    schema: int = 1

    @classmethod
    def read(cls, root: Path) -> "Candidate":
        data = record(load(root / "candidate.json"), {"schema", "source", "files"})
        if type(data["schema"]) is not int or data["schema"] != 1:
            raise ValueError("unsupported candidate schema")
        files = {safe_path(k): File.parse(v) for k, v in record(data["files"]).items()}
        return cls(Source.parse(data["source"]), files)


def inventory(root: Path, exclude: tuple[str, ...] = ()) -> dict[str, File]:
    if root.is_symlink() or not root.is_dir():
        raise ValueError("payload root must be a real directory")
    files = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError(f"payload contains a link or special file: {path}")
        name = safe_path(path.relative_to(root).as_posix())
        if path.is_file() and name not in exclude:
            files[name] = File(sha256(path), positive(path.stat().st_size))
    return files


def toolbox_digest(root: Path) -> str:
    images = list((root / "toolbox").glob("toolbox-*.erofs"))
    if len(images) != 1:
        raise ValueError("candidate must have exactly one toolbox image")
    digest = string(images[0].stem.removeprefix("toolbox-"), r"[0-9a-f]{64}")
    if sha256(images[0]) != digest or images[0].stat().st_size > 262848512:
        raise ValueError("toolbox digest or size gate failed")
    return digest


def candidate_names(digest: str) -> set[str]:
    return {f"bin/{name}" for name in BINARIES} | {
        "images/daemon-linux-amd64.tar", "images/daemon-linux-arm64.tar",
    } | {f"toolbox/toolbox-{digest}.{suffix}" for suffix in (
        "erofs", "json", "sbom.spdx.json", "scan.json",
    )}


def promoted_names(digest: str) -> set[str]:
    return candidate_names(digest) | {
        "candidate.json", "promotion.scan.json", "images.json",
        f"toolbox/toolbox-{digest}.json.sig", "toolbox/release.pub",
        "archives/agentsfleet-runner-bundle-linux-amd64.tar.gz",
    } | {f"archives/{name}.tar.gz" for name in BINARIES}


def verify_candidate(root: Path, tree: str, version: str,
                     stage: Literal["unsigned", "signed", "published"] = "unsigned") -> Candidate:
    excluded = ("candidate.json", "development.json") if stage == "published" else ("candidate.json",)
    actual = inventory(root, excluded)
    candidate = Candidate.read(root)
    if candidate.source.source_tree != tree or candidate.source.version != version:
        raise ValueError("candidate source tree or version does not match checkout")
    digest = toolbox_digest(root)
    if candidate.files.keys() != candidate_names(digest):
        raise ValueError("candidate is missing a required platform or has unexpected files")
    expected = candidate.files.keys() if stage == "unsigned" else promoted_names(digest) - {"candidate.json"}
    if stage == "signed":
        expected = expected - {"images.json"}
    if actual.keys() != expected:
        raise ValueError("payload file set differs from expected files")
    if any(actual[name] != entry for name, entry in candidate.files.items()):
        raise ValueError("candidate payload checksum or size mismatch")
    manifest = record(load(root / "toolbox" / f"toolbox-{digest}.json"))
    if (manifest.get("sha256") != digest or manifest.get("arch") != "amd64"
            or manifest.get("length") != actual[f"toolbox/toolbox-{digest}.erofs"].size
            or manifest.get("runner_versions") != [version]):
        raise ValueError("toolbox manifest identity does not match candidate")
    return candidate


def create(root: Path, source: Source) -> None:
    candidate = Candidate(source, inventory(root, ("candidate.json",)))
    save(root / "candidate.json", asdict(candidate))
    verify_candidate(root, source.source_tree, source.version)


@dataclass(frozen=True)
class Development:
    repository: str
    source_tree: str
    deployment_sha: str
    run_id: int
    run_attempt: int
    image: str
    files: dict[str, File]
    gates: dict[str, str]
    schema: int = 1

    @classmethod
    def read(cls, root: Path) -> "Development":
        data = record(load(root / "development.json"), set(cls.__dataclass_fields__))
        if (type(data["schema"]) is not int or data["schema"] != 1
                or record(data["gates"], set(GATES)) != dict.fromkeys(GATES, "success")):
            raise ValueError("candidate lacks passing development gates")
        return cls(
            string(data["repository"], r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+"),
            string(data["source_tree"], r"[0-9a-f]{40}"),
            string(data["deployment_sha"], r"[0-9a-f]{40}"),
            positive(data["run_id"]), positive(data["run_attempt"]),
            string(data["image"], re.escape(IMAGE_REPOSITORY) + r"@sha256:[0-9a-f]{64}"),
            {safe_path(k): File.parse(v) for k, v in record(data["files"]).items()},
            data["gates"],
        )


def verify_development(root: Path, tree: str, version: str) -> Development:
    verify_candidate(root, tree, version, stage="published")
    receipt = Development.read(root)
    if receipt.source_tree != tree or receipt.files != inventory(root, ("development.json",)):
        raise ValueError("development candidate source or payload checksum mismatch")
    image = record(load(root / "images.json"), {"image"})
    if image["image"] != receipt.image:
        raise ValueError("runtime image differs from the development-verified image")
    return receipt
