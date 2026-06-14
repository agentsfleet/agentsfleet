/**
 * Bridges the worktree-root .env into the playwright process.
 *
 * Bun auto-loads only the cwd's .env / .env.local. The acceptance suite runs
 * from `ui/packages/app/`, where .env.local has NEXT_PUBLIC_API_URL but the
 * Clerk credentials (CLERK_SECRET_KEY, CLERK_WEBHOOK_SECRET) live one level
 * up at the worktree root alongside agentsfleetd's .env. Running the suite used
 * to require shell-side `op read` exports for every run; this loader makes
 * `bun run test:e2e:acceptance` self-sufficient.
 *
 * Idempotent and non-clobbering: existing `process.env` values win, so CI
 * secrets / explicit shell exports always override the file.
 */
import * as fs from "node:fs";
import * as path from "node:path";

function unquote(value: string): string {
  if (value.length < 2) return value;
  const first = value[0];
  const last = value[value.length - 1];
  if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
    return value.slice(1, -1);
  }
  return value;
}

function parseDotenv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq < 1) continue;
    out[line.slice(0, eq).trim()] = unquote(line.slice(eq + 1).trim());
  }
  return out;
}

function applyFile(envPath: string): void {
  if (!fs.existsSync(envPath)) return;
  const parsed = parseDotenv(fs.readFileSync(envPath, "utf8"));
  for (const [key, value] of Object.entries(parsed)) {
    if (process.env[key] === undefined) process.env[key] = value;
  }
}

/**
 * Loads `.env.local` (this package's local overrides — NEXT_PUBLIC_API_URL)
 * and `<worktree-root>/.env` (shared CLERK_* secrets) into `process.env`.
 * Already-set keys are preserved, so CI secrets / explicit shell exports
 * always override the files. Local file wins on conflict (loaded first).
 *
 * Resolves paths from `process.cwd()`; the suite is invoked from
 * `ui/packages/app/`, so `../../../.env` lands at the worktree root.
 *
 * Why this exists: `bunx playwright test` does not trigger Bun's auto-load
 * of `.env.local`, and the playwright config + globalSetup need both
 * NEXT_PUBLIC_API_URL and the Clerk creds before the suite starts.
 *
 * Aliases CLERK_PUBLISHABLE_KEY → NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY so the
 * Next dev server (which only exposes NEXT_PUBLIC_-prefixed values to the
 * browser) gets the same Clerk DEV instance the harness mints against. The
 * worktree-root .env carries the un-prefixed name; aliasing keeps the
 * single-source-of-truth in .env without duplicating the secret.
 */
export function loadWorktreeEnv(): void {
  applyFile(path.resolve(process.cwd(), ".env.local"));
  applyFile(path.resolve(process.cwd(), "../../../.env"));
  if (
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY === undefined &&
    process.env.CLERK_PUBLISHABLE_KEY !== undefined
  ) {
    process.env.NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY = process.env.CLERK_PUBLISHABLE_KEY;
  }
}
