import { posix } from "node:path";
import type { Report } from "./schema.ts";
import { reportSchema } from "./schema.ts";
import { EvidenceError, canonical, decode, requireProof } from "./records.ts";
import { campaignChanged } from "./grade.ts";

export const REPORT_PATH = posix.join("playbooks/operations/acceptance/baselines", "M224_001-ci-evidence.json");
export const REPORT_MAX_BYTES = 16 * 1024 * 1024;
const GIT_TIMEOUT_MS = 10_000;

export function checkHistory(report: Report, root: string): void {
  const commits = git(root, ["log", "--format=%H", "--reverse", "--", REPORT_PATH]).trim().split("\n").filter(Boolean);
  const previous: {commit: string; report: Report}[] = [];
  for (const commit of commits) {
    let value: unknown;
    try {value = JSON.parse(git(root, ["show", `${commit}:${REPORT_PATH}`]));}
    catch (error) {
      if (error instanceof EvidenceError) throw error;
      throw new EvidenceError("previous_report_commits", "retained report is not JSON", 2);
    }
    const retained = decode(reportSchema, value, "previous_report_commits");
    if (retained.evaluation_id === report.evaluation_id && canonical(retained) === canonical(report)) continue;
    previous.push({commit, report: retained});
  }
  requireProof(canonical(previous.map(row => row.commit)) === canonical(report.previous_report_commits), "previous_report_commits", "committed report history was omitted or replaced");
  const campaigns = new Set([report.evaluation_id]);
  const events = new Set(report.cases.map(subject => subject.event_id));
  for (let index = 0; index < previous.length; index++) {
    const row = previous[index];
    requireProof(row, "previous_report_commits", "retained report is missing");
    requireProof(!campaigns.has(row.report.evaluation_id), "previous_report_commits", "campaign label was reused");
    campaigns.add(row.report.evaluation_id);
    requireProof(canonical(row.report.previous_report_commits) === canonical(previous.slice(0, index).map(row => row.commit)), "previous_report_commits", "retained history chain is broken");
    requireProof(row.report.cases.every(subject => !events.has(subject.event_id)), "previous_report_commits", "cases were imported from an earlier campaign");
    for (const subject of row.report.cases) events.add(subject.event_id);
  }
  const last = previous.at(-1);
  requireProof(!last || campaignChanged(last.report, report), "previous_report_commits", "unchanged retry cannot reset an earlier grade");
}

/** An explicit repository root cannot be redirected by a calling Git hook. */
export function gitEnvironment(): NodeJS.ProcessEnv {
  return Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("GIT_")));
}

function git(root: string, args: string[]): string {
  try {
    const result = Bun.spawnSync(["git", ...args], {cwd: root, env: gitEnvironment(), stdin: "ignore", stdout: "pipe", stderr: "pipe", timeout: GIT_TIMEOUT_MS, maxBuffer: REPORT_MAX_BYTES});
    if (result.exitCode !== 0 || result.signalCode) throw new Error("Git evidence unavailable");
    return result.stdout.toString();
  }
  catch {throw new EvidenceError("previous_report_commits", "fixed-path Git evidence is unavailable", 2);}
}
