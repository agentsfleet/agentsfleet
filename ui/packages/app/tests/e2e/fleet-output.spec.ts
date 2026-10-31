// Playwright workers use Node; Bun builds the real component for native Chromium.
import { execFile } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { expect, test as base } from "@playwright/test";
import { applicationCss } from "./fixtures/fleet-output-browser-assets";

const APP_ROOT = path.resolve(import.meta.dirname, "../..");
const OUTPUT = Array.from({ length: 2_000 }, (_, index) =>
  `line${String(index + 1).padStart(4, "0")} ${"payload".repeat(index % 17 === 0 ? 30 : 1)}`).join("\n");
const LAST_LINE = "line2000 payload";

/** This is the real output component and application CSS over loopback HTTP. */
const test = base.extend<object, { outputProbe: string }>({
  outputProbe: [async ({ browserName }, provideProbe) => {
    const built = await promisify(execFile)("bun", ["build", "tests/e2e/fixtures/fleet-output-browser-probe.tsx",
      "--target=browser", "--format=iife", "--minify", "--define", 'process.env.NODE_ENV="production"',
      "--define", 'process.env.NEXT_PUBLIC_API_URL="https://api-test.agentsfleet.net"'],
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
        response.end(`<html data-theme="dark"><head><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script src="/probe.js"></script></body></html>`);
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
