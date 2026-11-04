"""Debian identities shared by the builder and its scanner inventory."""

from dataclasses import dataclass
from email.parser import Parser
import re


NAME = re.compile(r"[a-z0-9][a-z0-9+.-]+")
ARCHITECTURE = re.compile(r"[a-z0-9][a-z0-9-]*")
VERSION = re.compile(r"[0-9][A-Za-z0-9.+:~\-]*")
SHA256 = re.compile(r"[0-9a-f]{64}")


@dataclass(frozen=True)
class Identity:
    name: str
    version: str
    architecture: str

    @classmethod
    def parse(cls, name, version, architecture):
        for value, pattern, label in (
            (name, NAME, "package"),
            (version, VERSION, "version"),
            (architecture, ARCHITECTURE, "architecture"),
        ):
            if not isinstance(value, str) or pattern.fullmatch(value) is None:
                raise ValueError(f"invalid Debian {label}: {value!r}")
        return cls(name, version, architecture)

    @property
    def key(self):
        return self.name, self.architecture

    @property
    def notice_path(self):
        return f"/usr/share/doc/{self.name}/copyright"


@dataclass(frozen=True)
class ArchivePackage:
    identity: Identity
    sha256: str

    @classmethod
    def parse(cls, fields):
        identity = Identity.parse(
            fields.get("package"), fields.get("version"), fields.get("architecture")
        )
        digest = fields.get("sha256")
        if not isinstance(digest, str) or SHA256.fullmatch(digest) is None:
            raise ValueError(f"missing or invalid archive SHA-256: {identity.name}")
        return cls(identity, digest)


@dataclass(frozen=True)
class ReleasePackage:
    archive: ArchivePackage
    notice_sha256: str

    @classmethod
    def parse(cls, value):
        if not isinstance(value, dict) or set(value) != {
            "package", "version", "architecture", "sha256", "copyright"
        }:
            raise ValueError("package needs identity, archive hash and copyright")
        archive = ArchivePackage.parse(value)
        notice = value["copyright"]
        if not isinstance(notice, dict) or set(notice) != {"path", "sha256"}:
            raise ValueError("copyright needs path and sha256")
        digest = notice["sha256"]
        if (notice["path"] != archive.identity.notice_path
                or not isinstance(digest, str) or SHA256.fullmatch(digest) is None):
            raise ValueError(f"invalid copyright: {archive.identity.name}")
        return cls(archive, digest)

    def to_json(self):
        identity = self.archive.identity
        return {
            "package": identity.name,
            "version": identity.version,
            "architecture": identity.architecture,
            "sha256": self.archive.sha256,
            "copyright": {"path": identity.notice_path, "sha256": self.notice_sha256},
        }


def debian_stanzas(text):
    if not isinstance(text, str) or not text.strip() or "\0" in text:
        raise ValueError("Debian records must be nonempty text without NUL")
    for paragraph in text.strip().split("\n\n"):
        if not paragraph.strip():
            continue
        message = Parser().parsestr(paragraph.lstrip("\n"), headersonly=True)
        if message.defects or message.get_payload():
            raise ValueError("malformed Debian record")
        fields = {}
        for key, value in message.raw_items():
            key = key.lower()
            if key in fields:
                raise ValueError(f"duplicate Debian field: {key}")
            fields[key] = value.strip()
        yield fields


@dataclass(frozen=True)
class InstalledPackage:
    identity: Identity
    source_name: str
    source_version: str


def installed_packages(status):
    packages = {}
    for fields in debian_stanzas(status):
        identity = Identity.parse(
            fields.get("package"), fields.get("version"), fields.get("architecture")
        )
        if fields.get("status") != "install ok installed":
            raise ValueError(f"package is not installed: {identity.name}")
        if identity.key in packages:
            raise ValueError(f"duplicate installed package: {identity.key}")
        source = fields.get("source")
        source_identity = identity
        if source is not None:
            match = re.fullmatch(r"([^ ()]+)(?: \(([^()]+)\))?", source)
            if match is None:
                raise ValueError(f"invalid source identity: {identity.name}")
            source_identity = Identity.parse(match[1], match[2] or identity.version, identity.architecture)
        packages[identity.key] = InstalledPackage(identity, source_identity.name, source_identity.version)
    return packages


def archive_packages(text):
    packages = {}
    for fields in debian_stanzas(text):
        package = ArchivePackage.parse(fields)
        if package.identity.key in packages:
            raise ValueError(f"duplicate archive package: {package.identity.key}")
        packages[package.identity.key] = package
    return packages


def info_identity(filename, identities):
    basename, dot, suffix = filename.rpartition(".")
    if dot != "." or suffix not in {"list", "md5sums"}:
        raise ValueError(f"invalid package info filename: {filename}")
    name, separator, architecture = basename.partition(":")
    if NAME.fullmatch(name) is None or (separator and ARCHITECTURE.fullmatch(architecture) is None):
        raise ValueError(f"invalid package info basename: {filename}")
    matches = [identity for identity in identities.values()
               if identity.name == name and (not separator or identity.architecture == architecture)]
    if len(matches) != 1:
        raise ValueError(f"info has no unique retained owner: {filename}")
    return matches[0]
