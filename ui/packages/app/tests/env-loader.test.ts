import { afterEach, expect, test, vi } from "vitest";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadWorktreeEnv } from "./e2e/acceptance/fixtures/env-loader";

const originalFile = process.env.AGENTSFLEET_UI_ENV_FILE;
const originalUrl = process.env.NEXT_PUBLIC_API_URL;
let fixtureDirectory: string | undefined;

afterEach(() => {
  if (fixtureDirectory) rmSync(fixtureDirectory, { recursive: true });
  fixtureDirectory = undefined;
  if (originalFile === undefined) delete process.env.AGENTSFLEET_UI_ENV_FILE;
  else process.env.AGENTSFLEET_UI_ENV_FILE = originalFile;
  if (originalUrl === undefined) delete process.env.NEXT_PUBLIC_API_URL;
  else process.env.NEXT_PUBLIC_API_URL = originalUrl;
});

test("acceptance env loader reads the configured machine-level file", () => {
  fixtureDirectory = mkdtempSync(join(tmpdir(), "agentsfleet-ui-env-"));
  const file = join(fixtureDirectory, "ui.env.local");
  writeFileSync(file, "NEXT_PUBLIC_API_URL=https://example.test\n");
  process.env.AGENTSFLEET_UI_ENV_FILE = file;
  delete process.env.NEXT_PUBLIC_API_URL;

  loadWorktreeEnv();

  expect(process.env.NEXT_PUBLIC_API_URL).toBe("https://example.test");
});

test("acceptance env loader rejects a missing configured file", () => {
  process.env.AGENTSFLEET_UI_ENV_FILE = join(tmpdir(), "agentsfleet-ui-env-missing");
  expect(loadWorktreeEnv).toThrow("AGENTSFLEET_UI_ENV_FILE does not exist");
});

test("Next configuration loads the configured file without a worktree symlink", async () => {
  fixtureDirectory = mkdtempSync(join(tmpdir(), "agentsfleet-next-env-"));
  const file = join(fixtureDirectory, "ui.env.local");
  writeFileSync(file, "NEXT_PUBLIC_API_URL=https://next.example.test\n");
  process.env.AGENTSFLEET_UI_ENV_FILE = file;
  delete process.env.NEXT_PUBLIC_API_URL;
  vi.resetModules();

  await import("../next.config");

  expect(process.env.NEXT_PUBLIC_API_URL).toBe("https://next.example.test");
});
