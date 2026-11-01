import { afterEach, describe, expect, test } from "bun:test";
import { writeFileSync } from "node:fs";
import { runEvidenceCommand } from "@ci-evidence/command";
import { REPORT_MAX_BYTES } from "@ci-evidence/history";
import { EvidenceFixture } from "./fixture.ts";
import { EvidenceRepository } from "./repository.ts";

const repositories: EvidenceRepository[] = [];
afterEach(() => {for (const repository of repositories.splice(0)) repository.dispose();});
function repository(): EvidenceRepository {const value = new EvidenceRepository(); repositories.push(value); return value;}

async function run(repo: EvidenceRepository, section = "behavior") {
  const lines: string[] = [];
  const status = await runEvidenceCommand(["--report", repo.path, "--section", section], {root: repo.root, output: line => lines.push(line)});
  return {status, lines};
}

describe("offline evidence command", () => {
  test.each(["grounding", "behavior", "development"])("reports a complete synthetic %s result", async section => {
    const repo = repository(); repo.write(new EvidenceFixture());
    const result = await run(repo, section);
    expect(result.status).toBe(0); expect(result.lines).toEqual([section === "development" ? "development=PASS" : `${section}=8/8; ${section === "grounding" ? "critical_failures" : "worst_case_failures"}=0`]);
  });

  test("failed grades use exit one and keep the fixed denominator", async () => {
    const repo = repository(), fixture = new EvidenceFixture(); fixture.subject().review.usefulness = "fail"; repo.write(fixture);
    expect(await run(repo)).toEqual({status: 1, lines: ["behavior=7/8; worst_case_failures=1"]});
    expect((await run(repo, "development")).status).toBe(1);
  });

  test("requires all four arguments with an allowed section", async () => {
    const lines: string[] = [];
    expect(await runEvidenceCommand(["--section", "grounding"], {output: line => lines.push(line)})).toBe(2);
    expect(lines[0]).toStartWith("usage:");
  });

  test("missing and malformed files never echo private text or paths", async () => {
    const repo = repository();
    expect(await run(repo)).toEqual({status: 2, lines: ["report: input is missing or malformed JSON"]});
    writeFileSync(repo.path, '{"private-input":"do not echo this"');
    expect(await run(repo)).toEqual({status: 2, lines: ["report: input is missing or malformed JSON"]});
    repo.write(new EvidenceFixture()); writeFileSync(repo.manifest, "{");
    expect(await run(repo)).toEqual({status: 2, lines: ["manifest: input is missing or malformed JSON"]});
  });

  test("rejects input above the retained witness budget before parsing", async () => {
    const repo = repository(); writeFileSync(repo.path, " ".repeat(REPORT_MAX_BYTES + 1));
    expect(await run(repo)).toEqual({status: 2, lines: ["report: input exceeds 16 MiB"]});
  });

  test("sanitizes unexpected failures when deeply nested input cannot be validated", async () => {
    const repo = repository(), fixture = new EvidenceFixture(); repo.write(fixture);
    const text = JSON.stringify(fixture.report).replace('"inference":{"api":"chat"', `"inference":{"deep":${"[".repeat(10000)}0${"]".repeat(10000)},"api":"chat"`);
    writeFileSync(repo.path, text);
    expect(await run(repo)).toEqual({status: 2, lines: ["input: evidence could not be read or validated"]});
  });
});
