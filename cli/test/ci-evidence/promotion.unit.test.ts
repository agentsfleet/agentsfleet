import { describe, expect, test } from "bun:test";
import { promotionSchema } from "@ci-evidence/promotion";
import { checkDevelopment } from "@ci-evidence/development";
import { gradeBehavior } from "@ci-evidence/grade";
import { EvidenceFixture } from "./fixture.ts";
import { AFTER, BASE, SOURCE } from "./facts.ts";
import { BUILD } from "./promotion-fixture.ts";

function check(f: EvidenceFixture): void {checkDevelopment(gradeBehavior(f.report, f.manifest));}
function proof(f: EvidenceFixture) {return promotionSchema.parse(f.json(f.development().runtime.evidence.promotion));}
function edit(f: EvidenceFixture, path: (string | number)[], value: unknown): void {f.edit(f.development().runtime.evidence.promotion, path, value);}
function editReceipt(f: EvidenceFixture, name: "candidate.json" | "development.json", path: (string | number)[], value: unknown): void {
  const p = proof(f), candidate = p.download.entries["candidate.json"], development = p.download.entries["development.json"];
  f.edit(p.download.entries[name], path, value);
  if (name === "candidate.json") f.edit(development, ["files", "candidate.json"], {sha256: candidate.sha256, size: Buffer.byteLength(f.read(candidate))});
  f.replace(f.development().runtime.evidence.promotion, p);
}

describe("promoted build identity", () => {
  test("accepts the original candidate stamp on a different, equal-tree deployment revision", () => {
    const f = new EvidenceFixture();
    expect(BUILD).not.toBe(SOURCE);
    expect(f.development().runtime.api_commit_stamp).toBe(BUILD.slice(0, 7));
    expect(() => check(f)).not.toThrow();
  });

  test.each(["merge_group", "workflow_dispatch"])("retains supported %s build identities", event => {
    const f = new EvidenceFixture();
    editReceipt(f, "candidate.json", ["source", "run_head_sha"], BUILD);
    edit(f, ["build_run", "response", "event"], event);
    edit(f, ["build_run", "response", "head_sha"], BUILD);
    expect(() => check(f)).not.toThrow();
  });

  test("uses original receipt bytes rather than JSON reserialization", () => {
    const f = new EvidenceFixture(), p = proof(f), candidate = p.download.entries["candidate.json"];
    f.replaceText(candidate, JSON.stringify(f.json(candidate), null, 2) + "\n");
    f.edit(p.download.entries["development.json"], ["files", "candidate.json"], {sha256: candidate.sha256, size: Buffer.byteLength(f.read(candidate))});
    f.replace(f.development().runtime.evidence.promotion, p);
    expect(() => check(f)).not.toThrow();
  });

  test("accepts a successful deployment retry after the candidate becomes available", () => {
    const f = new EvidenceFixture();
    f.edit(f.development().runtime.evidence.deployment, ["run", "response", "created_at"], "2026-01-01T00:00:00Z");
    f.edit(f.development().runtime.evidence.deployment, ["run", "response", "run_attempt"], 2);
    editReceipt(f, "development.json", ["run_attempt"], 2);
    f.edit(f.development().runtime.evidence.approval, ["at"], "2025-12-31T00:00:00Z");
    expect(() => check(f)).not.toThrow();
  });

  test("rejects a different native candidate tree while receipts and health stay consistent", () => {
    const f = new EvidenceFixture();
    edit(f, ["candidate_commit", "response", "tree", "sha"], BASE);
    expect(f.development().runtime.api_commit_stamp).toBe(BUILD.slice(0, 7));
    expect(() => check(f)).toThrow("native Git commit does not identify the retained source tree");
  });

  test.each<[(f: EvidenceFixture) => void]>([
    [f => edit(f, ["artifact", "response", "workflow_run", "id"], 999)],
    [f => edit(f, ["artifact", "response", "workflow_run", "head_sha"], BASE)],
    [f => edit(f, ["artifact", "read_url"], "https://api.github.com/repos/agentsfleet/agentsfleet/actions/artifacts/999")],
    [f => edit(f, ["artifact", "response", "name"], "unrelated-artifact")],
    [f => edit(f, ["artifact", "response", "expired"], true)],
    [f => edit(f, ["artifact", "response", "digest"], `sha256:${"0".repeat(64)}`)],
    [f => edit(f, ["download", "artifact_id"], 999)],
    [f => edit(f, ["download", "entries", "unexpected.json"], proof(f).download.entries["candidate.json"])],
    [f => editReceipt(f, "development.json", ["files", "candidate.json", "sha256"], "0".repeat(64))],
    [f => editReceipt(f, "development.json", ["files", "candidate.json", "size"], 1)],
    [f => editReceipt(f, "development.json", ["deployment_sha"], BASE)],
    [f => editReceipt(f, "development.json", ["source_tree"], BASE)],
    [f => editReceipt(f, "development.json", ["repository"], "other/project")],
    [f => editReceipt(f, "development.json", ["run_attempt"], 2)],
    [f => editReceipt(f, "development.json", ["gates", "metal"], "skipped")],
    [f => editReceipt(f, "development.json", ["gates", "unexpected"], "success")],
    [f => editReceipt(f, "development.json", ["files", "bin/agentsfleet-runner-linux-amd64", "size"], 124)],
    [f => editReceipt(f, "candidate.json", ["files", "bin/agentsfleet-runner-linux-amd64", "sha256"], "0".repeat(64))],
    [f => editReceipt(f, "candidate.json", ["files"], {})],
    [f => editReceipt(f, "candidate.json", ["files"], {"agentsfleet-runner-linux-amd64": {sha256: "d".repeat(64), size: 123}})],
    [f => editReceipt(f, "candidate.json", ["source", "repository"], "other/project")],
    [f => editReceipt(f, "candidate.json", ["source", "version"], "0.73.0")],
    [f => editReceipt(f, "candidate.json", ["source", "profile"], "dev")],
    [f => edit(f, ["candidate_commit", "read_url"], `https://api.github.com/repos/other/project/git/commits/${BUILD}`)],
    [f => edit(f, ["candidate_commit", "response", "sha"], BASE)],
    [f => edit(f, ["candidate_commit", "response", "parents"], [{sha: BASE}])],
    [f => edit(f, ["candidate_commit", "response", "parents", 1, "sha"], BASE)],
    [f => edit(f, ["deployment_commit", "response", "sha"], BASE)],
    [f => edit(f, ["build_run", "read_url"], "https://api.github.com/repos/agentsfleet/agentsfleet/actions/runs/999")],
    [f => edit(f, ["build_run", "response", "id"], 999)],
    [f => edit(f, ["build_run", "response", "run_attempt"], 1)],
    [f => edit(f, ["build_run", "response", "repository", "full_name"], "other/project")],
    [f => edit(f, ["build_run", "response", "head_repository", "full_name"], "other/project")],
    [f => edit(f, ["build_run", "response", "path"], ".github/workflows/unrelated.yml")],
    [f => edit(f, ["build_run", "response", "conclusion"], "failure")],
    [f => edit(f, ["build_run", "response", "head_sha"], BASE)],
    [f => edit(f, ["build_run", "response", "updated_at"], AFTER)],
    [f => edit(f, ["build_run", "response", "event"], "push")],
    [f => edit(f, ["build_run", "response", "event"], "merge_group")],
    [f => f.edit(f.development().runtime.evidence.deployment, ["run", "response", "event"], "pull_request")],
    [f => f.edit(f.development().runtime.evidence.deployment, ["run", "response", "run_attempt"], 2)],
    [f => f.edit(f.development().runtime.evidence.deployment, ["run", "response", "path"], ".github/workflows/test.yml")],
    [f => {f.development().runtime.api_commit_stamp = SOURCE.slice(0, 7); f.edit(f.development().runtime.evidence.health, ["response", "commit"], SOURCE.slice(0, 7));}],
  ])("rejects substituted promotion evidence %#", mutate => {
    const f = new EvidenceFixture(); mutate(f);
    expect(() => check(f)).toThrow("development.runtime");
  });
});
