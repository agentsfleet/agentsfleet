import * as fs from "node:fs";
import * as path from "node:path";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  initializeBrowserSessions,
  recordBrowserSession,
  revokeBrowserSessions,
} from "./e2e/acceptance/fixtures/browser-sessions";

vi.mock("node:fs", async (importOriginal) => ({
  ...await importOriginal<typeof import("node:fs")>(),
}));

const DIRECTORY_ENV = "AGENTSFLEET_E2E_SESSION_DIRECTORY";
const SESSION_A = "sess_acceptanceA";
const SESSION_B = "sess_acceptanceB";
const DIRECTORIES: string[] = [];
const remoteSessions = new Set<string>();
const requests: string[] = [];

function newRun(): string {
  initializeBrowserSessions();
  const directory = process.env[DIRECTORY_ENV];
  if (!directory) throw new Error("Session ledger was not initialized");
  DIRECTORIES.push(directory);
  return directory;
}

function revokeAtClerk(url: string | URL | Request): Response {
  const sessionId = (url instanceof Request ? url.url : String(url)).split("/").at(-2);
  if (!sessionId) throw new Error("Expected a Clerk session URL");
  requests.push(sessionId);
  remoteSessions.delete(sessionId);
  return Response.json({ id: sessionId, status: "revoked" });
}

beforeEach(() => {
  vi.stubEnv(DIRECTORY_ENV, undefined);
  vi.stubEnv("CLERK_SECRET_KEY", "sk_test_fixture");
  requests.length = 0;
  remoteSessions.clear();
  remoteSessions.add(SESSION_A);
  remoteSessions.add(SESSION_B);
  vi.stubGlobal("fetch", vi.fn(revokeAtClerk));
});

afterEach(() => {
  for (const directory of DIRECTORIES.splice(0)) fs.rmSync(directory, { recursive: true, force: true });
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("revokes every recorded browser session once and removes the ledger", async () => {
  const directory = newRun();
  recordBrowserSession(SESSION_A);
  recordBrowserSession(SESSION_B);
  recordBrowserSession(SESSION_A);
  await revokeBrowserSessions();
  expect(requests.sort((left, right) => left.localeCompare(right))).toEqual([SESSION_A, SESSION_B]);
  expect(remoteSessions.size).toBe(0);
  expect(fs.existsSync(directory)).toBe(false);
  await revokeBrowserSessions();
  expect(requests).toHaveLength(2);
});

it("never revokes sessions recorded by a different acceptance run", async () => {
  const otherRun = newRun();
  recordBrowserSession(SESSION_A);
  newRun();
  recordBrowserSession(SESSION_B);
  await revokeBrowserSessions();
  expect(requests).toEqual([SESSION_B]);
  expect(remoteSessions).toEqual(new Set([SESSION_A]));
  expect(fs.existsSync(path.join(otherRun, SESSION_A))).toBe(true);
});

it("continues after one revocation fails and retains only the failed record for retry", async () => {
  const directory = newRun();
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  recordBrowserSession(SESSION_A);
  recordBrowserSession(SESSION_B);
  vi.stubGlobal("fetch", vi.fn(revokeAtClerk)
    .mockImplementationOnce(() => new Response("Clerk unavailable", { status: 503 })));
  await revokeBrowserSessions();
  expect(remoteSessions).toEqual(new Set([SESSION_A]));
  expect(fs.readdirSync(directory)).toEqual([SESSION_A]);
  expect(log).toHaveBeenCalledOnce();
  await revokeBrowserSessions();
  expect(remoteSessions.size).toBe(0);
  expect(fs.existsSync(directory)).toBe(false);
});

it("retains failed authorization records instead of claiming a revoke succeeded", async () => {
  const directory = newRun();
  vi.spyOn(console, "error").mockImplementation(() => {});
  recordBrowserSession(SESSION_A);
  vi.stubGlobal("fetch", vi.fn(() => new Response("unauthorized", { status: 401 })));
  await revokeBrowserSessions();
  expect(remoteSessions.has(SESSION_A)).toBe(true);
  expect(fs.readdirSync(directory)).toEqual([SESSION_A]);
});

it("retries safely when Clerk revoked the session but its response was lost", async () => {
  const directory = newRun();
  vi.spyOn(console, "error").mockImplementation(() => {});
  recordBrowserSession(SESSION_A);
  vi.stubGlobal("fetch", vi.fn(revokeAtClerk).mockImplementationOnce((url) => {
    revokeAtClerk(url);
    throw new TypeError("connection reset after revoke");
  }));
  await revokeBrowserSessions();
  expect(remoteSessions.has(SESSION_A)).toBe(false);
  expect(fs.readdirSync(directory)).toEqual([SESSION_A]);
  await revokeBrowserSessions();
  expect(remoteSessions.has(SESSION_A)).toBe(false);
  expect(requests).toEqual([SESSION_A, SESSION_A]);
  expect(fs.existsSync(directory)).toBe(false);
});

it("retains the record if local deletion fails after remote revocation", async () => {
  const directory = newRun();
  vi.spyOn(console, "error").mockImplementation(() => {});
  recordBrowserSession(SESSION_A);
  const unlink = vi.spyOn(fs, "unlinkSync").mockImplementationOnce(() => {
    throw new Error("ledger filesystem unavailable");
  });
  await revokeBrowserSessions();
  expect(remoteSessions.has(SESSION_A)).toBe(false);
  expect(fs.readdirSync(directory)).toEqual([SESSION_A]);
  unlink.mockRestore();
  await revokeBrowserSessions();
  expect(fs.existsSync(directory)).toBe(false);
});

it("fails before recording unsafe IDs and ignores unexpected ledger files", async () => {
  expect(() => recordBrowserSession(SESSION_A)).toThrow("globalSetup must run first");
  const directory = newRun();
  expect(() => recordBrowserSession("../session")).toThrow("Invalid Clerk browser session id");
  fs.writeFileSync(path.join(directory, "unrelated"), "");
  await revokeBrowserSessions();
  expect(requests).toEqual([]);
  expect(fs.readdirSync(directory)).toEqual(["unrelated"]);
});

it("does nothing when setup did not establish a run", async () => {
  await revokeBrowserSessions();
  expect(requests).toEqual([]);
});
