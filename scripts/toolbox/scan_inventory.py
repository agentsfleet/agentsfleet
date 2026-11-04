#!/usr/bin/env python3
"""Validate the image-bound release inventory before creating scanner records."""

import base64
import binascii
from dataclasses import dataclass
import gzip
import hashlib
import io
import json
from pathlib import Path
import sys
import zlib

from package_records import ReleasePackage, SHA256, info_identity, installed_packages


MANIFEST_LIMIT = 1_048_576
METADATA_LIMIT = 4_194_304
PACKAGE_LIMIT = 512
INFO_LIMIT = 1_024
COMMITMENT_PATH = "etc/toolbox-inventory.sha256"


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def read_json(path, limit=MANIFEST_LIMIT):
    with Path(path).open("rb") as handle:
        data = handle.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f"JSON exceeds {limit} bytes: {path}")
    return json.loads(data.decode("utf-8"), object_pairs_hook=unique_object)


def file_sha256(path):
    with Path(path).open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def encode_metadata(status, info):
    data = canonical({"status": status, "info": info})
    if len(data) > METADATA_LIMIT:
        raise ValueError("decoded package metadata exceeds its byte limit")
    with io.BytesIO() as buffer:
        with gzip.GzipFile(filename="", mode="wb", fileobj=buffer, mtime=0) as compressed:
            compressed.write(data)
        return base64.b64encode(buffer.getvalue()).decode("ascii")


def decode_metadata(encoded):
    if not isinstance(encoded, str) or len(encoded) > MANIFEST_LIMIT:
        raise ValueError("package_metadata must be bounded base64 text")
    try:
        compressed = base64.b64decode(encoded, validate=True)
        decoder = zlib.decompressobj(wbits=31)
        decoded = decoder.decompress(compressed, METADATA_LIMIT + 1)
    except (ValueError, binascii.Error, zlib.error) as error:
        raise ValueError("invalid compressed package metadata") from error
    if (len(decoded) > METADATA_LIMIT or not decoder.eof
            or decoder.unused_data or decoder.unconsumed_tail):
        raise ValueError("oversized, truncated or concatenated package metadata")
    value = json.loads(decoded.decode("utf-8"), object_pairs_hook=unique_object)
    if not isinstance(value, dict) or set(value) != {"status", "info"}:
        raise ValueError("metadata needs exactly status and info")
    return value


def validate_info(info, identities):
    if not isinstance(info, dict) or len(info) > INFO_LIMIT:
        raise ValueError("info must be a bounded object")
    entries = set()
    lists = set()
    for filename, contents in info.items():
        if not isinstance(contents, str) or "\0" in contents:
            raise ValueError(f"info must contain text without NUL: {filename}")
        identity = info_identity(filename, identities)
        suffix = filename.rsplit(".", 1)[1]
        entry = identity.key, suffix
        if entry in entries:
            raise ValueError(f"duplicate package info identity: {filename}")
        entries.add(entry)
        if suffix == "list":
            lists.add(identity.key)
    if lists != set(identities):
        raise ValueError("every retained package needs exactly one file list")
    return dict(info)


@dataclass(frozen=True)
class Inventory:
    packages: tuple[ReleasePackage, ...]
    status: str
    info: dict[str, str]
    encoded_metadata: str

    @classmethod
    def from_payload(cls, value):
        if not isinstance(value, dict):
            raise ValueError("release must be a JSON object")
        packages = value.get("packages")
        if not isinstance(packages, list) or not 0 < len(packages) <= PACKAGE_LIMIT:
            raise ValueError("packages must be a nonempty bounded array")
        packages = tuple(ReleasePackage.parse(package) for package in packages)
        expected = {}
        for package in packages:
            identity = package.archive.identity
            if identity.key in expected:
                raise ValueError(f"duplicate release package: {identity.key}")
            expected[identity.key] = identity
        encoded = value.get("package_metadata")
        metadata = decode_metadata(encoded)
        identities = {key: package.identity for key, package in installed_packages(metadata["status"]).items()}
        if identities != expected:
            raise ValueError("release packages and installed status disagree")
        info = validate_info(metadata["info"], identities)
        return cls(packages, metadata["status"], info, encoded)

    @classmethod
    def create(cls, packages, status, info):
        packages = sorted(packages, key=lambda package: package.archive.identity.key)
        return cls.from_payload({
            "packages": [package.to_json() for package in packages],
            "package_metadata": encode_metadata(status, info),
        })

    def payload(self):
        # Keep the original compressed bytes: the root's pinned Python wrote them.
        return {
            "packages": [package.to_json() for package in self.packages],
            "package_metadata": self.encoded_metadata,
        }

    def commitment(self):
        return hashlib.sha256(canonical(self.payload())).hexdigest()

    def verify_commitment(self, lower):
        marker = Path(lower) / COMMITMENT_PATH
        if marker.is_symlink():
            raise ValueError("inventory commitment must be a regular image file")
        with marker.open("rb") as handle:
            actual = handle.read(66)
        if actual != (self.commitment() + "\n").encode("ascii"):
            raise ValueError("release inventory differs from image commitment")

    def materialize(self, upper):
        upper = Path(upper)
        if upper.is_symlink() or not upper.is_dir() or any(upper.iterdir()):
            raise ValueError("scanner upper directory must be empty and private")
        database = upper / "var/lib/dpkg"
        directory = database / "info"
        directory.mkdir(parents=True, mode=0o755)
        for path, contents in [(database / "status", self.status), *(
            (directory / filename, text) for filename, text in self.info.items()
        )]:
            with path.open("x", encoding="utf-8") as handle:
                handle.write(contents)
            path.chmod(0o644)


@dataclass(frozen=True)
class ImageFacts:
    sha256: str
    length: int

    @classmethod
    def parse(cls, release):
        digest = release.get("sha256")
        length = release.get("length")
        if not isinstance(digest, str) or SHA256.fullmatch(digest) is None:
            raise ValueError("release needs an image SHA-256")
        if type(length) is not int or length <= 0:
            raise ValueError("release needs a positive image length")
        return cls(digest, length)

    def verify(self, image):
        image = Path(image)
        if image.name != f"toolbox-{self.sha256}.erofs":
            raise ValueError("image filename differs from its release digest")
        if image.stat().st_size != self.length or file_sha256(image) != self.sha256:
            raise ValueError("image bytes differ from their release identity")


def prepare(image, lower, upper):
    release = read_json(Path(image).with_suffix(".json"))
    inventory = Inventory.from_payload(release)
    facts = ImageFacts.parse(release)
    facts.verify(image)
    inventory.verify_commitment(lower)
    inventory.materialize(upper)
    return inventory


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit("usage: scan_inventory.py IMAGE LOWER UPPER")
    prepare(*sys.argv[1:])
