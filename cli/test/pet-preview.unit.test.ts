import {afterEach, beforeEach, expect, test} from "bun:test";
import {mkdtempSync, rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {runCli} from "../src/cli.ts";
import {SIGINT, SIGTERM} from "../src/constants/signals.ts";

let state: string;
beforeEach(() => {state = mkdtempSync(join(tmpdir(), "agentsfleet-pet-"));});
afterEach(() => rmSync(state, {recursive: true, force: true}));

async function preview(args: readonly string[], tty = false) {
  let stdout = ""; let stderr = ""; let requests = 0;
  const fetchImpl = async () => {requests += 1; throw new Error("The pet preview must stay local");};
  fetchImpl.preconnect = () => undefined;
  const code = await runCli(args, {
    env: {AGENTSFLEET_STATE_DIR: state, TERM: "xterm-256color"},
    stdout: {isTTY: tty, columns: 80, rows: 24, write: text => {stdout += text; return true;}},
    stderr: {isTTY: tty, columns: 80, rows: 24, write: text => {stderr += text; return true;}},
    fetchImpl,
  });
  return {code, stdout, stderr, requests};
}

test.each([{args: ["pet"]}, {args: ["pet", "--json"]}, {args: ["--json", "pet"]}])("pet needs no login and preserves machine output for %j", async ({args}) => {
  const result = await preview(args);
  expect(result).toEqual({code: 0, stdout: '{\n  "name": "Orbit"\n}\n', stderr: "", requests: 0});
});

test("pet help explains the preview without authenticating or calling the API", async () => {
  const result = await preview(["pet", "--help"]);
  expect(result.code).toBe(0);
  expect(result.stdout).toContain("five seconds");
  expect(result.stdout).toContain("--json");
  expect(result.requests).toBe(0);
});

test("pet animates Orbit locally and clears its rows before the final sentence", async () => {
  const result = await preview(["pet"], true);
  expect(result.code).toBe(0);
  expect(result.requests).toBe(0);
  expect(result.stderr).toContain("▀");
  expect(result.stderr).toContain("▄");
  expect(result.stderr).toContain("\u001b[48;5;");
  expect(result.stderr).toContain("Just floating.");
  expect(result.stderr).toContain("On the lookout.");
  expect(result.stderr).toContain("A little victory dance.");
  expect(result.stderr).toEndWith("\u001b[17A\r");
  expect(result.stdout).toBe("Orbit appears while a Platform request is running.\n");
}, 10000);

test.each([SIGINT, SIGTERM])("pet cancels on %s, clears its drawing and releases signal handlers", async signal => {
  const beforeInterrupt = process.listenerCount(SIGINT);
  const beforeTerminate = process.listenerCount(SIGTERM);
  const running = preview(["pet"], true);
  for (let attempt = 0; attempt < 200 && process.listenerCount(SIGINT) === beforeInterrupt; attempt += 1) await Bun.sleep(5);
  expect(process.listenerCount(SIGINT)).toBe(beforeInterrupt + 1);
  process.emit(signal);
  const result = await running;
  expect(result.code).toBe(130);
  expect(result.requests).toBe(0);
  expect(result.stderr).toContain("\u001b[17A\r");
  expect(result.stderr).toContain("pet preview canceled");
  expect(result.stdout).toBe("");
  expect(process.listenerCount(SIGINT)).toBe(beforeInterrupt);
  expect(process.listenerCount(SIGTERM)).toBe(beforeTerminate);
});
