import { describe, expect, it } from "vitest";
import { execSync } from "node:child_process";
import { resolve } from "node:path";

// I9.1 — the api-template JWT mint (`getToken({ template: "api" })`) is
// retired everywhere except the one carve-out site that the CLI handoff
// still depends on: `app/cli-auth/[session_id]/page.tsx`. The dashboard's
// own traffic now rides the customized default session token configured
// in Clerk's Session Token Customization. If anything
// else mints `template: "api"`, this gate fails.
//
// Allowlisted carve-out: `app/cli-auth/[session_id]/page.tsx` — see that
// file for the full rationale in the carve-out comment block.

const APP_ROOT = resolve(__dirname, "..", "..");
const CARVE_OUT = "app/cli-auth/[session_id]/page.tsx";

describe("I9.1 — api-template mint is carve-out-only", () => {
  it("only the CLI handoff page contains `template: \"api\"`", () => {
    let out = "";
    try {
      out = execSync(
        `grep -rn --include='*.ts' --include='*.tsx' ` +
          `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
          `--exclude='no-api-template-mint.test.ts' ` +
          `-- 'template: *"api"' .`,
        { cwd: APP_ROOT, encoding: "utf8" },
      );
    } catch (err) {
      if ((err as { status?: number }).status === 1) {
        // grep found nothing — including the carve-out itself. That means
        // the carve-out site was edited away, which is also a regression.
        throw new Error(
          `expected the carve-out at ${CARVE_OUT} to retain the api-template mint, but grep matched 0 lines`,
        );
      }
      throw err;
    }

    const hits = out
      .split("\n")
      .filter((line) => line.trim().length > 0)
      // Comments are allowed — only live code uses are violations.
      .filter((line) => !/^[^:]+:\d+:\s*(\/\/|\*|\/\*)/.test(line));

    const offenders = hits.filter((line) => {
      const path = line.split(":", 1)[0] ?? "";
      // Normalize Windows separators just in case CI ever runs on Windows.
      const normalized = path.replace(/\\/g, "/");
      return !normalized.endsWith(CARVE_OUT);
    });

    expect(offenders, offenders.join("\n")).toEqual([]);

    // Belt-and-braces: confirm the carve-out still has at least one hit.
    const carveOutHits = hits.filter((line) => {
      const path = (line.split(":", 1)[0] ?? "").replace(/\\/g, "/");
      return path.endsWith(CARVE_OUT);
    });
    expect(carveOutHits.length, "carve-out site lost its api-template mint").toBeGreaterThan(0);
  });
});

// The browser holds no dashboard credential: no `"use client"` component
// calls `getToken()`, constructs an `Authorization: Bearer` header, or
// declares/receives a token-typed prop (a prop on a client component is
// serialized into the hydration payload). Reads run in Server Components
// and mutations in Server Actions, both server-side. The single allowed
// exception is the CLI-handoff carve-out — the same site the api-template
// gate above protects — so it is excluded here too. Test files are skipped
// (fixtures may reference tokens without being a production client surface).

function clientFiles(): string[] {
  try {
    const out = execSync(
      `grep -rl --include='*.ts' --include='*.tsx' ` +
        `--exclude-dir=node_modules --exclude-dir=.next --exclude-dir=dist ` +
        `--exclude='*.test.ts' --exclude='*.test.tsx' ` +
        `-- 'use client' .`,
      { cwd: APP_ROOT, encoding: "utf8" },
    );
    return out
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l.length > 0)
      .filter((f) => !f.replace(/\\/g, "/").endsWith(CARVE_OUT));
  } catch (err) {
    if ((err as { status?: number }).status === 1) return [];
    throw err;
  }
}

function scanClientFiles(pattern: string): string[] {
  const files = clientFiles();
  if (files.length === 0) return [];
  const fileArgs = files.map((f) => `'${f}'`).join(" ");
  try {
    const out = execSync(`grep -HnE -- '${pattern}' ${fileArgs}`, {
      cwd: APP_ROOT,
      encoding: "utf8",
    });
    return out
      .split("\n")
      .filter((line) => line.trim().length > 0)
      // Comments are allowed — only live code references are violations.
      .filter((line) => !/^[^:]+:\d+:\s*(\/\/|\*|\/\*)/.test(line));
  } catch (err) {
    if ((err as { status?: number }).status === 1) return [];
    throw err;
  }
}

describe("the browser holds no dashboard credential", () => {
  it("no getToken() call in any \"use client\" file", () => {
    const hits = scanClientFiles("getToken\\(");
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("no Authorization: Bearer header built in any \"use client\" file", () => {
    const hits = scanClientFiles("Authorization[[:space:]]*:[[:space:]]*.?Bearer");
    expect(hits, hits.join("\n")).toEqual([]);
  });

  it("no token-typed prop declared or passed in any \"use client\" file", () => {
    // `token:` (type/declaration) and `token=` (JSX prop), anchored so
    // camelCase/underscore neighbours (getToken, access_token) don't hit.
    const hits = scanClientFiles("(^|[^A-Za-z_])token[[:space:]]*[?:=]");
    expect(hits, hits.join("\n")).toEqual([]);
  });
});
