// @vitest-environment node
import { describe, expect, it } from "vitest";

// The checker lives at the repository root because rubric R3 invokes it as
// `bun scripts/check-session-keeper-canary.ts`, and root `scripts/` is outside
// this package's vitest root. Importing across is the cost of keeping the
// script where its caller expects it; the alternative was a second make target
// with no distinct caller.
import {
  BROWSERS,
  COHORTS,
  REQUIRED_ATTEMPTS,
  SCENARIOS,
  checkSourceConsistency,
  gradeNumbers,
  validateShape,
  type CanaryReport,
  type Cell,
} from "../../../../scripts/check-session-keeper-canary";

/** A clean cell: full attempts, no failures, every recovery succeeded. */
function cell(overrides: Partial<Cell> = {}): Cell {
  return {
    cohort: "baseline",
    browser: "chromium",
    scenario: "session_lifetime_continuity",
    completed_attempts: REQUIRED_ATTEMPTS,
    unexpected_auth_failures: 0,
    recovery_required: 4,
    recovery_succeeded: 4,
    refresh_eligible: 10,
    duplicate_refreshes: 0,
    ...overrides,
  };
}

/** A full 2×3×5 matrix, optionally mutating one named cell. */
function report(mutate?: (c: Cell) => void, meta: Partial<CanaryReport["metadata"]> = {}): CanaryReport {
  const cells: Cell[] = [];
  for (const cohort of COHORTS) {
    for (const browser of BROWSERS) {
      for (const scenario of SCENARIOS) {
        cells.push(cell({ cohort, browser, scenario }));
      }
    }
  }
  if (mutate) for (const c of cells) mutate(c);
  return {
    schema_version: 1,
    metadata: {
      clerk_instance: "acme-dev.clerk.accounts.dev",
      clerk_instance_kind: "development",
      session_lifetime_seconds: 60,
      baseline_ref: "origin/main",
      candidate_ref: "HEAD",
      ...meta,
    },
    rollback_check: "pass",
    decision: "remove",
    cells,
  };
}

describe("canary report shape", () => {
  it("accepts a complete 2x3x5 matrix", () => {
    expect(validateShape(report())).toEqual([]);
  });

  it("rejects a cell short of the required attempts", () => {
    // Counts, not percentages: a short cell cannot be graded honestly, because
    // one failure in twenty moves a rate by five points.
    const r = report();
    r.cells[0]!.completed_attempts = 19;
    expect(validateShape(r).join()).toMatch(/completed_attempts 19 != 20/);
  });

  it("rejects a missing lane", () => {
    const r = report();
    r.cells = r.cells.filter((c) => c.browser !== "webkit");
    const reasons = validateShape(r).join();
    expect(reasons).toMatch(/missing cell .*webkit/);
  });

  it("requires metadata naming the instance and its configured lifetime", () => {
    // An expiry scenario graded against an unknown session lifetime measures
    // nothing, so the report is invalid without it.
    const noInstance = validateShape(report(undefined, { clerk_instance: "" })).join();
    expect(noInstance).toMatch(/clerk_instance is required/);
    const noLifetime = validateShape(report(undefined, { session_lifetime_seconds: 0 })).join();
    expect(noLifetime).toMatch(/session_lifetime_seconds must be a positive number/);
  });

  it("rejects a zero denominator carrying a non-zero numerator", () => {
    const r = report();
    r.cells[0]!.recovery_required = 0;
    r.cells[0]!.recovery_succeeded = 3;
    expect(validateShape(r).join()).toMatch(/recovery_succeeded 3 with zero recovery_required/);
  });
});

describe("canary decision rule", () => {
  it("earns remove when every candidate cell is clean", () => {
    const g = gradeNumbers(report());
    expect(g.valid).toBe(true);
    expect(g.valid && g.earned).toBe("remove");
  });

  it("abandons the comparison when the BASELINE arm is already broken", () => {
    // A non-zero baseline failure means the control was broken, so the result
    // is invalid rather than reinterpreted in the candidate's favour.
    const r = report((c) => {
      if (c.cohort === "baseline" && c.browser === "firefox") c.unexpected_auth_failures = 1;
    });
    const g = gradeNumbers(r);
    expect(g.valid).toBe(false);
    expect(g.reasons.join()).toMatch(/baseline has 1 unexpected auth failures; comparison abandoned/);
  });

  it("earns retain on a single candidate auth failure anywhere", () => {
    const r = report((c) => {
      if (c.cohort === "candidate" && c.scenario === "offline_online") c.unexpected_auth_failures = 1;
    });
    const g = gradeNumbers(r);
    expect(g.valid && g.earned).toBe("retain");
  });

  it("earns retain on a recovery shortfall, however small", () => {
    // 19/20 is a shortfall. "Nearly all" is not the rule.
    const r = report((c) => {
      if (c.cohort === "candidate" && c.browser === "webkit") c.recovery_succeeded = c.recovery_required - 1;
    });
    const g = gradeNumbers(r);
    expect(g.valid && g.earned).toBe("retain");
    expect(g.reasons.join()).toMatch(/candidate recovered 3\/4/);
  });

  it("earns retain when candidate duplicate refreshes exceed the matching baseline cell", () => {
    const r = report((c) => {
      if (c.cohort === "candidate" && c.browser === "chromium") c.duplicate_refreshes = 1;
    });
    const g = gradeNumbers(r);
    expect(g.valid && g.earned).toBe("retain");
    expect(g.reasons.join()).toMatch(/duplicate refreshes 1 > baseline 0/);
  });

  it("allows candidate duplicate refreshes equal to baseline", () => {
    // The rule is "no greater than", not "fewer than" — matching the baseline
    // exactly is not a regression.
    const r = report((c) => {
      c.duplicate_refreshes = 2;
    });
    const g = gradeNumbers(r);
    expect(g.valid).toBe(true);
    expect(g.valid && g.earned).toBe("remove");
  });
});

describe("verdict is bound to the tree", () => {
  const withRefs = (files: string[]) => (args: string[]) =>
    args[0] === "grep" ? files.join("\n") : "";
  const withDiff = (files: string[]) => (args: string[]) =>
    args[0] === "git" ? files.join("\n") : "";

  it("rejects remove while production references remain", () => {
    // The numbers can be perfect; a keeper still mounted makes remove a lie.
    const out = checkSourceConsistency("remove", withRefs(["ui/packages/app/app/layout.tsx"]), "origin/main");
    expect(out.join()).toMatch(/decision=remove but 1 production reference/);
  });

  it("accepts remove when no production reference survives", () => {
    // Test files do not count — they may legitimately outlive the component
    // for a release while the deletion is verified.
    const runner = withRefs(["ui/packages/app/lib/auth/client.test.tsx"]);
    expect(checkSourceConsistency("remove", runner, "origin/main")).toEqual([]);
  });

  it("rejects retain when the keeper was quietly edited", () => {
    const out = checkSourceConsistency("retain", withDiff(["ui/packages/app/lib/auth/client.ts"]), "origin/main");
    expect(out.join()).toMatch(/decision=retain but keeper files changed/);
  });

  it("accepts retain when the keeper and its mount are untouched", () => {
    expect(checkSourceConsistency("retain", withDiff([]), "origin/main")).toEqual([]);
  });
});
