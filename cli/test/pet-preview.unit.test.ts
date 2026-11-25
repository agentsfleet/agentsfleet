import {afterEach, beforeEach, expect, test} from "bun:test";
import {mkdtempSync, rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {PassThrough} from "node:stream";
import {runCli} from "../src/cli.ts";
import {playOrbit} from "../src/commands/pet/play.ts";
import {SIGINT, SIGTERM} from "../src/constants/signals.ts";
import {ORBIT_ROWS} from "../src/output/orbit.ts";
import {terminal} from "../src/output/palette.ts";

let state: string;
beforeEach(() => {state = mkdtempSync(join(tmpdir(), "agentsfleet-pet-"));});
afterEach(() => rmSync(state, {recursive: true, force: true}));

function harness(args: readonly string[], {tty = false, interactive = false, term = "xterm-256color", columns = 80} = {}) {
  let stdout = "";
  let stderr = "";
  let requests = 0;
  const raw: boolean[] = [];
  const fetchImpl = async () => {requests += 1; throw new Error("Orbit must stay local");};
  fetchImpl.preconnect = () => undefined;
  const input = Object.assign(new PassThrough(), {isTTY: interactive, isRaw: false, setRawMode: (mode: boolean) => raw.push(mode)});
  const screen = {isTTY: tty, columns, rows: 24, write: (text: string) => {stdout += text; return true;}};
  const code = runCli(args, {
    env: {AGENTSFLEET_STATE_DIR: state, TERM: term},
    stdin: input,
    stdout: screen,
    stderr: {isTTY: tty, columns: 80, rows: 24, write: text => {stderr += text; return true;}},
    fetchImpl,
  });
  const result = async () => ({code: await code, stdout, stderr, requests});
  return {input, screen, raw, result, read: () => stdout};
}

async function appeared(read: () => string, text: string) {
  for (let attempt = 0; attempt < 360 && !read().includes(text); attempt += 1) await Bun.sleep(5);
  expect(read()).toContain(text);
}

test.each([{args: ["pet"]}, {args: ["pet", "--json"]}, {args: ["--json", "pet"]}])("pet keeps the same local machine output for %j", async ({args}) => {
  const result = await harness(args).result();
  expect(result).toEqual({code: 0, stdout: '{\n  "name": "Orbit"\n}\n', stderr: "", requests: 0});
});

test("pet help names the paddle game without making a request", async () => {
  const result = await harness(["pet", "--help"]).result();
  expect(result.code).toBe(0);
  expect(result.stdout).toContain("terminal paddle game");
  expect(result.stdout).toContain("--json");
  expect(result.requests).toBe(0);
});

test("plays, scores, restarts and restores the terminal on Q", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  await appeared(game.read, "Press Space to serve.");
  game.input.write("\u001b[D");
  game.input.write("\u001b[C");
  game.input.write("A");
  for (let move = 0; move < 7; move += 1) game.input.write("D");
  game.input.write("z");
  game.input.write(" ");
  await appeared(game.read, "Score 1");
  game.input.write("R");
  expect(game.read().split("Score 1").at(-1)).toContain("Score 0");
  game.input.write("Q");
  const result = await game.result();
  expect(result).toMatchObject({code: 0, requests: 0, stderr: ""});
  expect(result.stdout).toEndWith(terminal.clearRows(ORBIT_ROWS));
  expect(game.raw).toEqual([true, false]);
  expect(game.input.listenerCount("data")).toBe(0);
});

test.each([SIGINT, SIGTERM, "keyboard"] as const)("restores the terminal after %s", async signal => {
  const beforeInterrupt = process.listenerCount(SIGINT);
  const beforeTerminate = process.listenerCount(SIGTERM);
  const game = harness(["pet"], {tty: true, interactive: true});
  await appeared(game.read, "Press Space to serve.");
  if (signal === "keyboard") game.input.write("\u0003");
  else process.emit(signal);
  const result = await game.result();
  expect(result.code).toBe(130);
  expect(result.requests).toBe(0);
  expect(result.stdout).toEndWith(terminal.clearRows(ORBIT_ROWS));
  expect(result.stderr).toContain("Orbit interrupted");
  expect(process.listenerCount(SIGINT)).toBe(beforeInterrupt);
  expect(process.listenerCount(SIGTERM)).toBe(beforeTerminate);
});

test.each([
  [{tty: true}, "interactive terminal"],
  [{tty: true, interactive: true, term: "dumb"}, "interactive terminal"],
  [{tty: true, interactive: true, columns: 20}, "Make your terminal larger"],
] as const)("prints a useful message when Orbit cannot draw with %j", async (options, message) => {
  const game = harness(["pet"], options);
  expect(await game.result()).toMatchObject({code: 0, stdout: expect.stringContaining(message), requests: 0});
  expect(game.raw).toEqual([]);
});

test("finishes on input EOF and stops using cursor coordinates after resize", async () => {
  const ended = harness(["pet"], {tty: true, interactive: true});
  await appeared(ended.read, "Press Space to serve.");
  ended.input.end();
  expect(await ended.result()).toMatchObject({code: 0, requests: 0});
  const resized = harness(["pet"], {tty: true, interactive: true});
  await appeared(resized.read, "Press Space to serve.");
  resized.screen.columns = 26;
  expect(await resized.result()).toMatchObject({code: 0, stdout: expect.stringContaining("Terminal resized.")});
  expect(resized.read()).not.toContain(terminal.clearRows(ORBIT_ROWS));
  expect(resized.raw).toEqual([true, false]);
});

test("resumes paused input and restores it after the game", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  game.input.pause();
  await appeared(game.read, "Press Space to serve.");
  game.input.write("q");
  expect(await game.result()).toMatchObject({code: 0, requests: 0});
  expect(game.input.isPaused()).toBe(true);
  expect(game.raw).toEqual([true, false]);
});

test("clears the game if input closes without EOF", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  await appeared(game.read, "Press Space to serve.");
  game.input.destroy();
  const result = await game.result();
  expect(result).toMatchObject({code: 0, requests: 0});
  expect(result.stdout).toEndWith(terminal.clearRows(ORBIT_ROWS));
  expect(game.raw).toEqual([true, false]);
});

test("restores raw mode after a terminal write fails", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  const write = game.screen.write;
  game.screen.write = text => {
    if (text.includes("Score 0")) throw new Error("Screen unavailable");
    return write(text);
  };
  const result = await game.result();
  expect(result.stderr).toContain("Could not draw Orbit");
  expect(game.raw).toEqual([true, false]);
});

test("an already canceled game leaves the terminal alone", async () => {
  const input = Object.assign(new PassThrough(), {isTTY: true, setRawMode: () => {throw new Error("raw mode touched");}});
  const screen = {columns: 80, rows: 24, write: () => {throw new Error("screen touched");}};
  expect(await playOrbit(input, screen, AbortSignal.abort())).toBe("interrupted");
});
