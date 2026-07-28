#!/usr/bin/env bun
/**
 * Grade a captured session-keeper canary and bind its verdict to the tree.
 *
 * The question this answers is narrow: may `AuthSessionKeeper` be deleted?
 * It exists because Clerk session tokens live about a minute and a long
 * dashboard journey could otherwise POST a Server Action after the cookie
 * expired. If Clerk's own SDK now keeps the cookie fresh, the keeper is a
 * second timer doing nothing. "Probably fine" is not evidence, so removal
 * requires a measured comparison across genuine browsers.
 *
 * TWO things are checked, and BOTH must agree:
 *   1. the numbers earn the verdict, and
 *   2. the tree matches the verdict — `remove` with the keeper still mounted,
 *      or `retain` with it deleted, is a lie regardless of the numbers.
 *
 * `retain` is always available and is NOT a failure. If the capture cannot be
 * provisioned the honest outcome is `retain` with a reason, never a threshold
 * quietly loosened until `remove` fits.
 *
 * Usage:
 *   bun scripts/check-session-keeper-canary.ts \
 *     --input test-results/session-keeper-canary.json --base origin/main
 */

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

/** Verdicts. Const object rather than bare literals so narrowing survives. */
export const DECISION = { remove: "remove", retain: "retain" } as const;
export type Decision = (typeof DECISION)[keyof typeof DECISION];

export const COHORT = { baseline: "baseline", candidate: "candidate" } as const;
export const COHORTS = [COHORT.baseline, COHORT.candidate] as const;
export const BROWSERS = ["chromium", "firefox", "webkit"] as const;
export const SCENARIOS = [
  "session_lifetime_continuity",
  "background_expiry",
  "offline_online",
  "focus_restoration",
  "resumed_server_action",
] as const;

/**
 * Attempts required per cell.
 *
 * The rule is expressed in COUNTS, not percentages, because 20 samples cannot
 * resolve a percentage point: one failure in a 20-attempt cell moves a rate by
 * five points, so a "+1.0pp" threshold would be finer than the instrument and
 * could never be graded honestly.
 */
export const REQUIRED_ATTEMPTS = 20;

/** Production files whose presence or absence the verdict is bound to. */
const KEEPER_SOURCE = "ui/packages/app/lib/auth/client.ts";
const KEEPER_MOUNT = "ui/packages/app/app/layout.tsx";
const KEEPER_SYMBOL = "AuthSessionKeeper";

/** The only `rollback_check` value that clears the gate. */
const ROLLBACK_PASS = "pass";
const FILE_ENCODING = "utf8";
const NEWLINE = "\n";
const LIST_SEPARATOR = ", ";

export type Cell = {
  cohort: (typeof COHORTS)[number];
  browser: (typeof BROWSERS)[number];
  scenario: (typeof SCENARIOS)[number];
  completed_attempts: number;
  unexpected_auth_failures: number;
  recovery_required: number;
  recovery_succeeded: number;
  refresh_eligible: number;
  duplicate_refreshes: number;
};

export type CanaryReport = {
  schema_version: 1;
  metadata: {
    clerk_instance: string;
    clerk_instance_kind: string;
    session_lifetime_seconds: number;
    baseline_ref: string;
    candidate_ref: string;
  };
  rollback_check: string;
  decision: Decision;
  retain_reason?: string;
  cells: Cell[];
};

export type Grade =
  | { valid: false; reasons: string[] }
  | { valid: true; earned: Decision; reasons: string[] };

const key = (c: string, b: string, s: string) => `${c}/${b}/${s}`;

/**
 * Structural validity. A report that cannot be graded honestly is INVALID
 * rather than a quiet `retain` — the difference between "we measured and the
 * keeper stays" and "we never measured" is exactly what this file protects.
 */
export function validateShape(report: CanaryReport): string[] {
  const reasons: string[] = [];
  if (report.schema_version !== 1) reasons.push(`schema_version must be 1`);

  const meta = report.metadata;
  // Without these the report cannot be reproduced or trusted: an expiry
  // scenario graded against an unknown session lifetime measures nothing.
  if (!meta?.clerk_instance) reasons.push("metadata.clerk_instance is required");
  if (!meta?.clerk_instance_kind) reasons.push("metadata.clerk_instance_kind is required");
  if (!(meta?.session_lifetime_seconds > 0)) {
    reasons.push("metadata.session_lifetime_seconds must be a positive number");
  }

  const seen = new Map<string, Cell>();
  for (const cell of report.cells ?? []) seen.set(key(cell.cohort, cell.browser, cell.scenario), cell);

  for (const cohort of COHORTS) {
    for (const browser of BROWSERS) {
      for (const scenario of SCENARIOS) {
        const id = key(cohort, browser, scenario);
        const cell = seen.get(id);
        if (!cell) {
          reasons.push(`missing cell ${id}`);
          continue;
        }
        if (cell.completed_attempts !== REQUIRED_ATTEMPTS) {
          reasons.push(`${id}: completed_attempts ${cell.completed_attempts} != ${REQUIRED_ATTEMPTS}`);
        }
        reasons.push(...validateDenominators(id, cell));
      }
    }
  }
  return reasons;
}

/**
 * A zero denominator passes only when its numerator is also zero. Otherwise
 * the cell claims successes it had no opportunities for, which is a broken
 * capture rather than a good result.
 */
function validateDenominators(id: string, cell: Cell): string[] {
  const reasons: string[] = [];
  if (cell.recovery_required === 0 && cell.recovery_succeeded !== 0) {
    reasons.push(`${id}: recovery_succeeded ${cell.recovery_succeeded} with zero recovery_required`);
  }
  if (cell.recovery_succeeded > cell.recovery_required) {
    reasons.push(`${id}: recovery_succeeded exceeds recovery_required`);
  }
  if (cell.refresh_eligible === 0 && cell.duplicate_refreshes !== 0) {
    reasons.push(`${id}: duplicate_refreshes ${cell.duplicate_refreshes} with zero refresh_eligible`);
  }
  return reasons;
}

/**
 * What the numbers earn, independent of what the report claims.
 *
 * `remove` requires, in the candidate cohort and across EVERY lane and
 * scenario: zero unexpected auth failures against a baseline that is also
 * zero; every recovery-required attempt recovered; and duplicate refreshes no
 * greater than the matching baseline cell in absolute count.
 */
export function gradeNumbers(report: CanaryReport): Grade {
  const shape = validateShape(report);
  if (shape.length > 0) return { valid: false, reasons: shape };

  const cells = new Map<string, Cell>();
  for (const cell of report.cells) cells.set(key(cell.cohort, cell.browser, cell.scenario), cell);

  const reasons: string[] = [];
  let earned: Decision = DECISION.remove;

  for (const browser of BROWSERS) {
    for (const scenario of SCENARIOS) {
      const base = cells.get(key(COHORT.baseline, browser, scenario))!;
      const cand = cells.get(key(COHORT.candidate, browser, scenario))!;
      const id = `${browser}/${scenario}`;

      // A non-zero BASELINE failure means the control arm was already broken,
      // so the comparison is abandoned rather than reinterpreted in the
      // candidate's favour.
      if (base.unexpected_auth_failures !== 0) {
        return {
          valid: false,
          reasons: [`${id}: baseline has ${base.unexpected_auth_failures} unexpected auth failures; comparison abandoned`],
        };
      }
      if (cand.unexpected_auth_failures !== 0) {
        reasons.push(`${id}: candidate had ${cand.unexpected_auth_failures} unexpected auth failures`);
        earned = DECISION.retain;
      }
      if (cand.recovery_succeeded !== cand.recovery_required) {
        reasons.push(`${id}: candidate recovered ${cand.recovery_succeeded}/${cand.recovery_required}`);
        earned = DECISION.retain;
      }
      if (cand.duplicate_refreshes > base.duplicate_refreshes) {
        reasons.push(`${id}: candidate duplicate refreshes ${cand.duplicate_refreshes} > baseline ${base.duplicate_refreshes}`);
        earned = DECISION.retain;
      }
    }
  }
  return { valid: true, earned, reasons };
}

/** Production references to the keeper symbol, excluding tests. */
export function keeperReferences(runner: (args: string[]) => string): string[] {
  const out = runner(["grep", "-rln", KEEPER_SYMBOL, "ui/packages/app"]);
  return out
    .split(NEWLINE)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.includes(".test."));
}

/** Files touching the keeper that differ from the comparison base. */
export function keeperDiff(runner: (args: string[]) => string, base: string): string[] {
  const out = runner(["git", "diff", "--name-only", base, "--", KEEPER_SOURCE, KEEPER_MOUNT]);
  return out.split(NEWLINE).map((l) => l.trim()).filter((l) => l.length > 0);
}

/**
 * Bind the claimed decision to what the tree actually looks like. A `remove`
 * with the keeper still mounted, or a `retain` whose keeper was quietly
 * edited, fails here no matter how good the numbers were.
 */
export function checkSourceConsistency(
  decision: Decision,
  runner: (args: string[]) => string,
  base: string,
): string[] {
  if (decision === DECISION.remove) {
    const refs = keeperReferences(runner);
    return refs.length === 0 ? [] : [`decision=remove but ${refs.length} production reference(s) remain: ${refs.join(LIST_SEPARATOR)}`];
  }
  const diff = keeperDiff(runner, base);
  return diff.length === 0 ? [] : [`decision=retain but keeper files changed vs ${base}: ${diff.join(LIST_SEPARATOR)}`];
}

function shell(args: string[]): string {
  try {
    return execFileSync(args[0]!, args.slice(1), { encoding: FILE_ENCODING });
  } catch (e) {
    // grep exits 1 on "no matches", which is a legitimate zero-reference
    // answer rather than a tool failure.
    const status = (e as { status?: number }).status;
    if (status === 1) return "";
    throw e;
  }
}

function argValue(argv: string[], flag: string): string | undefined {
  const i = argv.indexOf(flag);
  return i >= 0 ? argv[i + 1] : undefined;
}

export function main(argv: string[], runner: (args: string[]) => string = shell): number {
  const input = argValue(argv, "--input");
  const base = argValue(argv, "--base") ?? "origin/main";
  if (!input) {
    console.error("usage: check-session-keeper-canary.ts --input <report.json> [--base <ref>]");
    return 2;
  }

  const report = JSON.parse(readFileSync(input, FILE_ENCODING)) as CanaryReport;
  const grade = gradeNumbers(report);

  if (!grade.valid) {
    console.error("INVALID report — cannot be graded honestly:");
    for (const r of grade.reasons) console.error(`  - ${r}`);
    return 1;
  }

  // A report may claim `retain` when the numbers would have earned `remove`
  // (capture unprovisionable, or an owner decision). It may never claim
  // `remove` the numbers did not earn.
  if (report.decision === DECISION.remove && grade.earned !== DECISION.remove) {
    console.error("decision=remove but the numbers did not earn it:");
    for (const r of grade.reasons) console.error(`  - ${r}`);
    return 1;
  }
  if (report.rollback_check !== ROLLBACK_PASS) {
    console.error(`rollback_check=${report.rollback_check} (expected "${ROLLBACK_PASS}")`);
    return 1;
  }

  const mismatches = checkSourceConsistency(report.decision, runner, base);
  if (mismatches.length > 0) {
    for (const m of mismatches) console.error(m);
    return 1;
  }

  const meta = report.metadata;
  console.log(
    `decision=${report.decision} rollback_check=${ROLLBACK_PASS} ` +
      `instance=${meta.clerk_instance} (${meta.clerk_instance_kind}) ` +
      `session_lifetime_seconds=${meta.session_lifetime_seconds}`,
  );
  if (report.decision === DECISION.retain) {
    console.log(`retain reason: ${report.retain_reason ?? "numbers did not earn removal"}`);
  }
  return 0;
}

if (import.meta.main) process.exit(main(process.argv.slice(2)));
