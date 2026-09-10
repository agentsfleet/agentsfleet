import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { loadWorktreeEnv } from "../tests/e2e/acceptance/fixtures/env-loader";

/**
 * The acceptance harness fails loud on four environment names before it runs.
 * Two spellings of the Clerk publishable key exist and each supported layout
 * supplies only one: a worktree-root `.env` carries `CLERK_PUBLISHABLE_KEY`,
 * while a linked `ui/packages/app/.env.local` carries only the
 * `NEXT_PUBLIC_`-prefixed one. `clerkSetup()` reads the former; the browser
 * needs the latter. Aliasing runs both ways so neither layout fails a check on
 * a value the process already holds under its other name.
 */

const SENTINEL_PREFIXED = "pk_test_prefixed_sentinel";
const SENTINEL_BARE = "pk_test_bare_sentinel";
const BARE = "CLERK_PUBLISHABLE_KEY";
const PREFIXED = "NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY";

// The loader resolves `.env.local` and `../../../.env` from `process.cwd()`,
// and the real files sit exactly there when vitest runs from this package.
// Pointing cwd at an empty directory isolates the aliasing from whatever a
// developer happens to have on disk — otherwise this test passes or fails
// depending on which env layout the machine uses, which is the opposite of a pin.
let emptyDir: string;

beforeEach(() => {
  emptyDir = fs.mkdtempSync(path.join(os.tmpdir(), "env-loader-alias-"));
  vi.spyOn(process, "cwd").mockReturnValue(emptyDir);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
  fs.rmSync(emptyDir, { recursive: true, force: true });
});

describe("Clerk publishable key aliasing", () => {
  it("should fill the bare name when only the NEXT_PUBLIC one is present", () => {
    // The linked-.env.local layout. Without this direction the harness refuses
    // to start on a key it is already holding.
    vi.stubEnv(PREFIXED, SENTINEL_PREFIXED);
    vi.stubEnv(BARE, "");
    delete process.env[BARE];

    loadWorktreeEnv();

    expect(process.env[BARE]).toBe(SENTINEL_PREFIXED);
  });

  it("should fill the NEXT_PUBLIC name when only the bare one is present", () => {
    // The worktree-root .env layout, which the dev server needs prefixed so the
    // browser reaches the same Clerk instance the harness mints against.
    vi.stubEnv(BARE, SENTINEL_BARE);
    vi.stubEnv(PREFIXED, "");
    delete process.env[PREFIXED];

    loadWorktreeEnv();

    expect(process.env[PREFIXED]).toBe(SENTINEL_BARE);
  });

  it("should not overwrite either name when both are already set", () => {
    // Non-clobbering is the property that lets an explicit shell export or a
    // Continuous Integration secret win over anything on disk.
    vi.stubEnv(BARE, SENTINEL_BARE);
    vi.stubEnv(PREFIXED, SENTINEL_PREFIXED);

    loadWorktreeEnv();

    expect(process.env[BARE]).toBe(SENTINEL_BARE);
    expect(process.env[PREFIXED]).toBe(SENTINEL_PREFIXED);
  });
});
