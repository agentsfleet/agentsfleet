import { execFile } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { test as base } from "@playwright/test";
import { applicationCss } from "./fleet-output-browser-assets";

export const test = base.extend<object, {runnerConfiguration: string}>({
  runnerConfiguration: [async ({browserName}, provideProbe) => {
    const appRoot = path.resolve(import.meta.dirname, "../../..");
    const built = await promisify(execFile)("bun", ["tests/e2e/fixtures/runner-configuration-build.ts"],
      {cwd: appRoot, maxBuffer: 4_000_000, timeout: 20_000});
    const css = await applicationCss(appRoot);
    const server = http.createServer((request, response) => {
      if (request.url === "/probe.js") {
        response.writeHead(200, {"content-type": "text/javascript"}); response.end(built.stdout);
      } else {
        response.writeHead(200, {"content-type": "text/html"});
        response.end(`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script type="module" src="/probe.js"></script></body></html>`);
      }
    });
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error(`${browserName} runner fixture did not bind a TCP port`);
    try { await provideProbe(`http://127.0.0.1:${address.port}`); }
    finally { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); }
  }, {scope: "worker"}],
});
