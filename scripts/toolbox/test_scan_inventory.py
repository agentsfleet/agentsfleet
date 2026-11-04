#!/usr/bin/env python3
"""Release validation refuses false inventory before writing scanner records."""

import gzip
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from scan_inventory import (
    INFO_LIMIT, MANIFEST_LIMIT, METADATA_LIMIT, PACKAGE_LIMIT, Inventory,
    canonical, encode_metadata, prepare,
)


STATUS = """Package: git
Status: install ok installed
Architecture: amd64
Version: 1.0-1
Source: git-source (1.0-2)

Package: perl-base
Status: install ok installed
Architecture: amd64
Version: 1.0-1

"""
INFO = {
    "git.list": "/usr/bin/git\n/usr/share/doc/git/copyright\n",
    "git.md5sums": "a" * 32 + "  usr/bin/git\n",
    "perl-base.list": "/usr/bin/perl\n",
}


def fixture(directory):
    packages = [{
        "package": name, "version": "1.0-1", "architecture": "amd64",
        "sha256": "a" * 64,
        "copyright": {"path": f"/usr/share/doc/{name}/copyright", "sha256": "b" * 64},
    } for name in ("git", "perl-base")]
    value = {"packages": packages, "package_metadata": encode_metadata(STATUS, INFO)}
    inventory = Inventory.from_payload(value)
    image_bytes = b"an image fixture"
    digest = hashlib.sha256(image_bytes).hexdigest()
    image = directory / f"toolbox-{digest}.erofs"
    image.write_bytes(image_bytes)
    value.update({"sha256": digest, "length": len(image_bytes)})
    image.with_suffix(".json").write_text(json.dumps(value))
    lower, upper = directory / "lower", directory / "upper"
    (lower / "etc").mkdir(parents=True)
    (lower / "etc/toolbox-inventory.sha256").write_text(inventory.commitment() + "\n")
    upper.mkdir()
    return image, lower, upper, value


class InventoryTests(unittest.TestCase):
    def test_inventory_restores_complete_records_without_changing_the_image(self):
        with tempfile.TemporaryDirectory() as temporary:
            image, lower, upper, _ = fixture(Path(temporary))
            original = image.read_bytes()
            inventory = prepare(image, lower, upper)
            database = upper / "var/lib/dpkg"
            self.assertEqual((database / "status").read_text(), STATUS)
            self.assertEqual((database / "info/git.md5sums").read_text(), INFO["git.md5sums"])
            self.assertEqual(sorted(path.name for path in (database / "info").iterdir()),
                             ["git.list", "git.md5sums", "perl-base.list"])
            self.assertEqual((database / "status").stat().st_mode & 0o777, 0o644)
            self.assertEqual(len(inventory.packages), 2)
            self.assertEqual(image.read_bytes(), original)
            self.assertFalse((lower / "var/lib/dpkg").exists())

    def test_inventory_encoding_is_deterministic_and_retains_source_versions(self):
        with tempfile.TemporaryDirectory() as temporary:
            _, _, _, value = fixture(Path(temporary))
            first = Inventory.from_payload(value)
            second = Inventory.create(first.packages, first.status, first.info)
            self.assertEqual(first.payload(), second.payload())
            self.assertEqual(first.commitment(), second.commitment())
            self.assertIn("Source: git-source (1.0-2)", second.status)
            self.assertEqual(second.info, INFO)

    def test_coherent_omission_and_changed_source_or_file_records_fail_commitment(self):
        for change in ("omit", "source", "file", "archive", "notice"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                status, info = STATUS, dict(INFO)
                if change == "omit":
                    value["packages"] = value["packages"][1:]
                    status = STATUS[STATUS.index("Package: perl-base"):]
                    info = {"perl-base.list": INFO["perl-base.list"]}
                elif change == "source":
                    status = STATUS.replace("git-source (1.0-2)", "other-source (2.0-1)")
                elif change == "file":
                    info["git.md5sums"] = "c" * 32 + "  usr/bin/git\n"
                elif change == "archive":
                    value["packages"][0]["sha256"] = "c" * 64
                else:
                    value["packages"][0]["copyright"]["sha256"] = "c" * 64
                value["package_metadata"] = encode_metadata(status, info)
                image.with_suffix(".json").write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, "commitment"):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertFalse((lower / "var/lib/dpkg").exists())

    def test_malformed_inventory_fails_before_creating_records(self):
        cases = (
            ("duplicate package", "duplicate", lambda v: v["packages"].append(v["packages"][0])),
            ("missing package", "disagree", lambda v: v["packages"].pop()),
            ("bad notice", "copyright", lambda v: v["packages"][0]["copyright"].update(path="/etc/passwd")),
            ("bad digest", "SHA-256", lambda v: v["packages"][0].update(sha256="wrong")),
            ("wrong type", "bounded array", lambda v: v.update(packages={})),
            ("too many", "bounded array", lambda v: v.update(packages=v["packages"] * PACKAGE_LIMIT)),
            ("bad compressed data", "compressed", lambda v: v.update(package_metadata="!")),
            ("oversized encoded", "bounded", lambda v: v.update(package_metadata="A" * (MANIFEST_LIMIT + 1))),
        )
        for name, diagnostic, mutate in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                mutate(value)
                image.with_suffix(".json").write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, diagnostic + "|JSON exceeds"):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertEqual(image.read_bytes(), b"an image fixture")

    def test_metadata_paths_types_duplicates_and_incomplete_lists_are_refused(self):
        cases = (
            ("../git.list", "/usr/bin/git\n", "basename"),
            ("git.postinst", "shell commands", "filename"),
            ("missing.list", "/usr/bin/git\n", "owner"),
            ("git:arm64.list", "/usr/bin/git\n", "owner"),
            ("git.list", 1, "text"),
            ("git.list", "\0", "NUL"),
        )
        for name, contents, diagnostic in cases:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                info = {**INFO, name: contents}
                value["package_metadata"] = encode_metadata(STATUS, info)
                image.with_suffix(".json").write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, diagnostic):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertFalse((lower / "var/lib/dpkg").exists())
        for info in (
            {"git.list": INFO["git.list"]},
            {**INFO, "git:amd64.list": INFO["git.list"]},
            {f"package{index}.list": "" for index in range(INFO_LIMIT + 1)},
        ):
            with self.subTest(info_entries=len(info)), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                value["package_metadata"] = encode_metadata(STATUS, info)
                image.with_suffix(".json").write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertFalse((lower / "var/lib/dpkg").exists())

    def test_compression_bombs_truncation_and_concatenation_are_refused(self):
        import base64
        encoded = (
            gzip.compress(b"x" * (METADATA_LIMIT + 1), mtime=0),
            gzip.compress(canonical({"status": STATUS, "info": INFO}), mtime=0)[:-1],
            gzip.compress(b"{}", mtime=0) + gzip.compress(b"{}", mtime=0),
        )
        for compressed in encoded:
            with self.subTest(size=len(compressed)), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                value["package_metadata"] = base64.b64encode(compressed).decode()
                image.with_suffix(".json").write_text(json.dumps(value))
                with self.assertRaisesRegex(ValueError, "oversized, truncated or concatenated"):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertFalse((lower / "var/lib/dpkg").exists())

    def test_duplicate_json_keys_and_manifest_byte_overflow_are_refused(self):
        for contents, diagnostic in (
            ('{"packages":[],"packages":[]}', "duplicate JSON key"),
            (" " * (MANIFEST_LIMIT + 1), "JSON exceeds"),
        ):
            with self.subTest(diagnostic=diagnostic), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, _ = fixture(Path(temporary))
                image.with_suffix(".json").write_text(contents)
                with self.assertRaisesRegex(ValueError, diagnostic):
                    prepare(image, lower, upper)
                self.assertEqual(list(upper.iterdir()), [])
                self.assertEqual(image.read_bytes(), b"an image fixture")

    def test_wrong_image_identity_marker_and_nonempty_upper_are_refused(self):
        for change in ("bytes", "length", "name", "marker", "marker link", "upper"):
            with self.subTest(change=change), tempfile.TemporaryDirectory() as temporary:
                image, lower, upper, value = fixture(Path(temporary))
                marker = lower / "etc/toolbox-inventory.sha256"
                if change == "bytes":
                    image.write_bytes(b"changed bytes")
                elif change == "length":
                    value["length"] = True
                    image.with_suffix(".json").write_text(json.dumps(value))
                elif change == "name":
                    other = image.with_name("toolbox-" + "a" * 64 + ".erofs")
                    image.rename(other)
                    image.with_suffix(".json").rename(other.with_suffix(".json"))
                    image = other
                elif change == "marker":
                    marker.write_text("a" * 64 + "\n")
                elif change == "marker link":
                    target = lower / "marker"
                    marker.rename(target)
                    marker.symlink_to(target)
                else:
                    (upper / "existing").write_text("keep")
                before = sorted(path.name for path in upper.iterdir())
                with self.assertRaises(ValueError):
                    prepare(image, lower, upper)
                self.assertEqual(sorted(path.name for path in upper.iterdir()), before)
                self.assertFalse((upper / "var").exists())


if __name__ == "__main__":
    unittest.main()
