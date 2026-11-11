import {afterEach, beforeEach, expect, test} from "bun:test";
import {mkdtempSync, rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {runCli} from "../src/cli.ts";

let state: string;
beforeEach(() => {state = mkdtempSync(join(tmpdir(), "agentsfleet-pet-"));});
afterEach(() => rmSync(state, {recursive: true, force: true}));

async function preview(args: readonly string[], tty = false) {
  let stdout = ""; let stderr = ""; let requests = 0;
  const fetchImpl = async () => {requests += 1; throw new Error("The pet preview must stay local");};
  fetchImpl.preconnect = () => undefined;
  const code = await runCli(args, {
    env: {AGENTSFLEET_STATE_DIR: state, TERM: "xterm-256color"},
    stdout: {isTTY: tty, columns: 80, write: text => {stdout += text; return true;}},
    stderr: {isTTY: tty, columns: 80, write: text => {stderr += text; return true;}},
    fetchImpl,
  });
  return {code, stdout, stderr, requests};
}

test.each([{args: ["pet"]}, {args: ["pet", "--json"]}, {args: ["--json", "pet"]}])("pet needs no login and preserves machine output for %j", async ({args}) => {
  const result = await preview(args);
  expect(result).toEqual({code: 0, stdout: '{\n  "name": "Scout"\n}\n', stderr: "", requests: 0});
});

test("pet help explains the preview without authenticating or calling the API", async () => {
  const result = await preview(["pet", "--help"]);
  expect(result.code).toBe(0);
  expect(result.stdout).toContain("five seconds");
  expect(result.stdout).toContain("--json");
  expect(result.requests).toBe(0);
});

test("pet plays the colored owl locally and clears it before its final sentence", async () => {
  const result = await preview(["pet"], true);
  expect(result.code).toBe(0);
  expect(result.requests).toBe(0);
  expect(result.stderr).toContain("\u001b[38;5;79m ,(o,o), \u001b[0m Scout");
  expect(result.stderr).toContain("(-,-)");
  expect(result.stderr).toMatch(/\r +\r$/);
  expect(result.stdout).toBe("Scout appears while a Platform request is running.\n");
}, 10000);
