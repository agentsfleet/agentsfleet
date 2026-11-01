// Playwright workers use Node; Bun builds the real component for native Chromium.
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { expect, test as base, type Page } from "@playwright/test";
import { applicationCss } from "./fixtures/fleet-output-browser-assets";

const APP_ROOT = path.resolve(import.meta.dirname, "../..");
const OUTPUT = Array.from({ length: 2_000 }, (_, index) =>
  `line${String(index + 1).padStart(4, "0")} ${"payload".repeat(index % 17 === 0 ? 30 : 1)}`).join("\n");
const LAST_LINE = "line2000 payload";
type OutputSample = {openingMs: number; initialRenderMs: number; scrollFrameP95Ms: number; mountedRows: number[]};

/** This is the real output component and application CSS over loopback HTTP. */
const test = base.extend<object, { outputProbe: string }>({
  outputProbe: [async ({ browserName }, provideProbe) => {
    const built = await promisify(execFile)("bun", ["tests/e2e/fixtures/fleet-output-browser-build.ts"],
    { cwd: APP_ROOT, maxBuffer: 2_000_000, timeout: 20_000 });
    const css = await applicationCss(APP_ROOT);
    const server = http.createServer((request, response) => {
      if (request.url?.startsWith("/live/")) {
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify({ arguments: {}, output: OUTPUT, truncated: false }));
      } else if (request.url === "/probe.js") {
        response.writeHead(200, { "content-type": "text/javascript" }); response.end(built.stdout);
      } else {
        response.writeHead(200, { "content-type": "text/html" });
        response.end(`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script src="/probe.js"></script></body></html>`);
      }
    });
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error(`${browserName} output probe did not bind a TCP port`);
    try { await provideProbe(`http://127.0.0.1:${address.port}`); }
    finally { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); }
  }, { scope: "worker" }],
});

test("long output stays keyboard-reachable and wrapped rows remeasure after narrowing", async ({ page, outputProbe }, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.goto(outputProbe);
  await page.getByRole("button", {name: "Open output"}).click();
  await expect(page.getByLabel("Output line count")).toHaveText("2000 lines");
  await page.evaluate(() => document.fonts.ready);
  expect(await page.evaluate(async () => (await document.fonts.load('14px "Commit Mono"', "line0001")).length)).toBeGreaterThan(0);
  expect(await page.evaluate(() => document.fonts.check('14px "Commit Mono"'))).toBe(true);
  const pane = page.getByRole("list", { name: "Command output lines" });
  await pane.focus(); await page.keyboard.press("End");
  await expect(page.getByText(LAST_LINE, { exact: true })).toBeInViewport();
  expect(await pane.locator("[data-line-number]").count()).toBeLessThan(100);
  await page.setViewportSize({ width: 390, height: 844 });
  await pane.focus(); await page.keyboard.press("Home");
  const first = pane.locator('[data-line-number="1"]');
  const second = pane.locator('[data-line-number="2"]');
  await expect(first).toBeInViewport(); await expect(second).toBeInViewport();
  await expect.poll(async () => {
    const [a, b] = await Promise.all([first.boundingBox(), second.boundingBox()]);
    return a !== null && b !== null && a.height > b.height * 1.5 && b.y >= a.y + a.height - 1;
  }).toBe(true);
  expect(await pane.evaluate(node => node.scrollWidth <= node.clientWidth)).toBe(true);
  await page.keyboard.press("End");
  await expect(page.getByText(LAST_LINE, { exact: true })).toBeInViewport();
  expect(errors).toEqual([]);
  await info.attach("wrapped-output", { body: await page.screenshot(), contentType: "image/png" });
});

test("records five production output-opening and native-scroll samples", async ({page, outputProbe}, info) => {
  const viewport = info.project.name === "mobile-chromium" ? {width: 390, height: 844} : {width: 1280, height: 900};
  await page.setViewportSize(viewport);
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Emulation.setCPUThrottlingRate", {rate: 4});
  const samples: OutputSample[] = [];
  try {
    for (let run = 0; run < 5; run++) samples.push(await measureOutput(page, outputProbe));
    const revision = await promisify(execFile)("git", ["rev-parse", "HEAD"], {cwd: APP_ROOT});
    const status = await promisify(execFile)("git", ["status", "--porcelain"], {cwd: APP_ROOT});
    const bundle = await (await fetch(`${outputProbe}/probe.js`)).text();
    const report = {sourceRevision: revision.stdout.trim(), workingTreeDirty: status.stdout.length > 0,
      bundleSha256: createHash("sha256").update(bundle).digest("hex"), project: info.project.name, fixture: "2000 output lines",
      outputBytes: Buffer.byteLength(OUTPUT), outputSha256: createHash("sha256").update(OUTPUT).digest("hex"), cpuThrottling: 4,
      viewport: page.viewportSize(), paneHeight: 384, samples,
      medianOpeningMs: percentile(samples.map(sample => sample.openingMs), 0.5),
      medianInitialRenderMs: percentile(samples.map(sample => sample.initialRenderMs), 0.5)};
    console.log(`OUTPUT_BENCHMARK ${JSON.stringify(report)}`);
    await info.attach("output-benchmark.json", {body: JSON.stringify(report, null, 2), contentType: "application/json"});
  } finally {await cdp.detach();}
});

async function measureOutput(page: Page, url: string): Promise<OutputSample> {
  await page.goto(url);
  expect(await page.evaluate(async () => (await document.fonts.load('14px "Commit Mono"', "line0001")).length)).toBeGreaterThan(0);
  expect(await page.evaluate(async () => (await document.fonts.load('14px "Instrument Sans Variable"', "Open output")).length)).toBeGreaterThan(0);
  await page.getByRole("button", {name: "Open output"}).click();
  await expect.poll(() => page.evaluate(() => window.outputMeasurement.openingMs)).not.toBeNull();
  const opening = await page.evaluate(() => window.outputMeasurement);
  if (opening.openingMs === null || opening.initialRenderMs === null) throw new Error("Output opening was not measured");
  expect(opening.initialRenderMs).toBeGreaterThan(0);
  const pane = page.getByRole("list", {name: "Command output lines"});
  expect(await pane.evaluate(node => node.clientHeight)).toBe(384);
  const mountedRows = [await pane.locator("[data-line-number]").count()];
  await pane.hover();
  const [intervals] = await Promise.all([page.evaluate(scrollFrames), page.mouse.wheel(0, 6000)]);
  mountedRows.push(await pane.locator("[data-line-number]").count());
  await pane.focus(); await page.keyboard.press("End");
  await expect(page.getByText(LAST_LINE, {exact: true})).toBeInViewport();
  mountedRows.push(await pane.locator("[data-line-number]").count());
  for (const rows of mountedRows) {expect(rows).toBeGreaterThan(0); expect(rows).toBeLessThan(100);}
  return {openingMs: opening.openingMs, initialRenderMs: opening.initialRenderMs,
    scrollFrameP95Ms: percentile(intervals, 0.95), mountedRows};
}

function scrollFrames(): Promise<number[]> {
  return new Promise(resolve => {
    const frames: number[] = []; let previous: number | undefined;
    const sample = (now: number) => {
      if (previous !== undefined) frames.push(now - previous);
      previous = now;
      if (frames.length < 60) requestAnimationFrame(sample); else resolve(frames);
    };
    requestAnimationFrame(sample);
  });
}

function percentile(values: number[], quantile: number): number {
  const sorted = values.toSorted((left, right) => left - right);
  const result = sorted[Math.ceil(sorted.length * quantile) - 1];
  if (result === undefined) throw new Error("A benchmark sample is empty");
  return result;
}
