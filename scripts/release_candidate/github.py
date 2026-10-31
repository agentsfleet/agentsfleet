"""Select successful same-repository workflow artifacts, never a latest build."""

from dataclasses import dataclass
import json
import subprocess
from typing import Callable

from .model import Candidate, Development, positive, record, string


Fetch = Callable[[str], object]


def api(path: str) -> object:
    result = subprocess.run(["gh", "api", path], check=True, text=True, stdout=subprocess.PIPE)
    return json.loads(result.stdout)


@dataclass(frozen=True)
class Run:
    id: int
    attempt: int
    head: str
    repository: str
    head_repository: str
    event: str
    path: str
    conclusion: str | None

    @classmethod
    def parse(cls, raw: object) -> "Run":
        data = record(raw)
        return cls(
            positive(data["id"]), positive(data["run_attempt"]),
            string(data["head_sha"], r"[0-9a-f]{40}"),
            string(record(data["repository"])["full_name"], r"[\w.-]+/[\w.-]+"),
            string(record(data["head_repository"])["full_name"], r"[\w.-]+/[\w.-]+"),
            string(data["event"], r"[a-z_]+"), string(data["path"], r"[\w./@-]+"),
            data["conclusion"],
        )

    def require(self, repository: str, workflow: str, attempt: int) -> None:
        if (self.repository != repository or self.head_repository != repository
                or self.path != f".github/workflows/{workflow}.yml"
                or self.conclusion != "success" or self.attempt != attempt):
            raise ValueError("candidate workflow identity, attempt or conclusion is untrusted")


def tree_of(fetch: Fetch, repository: str, revision: str) -> tuple[str, list[str]]:
    data = record(fetch(f"repos/{repository}/git/commits/{revision}"))
    tree = string(record(data["tree"])["sha"], r"[0-9a-f]{40}")
    parents = [string(record(p)["sha"], r"[0-9a-f]{40}") for p in data["parents"]]
    return tree, parents


def verify_source(candidate: Candidate, repository: str, fetch: Fetch = api) -> None:
    source = candidate.source
    if source.repository != repository:
        raise ValueError("candidate came from a different repository")
    run = Run.parse(fetch(f"repos/{repository}/actions/runs/{source.run_id}"))
    run.require(repository, "test", source.run_attempt)
    tree, parents = tree_of(fetch, repository, source.source_sha)
    if tree != source.source_tree:
        raise ValueError("GitHub source commit tree differs from candidate receipt")
    if run.event == "pull_request":
        if len(parents) != 2 or parents[1] != source.run_head_sha:
            raise ValueError("candidate is not the tested pull-request merge tree")
        # GitHub may report the PR head or its synthetic merge as run.head_sha.
        if run.head not in (source.run_head_sha, source.source_sha):
            raise ValueError("candidate does not belong to this pull-request run")
    elif run.event in ("merge_group", "workflow_dispatch"):
        if run.head != source.source_sha or run.head != source.run_head_sha:
            raise ValueError("candidate does not belong to this workflow run")
    else:
        raise ValueError("unsupported candidate workflow event")


def select(repository: str, tree: str, fetch: Fetch = api, development: bool = False) -> tuple[int, str]:
    workflow = "deploy-dev" if development else "test"
    prefix = "development-candidate" if development else "release-candidate"
    events = ("push", "workflow_dispatch") if development else ("pull_request", "merge_group", "workflow_dispatch")
    name = f"{prefix}-{tree}"
    for page in range(1, 11):
        data = record(fetch(f"repos/{repository}/actions/artifacts?name={name}&per_page=100&page={page}"))
        artifacts = data["artifacts"]
        if not isinstance(artifacts, list):
            raise ValueError("invalid workflow artifact list")
        for raw in artifacts:
            artifact = record(raw)
            if artifact["name"] != name or artifact["expired"] is not False:
                continue
            positive(artifact["id"])
            run_id = positive(record(artifact["workflow_run"])["id"])
            run = Run.parse(fetch(f"repos/{repository}/actions/runs/{run_id}"))
            if (run.repository != repository or run.head_repository != repository
                    or run.path != f".github/workflows/{workflow}.yml"
                    or run.conclusion != "success"
                    or run.event not in events):
                continue
            return run.id, name
        if len(artifacts) < 100:
            break
    raise ValueError("no successful matching candidate; run test on the intended tree, then deploy-dev")


def verify_rollout(receipt: Development, repository: str, fetch: Fetch = api) -> None:
    if receipt.repository != repository:
        raise ValueError("development receipt came from another repository")
    run = Run.parse(fetch(f"repos/{repository}/actions/runs/{receipt.run_id}"))
    run.require(repository, "deploy-dev", receipt.run_attempt)
    tree, _ = tree_of(fetch, repository, run.head)
    if (run.event not in ("push", "workflow_dispatch") or run.head != receipt.deployment_sha
            or tree != receipt.source_tree):
        raise ValueError("development rollout does not match the tagged tree")
