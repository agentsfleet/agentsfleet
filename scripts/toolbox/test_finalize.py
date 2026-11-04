#!/usr/bin/env python3
"""Policy and independent payload checks without requiring root privileges."""

import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from finalize import APPROVED_PURGE, RemovalPlan
from finalize_payload import (
    Notice, owned_paths, preserve_notices, reconcile, require_removed, snapshot,
)
from package_records import Identity


PURGE = " ".join(sorted(APPROVED_PURGE))


def records(names, archive=False):
    return "\n\n".join(
        f"Package: {name}\nVersion: 1.0-1\nArchitecture: amd64\n"
        + ("SHA256: " + "a" * 64 + "\n" if archive else
           "Status: install ok installed\nSource: example-source (1.0-2)\n")
        for name in sorted(names)
    ) + "\n\n"


class FinalizationTests(unittest.TestCase):
    def test_policy_accepts_only_exact_removal_and_required_inputs(self):
        names = APPROVED_PURGE | {"git", "perl-base"}
        status, provenance = records(names), records(names, archive=True)
        plan = RemovalPlan.parse(status, provenance, PURGE, "git perl-base")
        self.assertEqual(set(plan.retained), {("git", "amd64"), ("perl-base", "amd64")})
        self.assertEqual(plan.purge[-1], "dpkg")
        self.assertEqual(set(plan.purge), APPROVED_PURGE)
        self.assertEqual(plan.retained[("git", "amd64")].source_version, "1.0-2")
        for purge, required, diagnostic in (
            (PURGE + " apt", "git", "unique"),
            (PURGE.replace("apt ", ""), "git", "exactly"),
            (PURGE + " git", "perl-base", "exactly"),
            (PURGE, "git apt", "overlaps"),
            (PURGE, "git git", "unique"),
            (PURGE, "missing", "missing"),
            (PURGE, "git;false", "names"),
            (PURGE, "", "nonempty"),
        ):
            with self.subTest(purge=purge, required=required):
                with self.assertRaisesRegex(ValueError, diagnostic):
                    RemovalPlan.parse(status, provenance, purge, required)

    def test_provenance_and_final_status_preserve_versions_sources_and_architecture(self):
        names = APPROVED_PURGE | {"git", "perl-base"}
        status, provenance = records(names), records(names, archive=True)
        plan = RemovalPlan.parse(status, provenance, PURGE, "git perl-base")
        final = records({"git", "perl-base"})
        plan.verify_status(final)
        self.assertEqual(plan.retained_identities[("git", "amd64")].version, "1.0-1")
        self.assertEqual(plan.retained[("git", "amd64")].source_name, "example-source")
        for changed in (
            final.replace("Version: 1.0-1", "Version: 2.0-1"),
            final.replace("example-source (1.0-2)", "other-source (1.0-3)"),
            records({"git"}),
            records({"git", "perl-base", "apt"}),
            final.replace("Architecture: amd64", "Architecture: arm64"),
        ):
            with self.subTest(status=changed):
                with self.assertRaisesRegex(ValueError, "unexpected final"):
                    plan.verify_status(changed)
        for raw_status, raw_provenance, diagnostic in (
            (status, provenance.replace("Version: 1.0-1", "Version: 2.0-1"), "provenance"),
            (status, provenance + records({"git"}, archive=True), "duplicate"),
            (status, provenance.replace("a" * 64, "missing"), "SHA-256"),
            (status + records({"git"}), provenance, "duplicate"),
            (status.replace("install ok installed", "deinstall ok config-files"), provenance, "not installed"),
            (status.replace("example-source (1.0-2)", "../bad (1.0-2)"), provenance, "package"),
            (records({"git", "perl-base"}), provenance, "missing"),
        ):
            with self.subTest(diagnostic=diagnostic):
                with self.assertRaisesRegex(ValueError, diagnostic):
                    RemovalPlan.parse(raw_status, raw_provenance, PURGE, "git perl-base")

    def test_retained_shared_payload_and_modes_survive_removal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kept, shared, removed = (root / name for name in ("kept", "shared", "removed"))
            kept.write_bytes(b"runtime")
            kept.chmod(0o640)
            shared.write_bytes(b"shared rights")
            removed.write_bytes(b"manager")
            info = root / "info"
            info.mkdir()
            (info / "git.list").write_text(f"{kept}\n{shared}\n")
            (info / "apt.list").write_text(f"{removed}\n{shared}\n")
            identities = {(name, "amd64"): Identity.parse(name, "1.0-1", "amd64")
                          for name in ("git", "apt")}
            retained, purged = owned_paths(info, identities, {("git", "amd64")})
            expected = snapshot(retained)
            unwanted = snapshot(purged)
            removed.unlink()
            reconcile(expected, {})
            require_removed(unwanted)
            self.assertEqual(kept.read_bytes(), b"runtime")
            self.assertEqual(shared.read_bytes(), b"shared rights")
            self.assertEqual(kept.stat().st_mode & 0o777, 0o640)
            self.assertEqual(purged, {removed})
            self.assertFalse(removed.exists())

    def test_missing_changed_and_replaced_retained_payload_is_refused(self):
        for fault in ("missing", "bytes", "mode", "link"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                retained, target, alias = (root / name for name in ("retained", "target", "alias"))
                retained.write_text("runtime")
                retained.chmod(0o644)
                target.write_text("original target")
                alias.symlink_to(target)
                expected = snapshot({retained, alias})
                if fault == "missing":
                    retained.unlink()
                elif fault == "bytes":
                    retained.write_text("changed runtime")
                elif fault == "mode":
                    retained.chmod(0o600)
                else:
                    alias.unlink()
                    alias.symlink_to(retained)
                with self.assertRaisesRegex(ValueError, "retained payload changed"):
                    reconcile(expected, {})
                self.assertEqual(target.read_text(), "original target")
                self.assertEqual(len(expected), 2)

    def test_notice_alias_is_materialized_without_restoring_removed_payload(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            owner, alias = root / "removed-doc", root / "retained-doc"
            owner.mkdir()
            (owner / "copyright").write_bytes(b"shared rights")
            (owner / "copyright").chmod(0o644)
            alias.symlink_to(owner, target_is_directory=True)
            path = alias / "copyright"
            expected = snapshot({alias, path})
            notice = Notice(b"shared rights", 0o644, hashlib.sha256(b"shared rights").hexdigest())
            shutil.rmtree(owner)
            exceptions = preserve_notices({path: notice})
            reconcile(expected, exceptions)
            self.assertEqual(path.read_bytes(), b"shared rights")
            self.assertFalse(alias.is_symlink())
            self.assertFalse(owner.exists())
            self.assertEqual(alias.stat().st_mode & 0o777, 0o755)
            self.assertEqual(path.stat().st_mode & 0o777, 0o644)
            path.write_bytes(b"tampered notice")
            with self.assertRaisesRegex(ValueError, "retained copyright changed"):
                preserve_notices({path: notice})
            with self.assertRaisesRegex(ValueError, "retained payload changed"):
                reconcile(expected, exceptions)
            self.assertEqual(path.read_bytes(), b"tampered notice")

    def test_removed_payload_and_unsafe_ownership_paths_are_refused(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            removed = root / "manager"
            removed.write_text("executable")
            unwanted = snapshot({removed})
            with self.assertRaisesRegex(ValueError, "removed package payload remains"):
                require_removed(unwanted)
            self.assertEqual(removed.read_text(), "executable")
            info = root / "info"
            info.mkdir()
            (info / "git.list").write_text("/usr/bin/../private\n")
            identity = Identity.parse("git", "1.0-1", "amd64")
            with self.assertRaisesRegex(ValueError, "invalid installed file path"):
                owned_paths(info, {identity.key: identity}, {identity.key})
            self.assertEqual((info / "git.list").read_text(), "/usr/bin/../private\n")
            self.assertEqual(removed.read_text(), "executable")

    def test_host_invocation_refuses_before_reading_or_purging_packages(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            provenance, output = root / "provenance", root / "inventory"
            provenance.write_text("do not modify")
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("finalize.py")),
                 "not-the-disposable-root", str(provenance), str(output), PURGE, "git"],
                capture_output=True, text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("identified disposable root", result.stderr)
            self.assertEqual(provenance.read_text(), "do not modify")
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
