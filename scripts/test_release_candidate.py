#!/usr/bin/env python3
"""Offline promotion proofs; external GitHub/registry calls stop at their boundary."""

from dataclasses import asdict
import hashlib
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from release_candidate.archive import package
from release_candidate.github import select, verify_rollout, verify_source
from release_candidate.model import (
    BINARIES, Candidate, Development, GATES, IMAGE_REPOSITORY, Source, create,
    inventory, load, safe_path, save, verify_candidate, verify_development,
)
from release_candidate.registry import promote_runtime, publish_runtime
from release_candidate.signature import verify as verify_signature


REPOSITORY = "agentsfleet/agentsfleet"
SOURCE_SHA, HEAD_SHA, TREE, BASE_SHA, DEPLOY_SHA = (c * 40 for c in "abcde")
SOURCE = Source(REPOSITORY, "0.71.0", SOURCE_SHA, TREE, HEAD_SHA, 100, 1, "dist")
IMAGE = f"{IMAGE_REPOSITORY}@sha256:{'f' * 64}"


def run(identifier: int = 100, *, development: bool = False) -> dict:
    return {
        "id": identifier, "run_attempt": 1, "head_sha": DEPLOY_SHA if development else HEAD_SHA,
        "repository": {"full_name": REPOSITORY}, "head_repository": {"full_name": REPOSITORY},
        "event": "push" if development else "pull_request", "conclusion": "success",
        "path": f".github/workflows/{'deploy-dev' if development else 'test'}.yml",
    }


def commit(*, development: bool = False) -> dict:
    return {"tree": {"sha": TREE}, "parents": [] if development else [
        {"sha": BASE_SHA}, {"sha": HEAD_SHA},
    ]}


def fetch(path: str) -> object:
    responses = {
        f"repos/{REPOSITORY}/actions/runs/100": run(),
        f"repos/{REPOSITORY}/actions/runs/200": run(200, development=True),
        f"repos/{REPOSITORY}/git/commits/{SOURCE_SHA}": commit(),
        f"repos/{REPOSITORY}/git/commits/{DEPLOY_SHA}": commit(development=True),
    }
    return responses[path]


def candidate_fixture(root: Path) -> str:
    for name in BINARIES:
        path = root / "bin" / name
        path.parent.mkdir(exist_ok=True)
        path.write_bytes(f"compiled {name} at {SOURCE_SHA}".encode())
    (root / "images").mkdir()
    for arch in ("amd64", "arm64"):
        (root / "images" / f"daemon-linux-{arch}.tar").write_bytes(f"saved {arch} image".encode())
    (root / "toolbox").mkdir()
    contents = b"pinned toolbox image"
    digest = hashlib.sha256(contents).hexdigest()
    (root / "toolbox" / f"toolbox-{digest}.erofs").write_bytes(contents)
    save(root / "toolbox" / f"toolbox-{digest}.json", {
        "sha256": digest, "length": len(contents), "arch": "amd64", "runner_versions": [SOURCE.version],
    })
    for suffix in ("sbom.spdx.json", "scan.json"):
        save(root / "toolbox" / f"toolbox-{digest}.{suffix}", {"report": suffix})
    create(root, SOURCE)
    return digest


def promote_fixture(root: Path, digest: str) -> Development:
    (root / "toolbox" / f"toolbox-{digest}.json.sig").write_text("signature")
    (root / "toolbox" / "release.pub").write_text("public key")
    save(root / "promotion.scan.json", {"matches": []})
    package(root)
    save(root / "images.json", {"image": IMAGE})
    receipt = Development(REPOSITORY, TREE, DEPLOY_SHA, 200, 1, IMAGE,
                          inventory(root), dict.fromkeys(GATES, "success"))
    save(root / "development.json", asdict(receipt))
    return receipt


def signed_fixture(root: Path, digest: str) -> None:
    promote_fixture(root, digest)
    (root / "development.json").unlink()
    (root / "images.json").unlink()


class PayloadTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.digest = candidate_fixture(self.root)

    def test_complete_payload_and_synthetic_merge_are_verified(self) -> None:
        candidate = verify_candidate(self.root, TREE, SOURCE.version)
        verify_source(candidate, REPOSITORY, fetch)
        self.assertEqual(candidate.source.source_sha, SOURCE_SHA)

    def test_changed_binary_is_rejected_before_registry_writes(self) -> None:
        signed_fixture(self.root, self.digest)
        (self.root / "bin" / BINARIES[0]).write_bytes(b"substituted binary")
        with patch("release_candidate.registry.docker") as docker:
            with self.assertRaisesRegex(ValueError, "checksum"):
                publish_runtime(self.root, TREE, SOURCE.version, REPOSITORY)
            docker.assert_not_called()

    def test_tree_and_version_mismatches_fail(self) -> None:
        for tree, version in ((BASE_SHA, SOURCE.version), (TREE, "0.72.0")):
            with self.subTest(tree=tree, version=version), self.assertRaisesRegex(ValueError, "source tree or version"):
                verify_candidate(self.root, tree, version)

    def test_missing_release_platform_fails(self) -> None:
        (self.root / "bin" / BINARIES[1]).unlink()
        with self.assertRaisesRegex(ValueError, "file set"):
            verify_candidate(self.root, TREE, SOURCE.version)

    def test_additional_file_and_symlink_fail(self) -> None:
        path = self.root / "toolbox" / "unexpected"
        path.write_text("extra")
        with self.assertRaisesRegex(ValueError, "file set"):
            verify_candidate(self.root, TREE, SOURCE.version)
        path.unlink()
        path.symlink_to(self.root / "bin" / BINARIES[0])
        with self.assertRaisesRegex(ValueError, "link or special"):
            verify_candidate(self.root, TREE, SOURCE.version)
        path.unlink()
        save(self.root / "development.json", {"untrusted": "injected receipt"})
        with self.assertRaisesRegex(ValueError, "file set"):
            verify_candidate(self.root, TREE, SOURCE.version)

    def test_unsafe_paths_and_profile_are_rejected(self) -> None:
        for name in ("../bin", "/etc/passwd", "bin/../../file", "bin//file", "./file"):
            with self.subTest(name=name), self.assertRaises(ValueError):
                safe_path(name)
        manifest = load(self.root / "candidate.json")
        manifest["source"]["profile"] = "fast-profile"
        save(self.root / "candidate.json", manifest)
        with self.assertRaises(ValueError):
            Candidate.read(self.root)

    def test_toolbox_manifest_cannot_claim_a_different_version(self) -> None:
        manifest = self.root / "toolbox" / f"toolbox-{self.digest}.json"
        data = load(manifest)
        data["runner_versions"] = ["0.70.0"]
        save(manifest, data)
        # Even a self-consistent candidate receipt cannot override this identity.
        with self.assertRaisesRegex(ValueError, "toolbox manifest identity"):
            create(self.root, SOURCE)

    def test_qualified_payload_and_archive_preserve_original_bytes(self) -> None:
        promote_fixture(self.root, self.digest)
        receipt = verify_development(self.root, TREE, SOURCE.version)
        verify_rollout(receipt, REPOSITORY, fetch)
        archive = self.root / "archives" / f"{BINARIES[0]}.tar.gz"
        with tarfile.open(archive) as handle:
            member = handle.getmember(BINARIES[0])
            reader = handle.extractfile(member)
            self.assertIsNotNone(reader)
            with reader:
                self.assertEqual(reader.read(), (self.root / "bin" / BINARIES[0]).read_bytes())
            self.assertEqual(member.mode, 0o755)

    def test_skipped_metal_never_qualifies(self) -> None:
        promote_fixture(self.root, self.digest)
        path = self.root / "development.json"
        data = load(path)
        data["gates"]["metal"] = "skipped"
        save(path, data)
        with self.assertRaisesRegex(ValueError, "passing development gates"):
            verify_development(self.root, TREE, SOURCE.version)

    def test_replacing_retained_scan_or_image_fails(self) -> None:
        promote_fixture(self.root, self.digest)
        (self.root / "promotion.scan.json").write_text('{"replaced": true}')
        with self.assertRaisesRegex(ValueError, "checksum"):
            verify_development(self.root, TREE, SOURCE.version)

    def test_wrong_embedded_signing_key_fails_before_cosign(self) -> None:
        promote_fixture(self.root, self.digest)
        with patch("release_candidate.signature.subprocess.run") as process:
            with self.assertRaisesRegex(ValueError, "embedded"):
                verify_signature(self.root)
            process.assert_not_called()

    def test_production_digest_change_fails(self) -> None:
        promote_fixture(self.root, self.digest)
        digests = [IMAGE.split("@")[1], "sha256:" + "0" * 64]
        with patch("release_candidate.registry.verify_rollout"), \
                patch("release_candidate.registry.verify_source"), \
                patch("release_candidate.registry.digest", side_effect=digests), \
                patch("release_candidate.registry.docker"):
            with self.assertRaisesRegex(ValueError, "changed.*digest"):
                promote_runtime(self.root, TREE, SOURCE.version, REPOSITORY)

    def test_wrong_repository_is_rejected_before_registry_writes(self) -> None:
        signed_fixture(self.root, self.digest)
        with patch("release_candidate.registry.docker") as docker:
            with self.assertRaisesRegex(ValueError, "different repository"):
                publish_runtime(self.root, TREE, SOURCE.version, "another/repository")
            docker.assert_not_called()

    def test_verified_images_publish_and_record_the_multi_platform_digest(self) -> None:
        signed_fixture(self.root, self.digest)
        digests = ["sha256:" + c * 64 for c in "abc"]
        with patch("release_candidate.registry.verify_source", side_effect=lambda c, r: verify_source(c, r, fetch)), \
                patch("release_candidate.registry.digest", side_effect=digests), \
                patch("release_candidate.registry.docker", return_value="") as docker:
            image = publish_runtime(self.root, TREE, SOURCE.version, REPOSITORY)
        self.assertEqual(load(self.root / "images.json"), {"image": image})
        self.assertEqual(image, f"{IMAGE_REPOSITORY}@sha256:{'c' * 64}")
        loads = [call.args for call in docker.call_args_list if call.args[0] == "load"]
        self.assertEqual(loads, [("load", "--input", str(self.root / "images" / f"daemon-linux-{a}.tar"))
                                 for a in ("amd64", "arm64")])

    def test_a_self_consistent_changed_image_still_must_match_development(self) -> None:
        promote_fixture(self.root, self.digest)
        save(self.root / "images.json", {"image": IMAGE.replace("f" * 64, "0" * 64)})
        receipt = load(self.root / "development.json")
        receipt["files"] = {name: asdict(entry) for name, entry in inventory(self.root, ("development.json",)).items()}
        save(self.root / "development.json", receipt)
        with self.assertRaisesRegex(ValueError, "differs from the development-verified"):
            verify_development(self.root, TREE, SOURCE.version)


class WorkflowTests(unittest.TestCase):
    def test_failed_wrong_workflow_fork_and_wrong_attempt_fail(self) -> None:
        mutations = (
            {"conclusion": "failure"}, {"path": ".github/workflows/other.yml"},
            {"head_repository": {"full_name": "fork/agentsfleet"}}, {"run_attempt": 2},
        )
        for mutation in mutations:
            def changed(path: str) -> object:
                return run() | mutation if path.endswith("/100") else fetch(path)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                verify_source(Candidate(SOURCE, {}), REPOSITORY, changed)

    def test_candidate_cannot_claim_unrelated_merge_or_tree(self) -> None:
        mutations = ({"parents": [{"sha": BASE_SHA}, {"sha": DEPLOY_SHA}]},
                     {"tree": {"sha": BASE_SHA}})
        for mutation in mutations:
            def changed(path: str) -> object:
                return commit() | mutation if "/git/commits/" in path else fetch(path)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                verify_source(Candidate(SOURCE, {}), REPOSITORY, changed)

    def test_dispatch_can_build_the_exact_tree_without_a_pr_merge(self) -> None:
        source = asdict(SOURCE) | {"source_sha": HEAD_SHA}
        candidate = Candidate(Source.parse(source), {})
        def dispatch(path: str) -> object:
            return run() | {"event": "workflow_dispatch"} if "/actions/runs/" in path else commit()
        verify_source(candidate, REPOSITORY, dispatch)

    def test_development_run_for_another_revision_fails(self) -> None:
        receipt = Development(REPOSITORY, TREE, DEPLOY_SHA, 200, 1, IMAGE, {}, dict.fromkeys(GATES, "success"))
        def wrong(path: str) -> object:
            return run(200, development=True) | {"head_sha": BASE_SHA} if "/actions/runs/" in path else commit()
        with self.assertRaisesRegex(ValueError, "does not match"):
            verify_rollout(receipt, REPOSITORY, wrong)

    def test_select_only_matching_unexpired_artifact_from_eligible_run(self) -> None:
        def available(path: str) -> object:
            if "/actions/runs/" in path:
                return run()
            return {"artifacts": [{"id": 123, "name": f"release-candidate-{TREE}",
                                   "expired": False, "workflow_run": {"id": 100}}]}
        self.assertEqual(select(REPOSITORY, TREE, available), (100, f"release-candidate-{TREE}"))

    def test_expired_or_different_tree_has_no_rebuild_fallback(self) -> None:
        for name, expired in ((f"release-candidate-{TREE}", True), (f"release-candidate-{BASE_SHA}", False)):
            def unavailable(path: str) -> object:
                if "/actions/runs/" in path:
                    return run()
                return {"artifacts": [{"id": 123, "name": name, "expired": expired,
                                       "workflow_run": {"id": 100}}]}
            with self.subTest(name=name, expired=expired), self.assertRaisesRegex(ValueError, "no successful matching"):
                select(REPOSITORY, TREE, unavailable)

    def test_failed_or_fork_owned_artifacts_are_not_selected(self) -> None:
        for mutation in ({"conclusion": "failure"}, {"head_repository": {"full_name": "fork/repo"}}):
            def untrusted(path: str) -> object:
                if "/actions/runs/" in path:
                    return run() | mutation
                return {"artifacts": [{"id": 123, "name": f"release-candidate-{TREE}",
                                       "expired": False, "workflow_run": {"id": 100}}]}
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "no successful matching"):
                select(REPOSITORY, TREE, untrusted)


if __name__ == "__main__":
    unittest.main(verbosity=2)
