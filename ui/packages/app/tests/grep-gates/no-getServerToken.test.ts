import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { resolve } from "node:path";

// I9.2 — `getServerToken` / `getServerAuth` / `getServerSessionMetadata`
// were deleted alongside `lib/auth/server.ts`. Every
// caller now hits `auth()` from `@clerk/nextjs/server` directly. If this
// grep test ever fails, a regression slipped a stale import or a copy-
// paste from before the migration into the tree.

const APP_ROOT = resolve(__dirname, "..", "..");

function grepHits(needle: string): string[] {
  // -r recursive, -n line numbers, -l names only, --include scopes by glob
  // We grep for the symbol body; exclude this very test file + the snapshot
  // of historical comments under docs/ and CHANGELOG-style notes.
  try {
    const out = execSync(
      `grep -rn --include='*.ts' --include='*.tsx' ` +
        `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
        `--exclude='no-getServerToken.test.ts' ` +
        `--exclude='no-api-template-mint.test.ts' ` +
        `--exclude='with-token.test.ts' ` +
        `-- '\\b${needle}\\b' .`,
      { cwd: APP_ROOT, encoding: "utf8" },
    );
    return out
      .split("\n")
      .filter((line) => line.trim().length > 0)
      // Strip purely-comment-mentioning lines — historical commit messages
      // and migration-note comments are allowed; only LIVE code references
      // are violations.
      .filter((line) => !/^[^:]+:\d+:\s*(\/\/|\*|\/\*)/.test(line));
  } catch (err) {
    // grep exits 1 when no matches found — that's the green path.
    const exitCode = (err as { status?: number }).status;
    if (exitCode === 1) return [];
    throw err;
  }
}

describe("I9.2 — getServerToken family fully retired", () => {
  it("getServerToken has zero live call sites", () => {
    const hits = grepHits("getServerToken");
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("getServerAuth has zero live call sites", () => {
    const hits = grepHits("getServerAuth");
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("getServerSessionMetadata has zero live call sites", () => {
    const hits = grepHits("getServerSessionMetadata");
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("lib/auth/server import path has zero references", () => {
    // Match the exact deleted module path. Quote the slashes carefully —
    // grep -F not used because we need word boundaries on the symbol set.
    try {
      const out = execSync(
        `grep -rn --include='*.ts' --include='*.tsx' ` +
          `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
          `--exclude='no-getServerToken.test.ts' ` +
          `-- '@/lib/auth/server' .`,
        { cwd: APP_ROOT, encoding: "utf8" },
      );
      const lines = out
        .split("\n")
        .filter((line) => line.trim().length > 0)
        .filter((line) => !/^[^:]+:\d+:\s*(\/\/|\*|\/\*)/.test(line));
      expect(lines, lines.join("\n")).toEqual([]);
    } catch (err) {
      if ((err as { status?: number }).status === 1) return; // no matches
      throw err;
    }
  });
});
