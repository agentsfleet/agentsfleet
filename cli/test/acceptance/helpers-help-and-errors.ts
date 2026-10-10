// Shared fixtures and doubles for the help and error-shape acceptance suites.
//
// Extracted when help-and-errors.spec.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import { beforeAll, afterAll } from "bun:test";
import fs from "node:fs/promises";
import { createServer } from "node:http";
import type { Socket } from "node:net";
import os from "node:os";
import path from "node:path";
import { UNROUTABLE_API_URL } from "./fixtures/constants.ts";
import { composeEnv } from "./fixtures/cli.js";

export const ANSI_RE = /\x1b\[[0-9;]*m/g;
export const TELEMETRY_NOT_DISABLED = "0";
export const TELEMETRY_EXIT_BUDGET_MS = 5_000;

export interface StalledServer {
  readonly url: string;
  close(): Promise<void>;
}

export async function startStalledServer(): Promise<StalledServer> {
  const sockets = new Set<Socket>();
  const server = createServer(() => {
    // Keep the response open so the client request timeout must end the flush.
  });
  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (address === null || typeof address === "string") {
    for (const socket of sockets) socket.destroy();
    throw new Error("stalled telemetry server did not open a TCP port");
  }
  return {
    url: `http://127.0.0.1:${address.port}`,
    close: () =>
      new Promise<void>((resolve, reject) => {
        for (const socket of sockets) socket.destroy();
        server.close((error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      }),
  };
}

export function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, "").replace(/\s+$/gm, "");
}

export let unauthenticatedStateDir: string;

beforeAll(async () => {
  unauthenticatedStateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-unauth-"));
});

afterAll(async () => fs.rm(unauthenticatedStateDir, { recursive: true, force: true }));

export function emptyEnv(extra?: Record<string, string>): Record<string, string> {
  return composeEnv({
    AGENTSFLEET_API_URL: UNROUTABLE_API_URL,
    AGENTSFLEET_STATE_DIR: unauthenticatedStateDir,
    NO_COLOR: "1",
    ...(extra ?? {}),
  });
}
