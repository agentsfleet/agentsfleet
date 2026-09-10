import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { resolve } from "node:path";

// I9.2 — `getServerToken` / `getServerAuth` / `getServerSessionMetadata`
// were deleted alongside `lib/auth/server.ts`. If this grep test ever fails, a
// regression slipped a stale import or a copy-paste from before the migration
// into the tree.
//
// Callers no longer reach the identity provider directly either: `auth()` moved
// behind `lib/auth/credential.ts`, and the second test below pins that boundary
// so the next page cannot quietly reopen it.

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

describe("the identity provider stays behind one module", () => {
  // 27 files imported `auth` from the provider's server SDK. Every one wanted a
  // bearer; none read a user id or an organisation. `credential.ts` is the
  // boundary that replaced them, and a boundary nothing pins is a convention.
  //
  // `proxy.ts` is the documented second importer and is irreducible: the
  // middleware IS the provider's session verification, and it takes
  // `clerkMiddleware`, never `auth`.
  const ALLOWED = ["lib/auth/credential.ts", "proxy.ts"];

  it("is imported by exactly the two files that must import it", () => {
    const out = execSync(
      `grep -rl --include='*.ts' --include='*.tsx' ` +
        `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
        `--exclude-dir=tests --exclude='*.test.ts' --exclude='*.test.tsx' ` +
        `'from "@clerk/nextjs/server"' app lib components proxy.ts || true`,
      { cwd: APP_ROOT, encoding: "utf8" },
    );
    const importers = out.split("\n").filter(Boolean).sort();
    expect(importers).toEqual([...ALLOWED].sort());
  });

  // The narrower claim, and the one that actually decays: `auth()` itself.
  it("hands out auth() from credential.ts alone", () => {
    const out = execSync(
      `grep -rl --include='*.ts' --include='*.tsx' ` +
        `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
        `--exclude-dir=tests --exclude='*.test.ts' --exclude='*.test.tsx' ` +
        `'import { auth }' app lib components || true`,
      { cwd: APP_ROOT, encoding: "utf8" },
    );
    expect(out.split("\n").filter(Boolean)).toEqual(["lib/auth/credential.ts"]);
  });
});
