#!/usr/bin/env python3
"""Workflow entry point for build-once native artifacts (no rebuild fallback)."""

import argparse
from dataclasses import asdict
import os
from pathlib import Path
import subprocess

from release_candidate.archive import package
from release_candidate.github import select, verify_rollout, verify_source
from release_candidate.model import (
    Development, GATES, Source, create, inventory, load, positive, record, save, string,
    verify_candidate, verify_development,
)
from release_candidate.registry import (
    promote_runtime, publish_runtime,
)
from release_candidate.signature import verify as verify_signature


def revision(expression: str) -> str:
    return subprocess.run(["git", "rev-parse", expression], check=True,
                          capture_output=True, text=True).stdout.strip()


def output(name: str, value: str | int) -> None:
    print(f"{name}={value}")
    destination = os.environ.get("GITHUB_OUTPUT")
    if destination:
        with open(destination, "a", encoding="utf-8") as handle:
            handle.write(f"{name}={value}\n")


def candidate_command(command: str, root: Path, tree: str, version: str, repository: str) -> None:
    if command == "identity":
        output("tree", tree)
        output("version", version)
    elif command == "create":
        source = Source.parse({
            "repository": repository, "version": version, "source_tree": tree,
            "source_sha": revision("HEAD"), "run_head_sha": os.environ["RUN_HEAD_SHA"],
            "run_id": int(os.environ["GITHUB_RUN_ID"]),
            "run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]), "profile": "dist",
        })
        create(root, source)
    elif command.startswith("select"):
        run_id, name = select(repository, tree, development=command == "select-development")
        output("run-id", run_id)
        output("artifact", name)
    else:
        candidate = verify_candidate(root, tree, version)
        verify_source(candidate, repository)
        if candidate.source.run_id != positive(int(os.environ["CANDIDATE_RUN_ID"])):
            raise ValueError("downloaded candidate belongs to a different workflow run")
        output("source-sha", candidate.source.source_sha)
        output("build-commit", candidate.source.source_sha[:7])
        output("version", version)
        output("tree", tree)


def promotion_command(command: str, root: Path, tree: str, version: str, repository: str) -> None:
    if command == "package":
        package(root)
    elif command == "verify-signature":
        verify_signature(root)
    elif command == "publish-runtime":
        output("image", publish_runtime(root, tree, version, repository))
    elif command == "qualify":
        candidate = verify_candidate(root, tree, version, stage="published")
        verify_source(candidate, repository)
        gates = {name: os.environ[f"GATE_{name.upper()}"] for name in GATES}
        if gates != dict.fromkeys(GATES, "success"):
            raise ValueError("every development gate, including metal, must pass")
        image = record(load(root / "images.json"), {"image"})["image"]
        receipt = Development(repository, tree, revision("HEAD"),
                              positive(int(os.environ["GITHUB_RUN_ID"])),
                              positive(int(os.environ["GITHUB_RUN_ATTEMPT"])),
                              image, inventory(root), gates)
        save(root / "development.json", asdict(receipt))
        verify_development(root, tree, version)
        output("tree", tree)
    elif command == "verify-development":
        receipt = verify_development(root, tree, version)
        verify_source(verify_candidate(root, tree, version, stage="published"), repository)
        verify_rollout(receipt, repository)
        if receipt.run_id != positive(int(os.environ["CANDIDATE_RUN_ID"])):
            raise ValueError("development candidate belongs to a different workflow run")
        output("image", receipt.image)
    else:
        output("image", promote_runtime(root, tree, version, repository))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=(
        "identity", "create", "select", "verify", "package", "publish-runtime",
        "qualify", "select-development", "verify-development", "promote-runtime", "verify-signature",
    ))
    parser.add_argument("--root", type=Path, default=Path("dist/candidate"))
    args = parser.parse_args()
    tree = string(revision("HEAD^{tree}"), r"[0-9a-f]{40}")
    version = Path("VERSION").read_text(encoding="utf-8").strip()
    repository = os.environ.get("GITHUB_REPOSITORY", "agentsfleet/agentsfleet")
    handler = candidate_command if args.command in (
        "identity", "create", "select", "select-development", "verify",
    ) else promotion_command
    handler(args.command, args.root, tree, version, repository)


if __name__ == "__main__":
    main()
