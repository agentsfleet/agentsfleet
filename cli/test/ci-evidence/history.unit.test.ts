import { afterEach, describe, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { checkHistory } from "@ci-evidence/history";
import { campaignChanged } from "@ci-evidence/grade";
import { EvidenceFixture } from "./fixture.ts";
import { EvidenceRepository } from "./repository.ts";

const repositories: EvidenceRepository[] = [];
afterEach(() => {for (const repository of repositories.splice(0)) repository.dispose();});
function repository(): EvidenceRepository {const value = new EvidenceRepository(); repositories.push(value); return value;}

describe("fixed-path retained campaign history", () => {
  test("a stalled Git process is terminated at the evidence-read deadline", async () => {
    const repo = repository(), bin = join(repo.root, "bin"), started = join(repo.root, "git-started");
    mkdirSync(bin);
    const executable = join(bin, "git");
    await Bun.write(executable, `#!${process.execPath}\nawait Bun.write(${JSON.stringify(started)}, "started");\nawait new Promise(resolve => setTimeout(resolve, 30000));\n`);
    chmodSync(executable, 0o755);
    const script = join(repo.root, "verify.ts"), report = join(repo.root, "report.json");
    const checker = new URL("../../../scripts/ci-evidence/history.ts", import.meta.url).href;
    await Bun.write(report, JSON.stringify(new EvidenceFixture().report));
    await Bun.write(script, `import {checkHistory} from ${JSON.stringify(checker)};\ncheckHistory(await Bun.file(process.argv[2]).json(), process.argv[3]);\n`);
    const entered = Date.now();
    const result = Bun.spawnSync([process.execPath, script, report, repo.root], {
      env: {...process.env, PATH: `${bin}:${process.env.PATH ?? ""}`},
      stdout: "pipe", stderr: "pipe", timeout: 25000,
    });
    expect(result.exitCode).toBe(1);
    expect(result.signalCode).toBeUndefined();
    expect(result.stderr.toString()).toContain("Git evidence is unavailable");
    expect(await Bun.file(started).exists()).toBe(true);
    expect(Date.now() - entered).toBeGreaterThanOrEqual(8000);
    expect(Date.now() - entered).toBeLessThan(25000);
  }, 35000);

  test("hook Git variables cannot redirect scratch writes or fixed-root history reads", async () => {
    const protectedRepo = repository(), fixture = new EvidenceFixture();
    protectedRepo.write(fixture); protectedRepo.commit();
    const head = protectedRepo.git("rev-parse", "HEAD");
    const script = join(protectedRepo.root, "hook.ts"), report = join(protectedRepo.root, "input.json");
    const helper = new URL("./repository.ts", import.meta.url).href;
    const checker = new URL("../../../scripts/ci-evidence/history.ts", import.meta.url).href;
    await Bun.write(report, JSON.stringify(fixture.report));
    await Bun.write(script, `import {EvidenceRepository} from ${JSON.stringify(helper)};\nimport {checkHistory} from ${JSON.stringify(checker)};\nconst scratch = new EvidenceRepository();\ntry {if (scratch.git("rev-parse", "--show-toplevel") !== scratch.root) throw new Error("wrong scratch root"); checkHistory(await Bun.file(process.argv[2]).json(), scratch.root);} finally {scratch.dispose();}\n`);
    const index = join(protectedRepo.root, "foreign-index");
    const result = Bun.spawnSync([process.execPath, script, report], {stdout: "pipe", stderr: "pipe", timeout: 10000,
      env: {...process.env, GIT_DIR: join(protectedRepo.root, ".git"), GIT_WORK_TREE: protectedRepo.root, GIT_INDEX_FILE: index}});
    expect(result.exitCode).toBe(0);
    expect(result.signalCode).toBeUndefined();
    expect(protectedRepo.git("rev-parse", "HEAD")).toBe(head);
    expect(existsSync(index)).toBe(false);
  });

  test("accepts a first campaign and its exact committed copy", () => {
    const repo = repository(), fixture = new EvidenceFixture();
    expect(() => checkHistory(fixture.report, repo.root)).not.toThrow();
    repo.write(fixture); repo.commit();
    expect(() => checkHistory(fixture.report, repo.root)).not.toThrow();
  });

  test("retains a failed campaign before accepting a changed correction", () => {
    const repo = repository(), before = new EvidenceFixture("failed-first");
    before.subject().review.stopping_scope = "fail";
    repo.write(before); const commit = repo.commit();
    const after = new EvidenceFixture("corrected", 1000); after.report.previous_report_commits = [commit];
    expect(() => checkHistory(after.report, repo.root)).not.toThrow();
    after.report.previous_report_commits = [];
    expect(() => checkHistory(after.report, repo.root)).toThrow("omitted or replaced");
  });

  test("rejects event reuse, label reuse and unchanged retry resets", () => {
    const repo = repository(), before = new EvidenceFixture(); repo.write(before); const commit = repo.commit();
    const repeated = new EvidenceFixture("different-label"); repeated.report.previous_report_commits = [commit];
    expect(() => checkHistory(repeated.report, repo.root)).toThrow("imported from an earlier");
    const renamed = new EvidenceFixture(before.report.evaluation_id, 1000); renamed.report.previous_report_commits = [commit];
    expect(() => checkHistory(renamed.report, repo.root)).toThrow("label was reused");
    const retry = new EvidenceFixture("unchanged-retry", 1000); retry.report.previous_report_commits = [commit];
    retry.report.corpus_sha256 = before.report.corpus_sha256;
    expect(() => checkHistory(retry.report, repo.root)).toThrow("unchanged retry");
  });

  test("checks the full ordered history chain", () => {
    const repo = repository(), first = new EvidenceFixture("first"); repo.write(first); const one = repo.commit();
    const second = new EvidenceFixture("second", 1000); second.report.previous_report_commits = [one]; repo.write(second); const two = repo.commit();
    const third = new EvidenceFixture("third", 2000); third.report.previous_report_commits = [one, two];
    expect(() => checkHistory(third.report, repo.root)).not.toThrow();
    third.report.previous_report_commits.reverse();
    expect(() => checkHistory(third.report, repo.root)).toThrow("omitted or replaced");
  });

  test("rejects a retained report with a broken predecessor chain", () => {
    const repo = repository(), first = new EvidenceFixture("broken"); first.report.previous_report_commits = ["a".repeat(40)];
    repo.write(first); const commit = repo.commit();
    const after = new EvidenceFixture("after", 1000); after.report.previous_report_commits = [commit];
    expect(() => checkHistory(after.report, repo.root)).toThrow("history chain is broken");
  });

  test("does not read report-selected paths or revisions outside fixed-path Git history", () => {
    const repo = repository(), fixture = new EvidenceFixture();
    fixture.report.previous_report_commits = ["a".repeat(40)];
    expect(() => checkHistory(fixture.report, repo.root)).toThrow("omitted or replaced");
    expect(() => checkHistory(fixture.report, "/not-an-evidence-repository")).toThrow("Git evidence is unavailable");
  });

  test.each(["{", '{"schema_version":99}'])("rejects malformed committed evidence %s", text => {
    const repo = repository(); writeFileSync(repo.path, text); repo.commit();
    expect(() => checkHistory(new EvidenceFixture().report, repo.root)).toThrow("previous_report_commits");
  });

  test("changes are determined from corpus, instruction and selected-model content", () => {
    const before = new EvidenceFixture().report;
    const after = structuredClone(before); after.evaluation_id = "new-label";
    expect(campaignChanged(before, after)).toBe(false);
    after.instructions_sha256.responder = "0".repeat(64); expect(campaignChanged(before, after)).toBe(true);
    after.instructions_sha256 = before.instructions_sha256; after.model.id = "different-model";
    expect(campaignChanged(before, after)).toBe(true);
  });
});
