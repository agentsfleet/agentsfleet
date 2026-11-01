import type { EvidenceFixture } from "./fixture.ts";
import type { Ref } from "@ci-evidence/schema";
import { BEFORE, BASE, HEAD, SOURCE } from "./facts.ts";

export const BUILD = "d".repeat(40);
export const TREE = "e".repeat(40);
const API = "https://api.github.com/repos/agentsfleet/agentsfleet";
const repository = {full_name: "agentsfleet/agentsfleet"};

export function deploymentRun() {
  return {id: 123, run_attempt: 1, head_sha: SOURCE, path: ".github/workflows/deploy-dev.yml", event: "push",
    conclusion: "success", repository, head_repository: repository, created_at: BEFORE, run_started_at: BEFORE, updated_at: BEFORE};
}

/** Both immutable receipts retain their original bytes, independent of the merge commit. */
export function promotionFixture(fixture: EvidenceFixture, binarySha: string): Ref {
  const runner = {sha256: binarySha, size: 123};
  const candidate = fixture.add({schema: 1, source: {repository: repository.full_name, version: "0.72.0", source_sha: BUILD,
    source_tree: TREE, run_head_sha: HEAD, run_id: 111, run_attempt: 2, profile: "dist"}, files: {"bin/agentsfleet-runner-linux-amd64": runner}});
  const development = fixture.add({schema: 1, repository: repository.full_name, source_tree: TREE, deployment_sha: SOURCE,
    run_id: 123, run_attempt: 1, image: `ghcr.io/agentsfleet/agentsfleetd@sha256:${"f".repeat(64)}`,
    files: {"candidate.json": {sha256: candidate.sha256, size: Buffer.byteLength(fixture.read(candidate))}, "bin/agentsfleet-runner-linux-amd64": runner},
    gates: {fly: "success", metal: "success", dashboard: "success", verify: "success", qa: "success", acceptance: "success", cli: "success"}});
  return fixture.add({download: {artifact_id: 789, archive_sha256: "f".repeat(64),
    entries: {"candidate.json": candidate, "development.json": development}},
    artifact: {read_url: `${API}/actions/artifacts/789`, response: {id: 789, name: `development-candidate-${TREE}`, expired: false,
      digest: `sha256:${"f".repeat(64)}`, workflow_run: {id: 123, head_sha: SOURCE}}},
    build_run: {read_url: `${API}/actions/runs/111`, response: {...deploymentRun(), id: 111, run_attempt: 2,
      head_sha: HEAD, path: ".github/workflows/test.yml", event: "pull_request"}},
    candidate_commit: {read_url: `${API}/git/commits/${BUILD}`, response: {sha: BUILD, tree: {sha: TREE}, parents: [{sha: BASE}, {sha: HEAD}]}},
    deployment_commit: {read_url: `${API}/git/commits/${SOURCE}`, response: {sha: SOURCE, tree: {sha: TREE}, parents: [{sha: BASE}, {sha: HEAD}]}},
  });
}
