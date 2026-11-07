import { execFile } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { test as base } from "@playwright/test";
import { applicationCss } from "./fleet-output-browser-assets";
import { summary, detail } from "@/tests/helpers/task-reads";
import { SCHEDULE_EVENT } from "@/tests/helpers/schedule-activity";

export const test = base.extend<object, {chatProbe: string}>({
  chatProbe: [async ({browserName}, provideProbe) => {
    const appRoot = path.resolve(import.meta.dirname, "../../..");
    const built = await promisify(execFile)("bun", ["tests/e2e/fixtures/chat-browser-build.ts"],
      {cwd: appRoot, maxBuffer: 4_000_000, timeout: 20_000});
    const css = await applicationCss(appRoot);
    const server = http.createServer((request, response) => {
      if (request.url === "/probe.js") {
        response.writeHead(200, {"content-type": "text/javascript"}); response.end(built.stdout);
      } else if (request.url?.includes("/tasks")) {
        const task = {...summary(undefined, "running"), title: "Validate the Linkwarden repair"};
        response.writeHead(200, {"content-type": "application/json"});
        response.end(JSON.stringify(request.url.includes("?limit=") ? {items: [task], next_cursor: null} : detail(task)));
      } else if (request.url?.startsWith("/event-details?")) {
        const query = new URL(request.url, "http://localhost").searchParams;
        const owned = query.get("workspace") === SCHEDULE_EVENT.workspace_id && query.get("fleet") === SCHEDULE_EVENT.fleet_id;
        response.writeHead(owned ? 200 : 404, {"content-type": "application/json"});
        response.end(JSON.stringify({...SCHEDULE_EVENT, event_id: query.get("event"), response_text: "Native recorded support review."}));
      } else {
        response.writeHead(200, {"content-type": "text/html"});
        response.end(`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script type="module" src="/probe.js"></script></body></html>`);
      }
    });
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error(`${browserName} chat fixture did not bind a TCP port`);
    try {await provideProbe(`http://127.0.0.1:${address.port}`);}
    finally {server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));}
  }, {scope: "worker"}],
});
