"""Publish prebuilt runtime images and promote their verified digest."""

import json
from pathlib import Path
import subprocess

from .github import verify_rollout, verify_source
from .model import (
    IMAGE_REPOSITORY, record, save, string,
    verify_candidate, verify_development,
)


def docker(*args: str) -> str:
    result = subprocess.run(["docker", *args], check=True, stdout=subprocess.PIPE, text=True)
    return result.stdout.strip()


def digest(reference: str) -> str:
    manifest = record(json.loads(docker("buildx", "imagetools", "inspect", reference,
                                       "--format", "{{json .Manifest}}")))
    return string(manifest["digest"], r"sha256:[0-9a-f]{64}")


def publish_runtime(root: Path, tree: str, version: str, repository: str) -> str:
    candidate = verify_candidate(root, tree, version, stage="signed")
    verify_source(candidate, repository)
    sources = []
    for arch in ("amd64", "arm64"):
        docker("load", "--input", str(root / "images" / f"daemon-linux-{arch}.tar"))
        local = f"agentsfleet-candidate:daemon-linux-{arch}"
        target = f"{IMAGE_REPOSITORY}:dev-{tree}-{arch}"
        docker("tag", local, target)
        docker("push", target)
        sources.append(f"{IMAGE_REPOSITORY}@{digest(target)}")
    target = f"{IMAGE_REPOSITORY}:dev-{tree}"
    docker("buildx", "imagetools", "create", "--tag", target, *sources)
    image = f"{IMAGE_REPOSITORY}@{digest(target)}"
    docker("buildx", "imagetools", "create", "--tag", f"{IMAGE_REPOSITORY}:dev-latest",
           "--tag", f"{IMAGE_REPOSITORY}:{version}-dev", image)
    save(root / "images.json", {"image": image})
    return image


def promote_runtime(root: Path, tree: str, version: str, repository: str) -> str:
    receipt = verify_development(root, tree, version)
    verify_source(verify_candidate(root, tree, version, stage="published"), repository)
    verify_rollout(receipt, repository)
    if digest(receipt.image) != receipt.image.split("@", 1)[1]:
        raise ValueError("runtime registry digest does not match verified candidate")
    docker("buildx", "imagetools", "create", "--tag", f"{IMAGE_REPOSITORY}:{version}",
           "--tag", f"{IMAGE_REPOSITORY}:latest", receipt.image)
    if digest(f"{IMAGE_REPOSITORY}:{version}") != receipt.image.split("@", 1)[1]:
        raise ValueError("production tag changed the verified runtime image digest")
    return receipt.image
