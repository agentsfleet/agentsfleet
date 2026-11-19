import { execFile } from "node:child_process";
import http from "node:http";
import type { AddressInfo } from "node:net";
import path from "node:path";
import { promisify } from "node:util";
import { expect, test } from "@playwright/test";
import { applicationCss } from "./fixtures/fleet-output-browser-assets";

const APP_ROOT = path.resolve(import.meta.dirname, "../..");
let server: http.Server;
let origin: string;

test.beforeAll(async () => {
  const [{ stdout: bundle }, css] = await Promise.all([
    promisify(execFile)("bun", [
      "build", "tests/e2e/fixtures/appearance-browser-probe.tsx",
      "--target=browser", "--format=esm", "--minify",
      "--define", "process.env={}",
      "--define", 'process.env.NODE_ENV="production"',
      "--define", 'process.env.NEXT_PUBLIC_API_URL="https://api-test.agentsfleet.net"',
    ], { cwd: APP_ROOT, maxBuffer: 2_000_000, timeout: 20_000 }),
    applicationCss(APP_ROOT),
  ]);
  server = http.createServer((request, response) => {
    if (request.url === "/probe.js") {
      response.writeHead(200, { "content-type": "text/javascript" });
      response.end(bundle);
      return;
    }
    response.writeHead(200, { "content-type": "text/html" });
    response.end(`<!doctype html><html><head><meta charset="utf-8"><style>${css}</style></head>
      <body><div id="appearance-probe"></div><script type="module" src="/probe.js"></script></body></html>`);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
});

test.afterAll(async () => {
  server.closeAllConnections();
  await new Promise<void>((resolve) => server.close(() => resolve()));
});

test("Appearance applies the selected palette and persists across reload", async ({ page }) => {
  await page.goto(origin);
  const root = page.locator("html");
  const surface = page.getByRole("main", { name: "Dashboard" });
  const trigger = page.getByRole("button", { name: "Workspace and account menu" });
  await expect(root).toHaveAttribute("data-theme", "dark");
  const dark = await surface.evaluate((node) => getComputedStyle(node).backgroundColor);

  await trigger.click();
  await page.getByRole("menuitem", { name: "Appearance: switch to light" }).click();
  await expect(root).toHaveAttribute("data-theme", "light");
  await expect(surface).toHaveCSS("background-color", "rgb(250, 251, 252)");
  await expect(page.getByRole("menuitem", { name: "Appearance: switch to dark" })).toBeVisible();

  await page.reload();
  await expect(root).toHaveAttribute("data-theme", "light");
  await trigger.click();
  await page.getByRole("menuitem", { name: "Appearance: switch to dark" }).press("Enter");
  await expect(root).toHaveAttribute("data-theme", "dark");
  await expect(surface).toHaveCSS("background-color", dark);
});
