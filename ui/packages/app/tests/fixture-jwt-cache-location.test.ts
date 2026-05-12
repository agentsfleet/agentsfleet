/**
 * Regression: the fixture JWT cache file must NOT live inside a Playwright-
 * managed directory.
 *
 * Threat model: Playwright's outputDir and the acceptance-config artifact
 * folder both get uploaded as CI artifacts. If a future refactor moves the
 * cache file into either, a fixture sessionJwt (~15 min valid window) plus a
 * cookieJwt would ride along in the public artifact bundle.
 *
 * The cache currently lives at `<cwd>/.fixture-jwts.json` with mode 0600, and
 * Playwright's acceptance config points outputDir at `playwright-acceptance-results`
 * + reporter at `playwright-acceptance-report`. This test pins the relationship —
 * a move that puts the cache inside either Playwright dir fails the test.
 */
import * as path from "node:path";
import { describe, expect, it } from "vitest";

const cwd = process.cwd();

describe("fixture jwt-cache location", () => {
  it("does not sit inside Playwright outputDir or acceptance report folder", () => {
    const cachePath = path.resolve(cwd, ".fixture-jwts.json");
    const outputDir = path.resolve(cwd, "playwright-acceptance-results");
    const reportDir = path.resolve(cwd, "playwright-acceptance-report");
    const smokeOutputDir = path.resolve(cwd, "playwright-results");

    expect(cachePath.startsWith(outputDir + path.sep)).toBe(false);
    expect(cachePath.startsWith(reportDir + path.sep)).toBe(false);
    expect(cachePath.startsWith(smokeOutputDir + path.sep)).toBe(false);
  });
});
