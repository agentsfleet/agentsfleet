// @vitest-environment node
/**
 * Playwright's output folders and the CI HTML reports are uploaded as CI
 * artifacts. The fixture JWT cache holds live session tokens, so it must sit
 * outside every one of them.
 */
import * as path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import { JWT_CACHE_PATH } from "./e2e/acceptance/fixtures/constants";

type ReporterEntry = string | readonly [string, Record<string, unknown>?];
type UploadedConfig = { outputDir?: string; reporter?: string | readonly ReporterEntry[] };

const HTML_REPORTER = "html";

function uploadedFolders(config: UploadedConfig): string[] {
  const reporters = Array.isArray(config.reporter) ? config.reporter : [];
  const reportFolders = reporters
    .filter((entry): entry is readonly [string, Record<string, unknown>] => Array.isArray(entry) && entry[0] === HTML_REPORTER)
    .map(([, options]) => String(options.outputFolder));
  return [config.outputDir, ...reportFolders].filter((dir): dir is string => typeof dir === "string");
}

afterEach(() => {
  vi.unstubAllEnvs();
  vi.resetModules();
});

describe("fixture jwt-cache location", () => {
  it("keeps the cache out of every folder the CI run uploads", async () => {
    // The HTML reporters are configured only on CI, which is where uploads happen.
    vi.stubEnv("CI", "1");
    const { default: acceptance } = await import("../playwright.acceptance.config");
    const { default: smoke } = await import("../playwright.config");
    const folders = [acceptance, smoke].flatMap((config) => uploadedFolders(config as UploadedConfig));

    // Both output folders and both HTML reports, so a dropped entry cannot pass silently.
    expect(folders).toHaveLength(4);
    for (const folder of folders) {
      expect(JWT_CACHE_PATH.startsWith(path.resolve(folder) + path.sep), folder).toBe(false);
    }
  });
});
