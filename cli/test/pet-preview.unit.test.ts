import {afterEach, beforeEach, expect, spyOn, test} from "bun:test";
import {mkdtempSync, rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {PassThrough} from "node:stream";
import {stripVTControlCharacters} from "node:util";
import {runCli} from "../src/cli.ts";
import {playOrbit} from "../src/commands/pet/play.ts";
import {SIGINT, SIGTERM} from "../src/constants/signals.ts";
import {ORBIT_ROWS} from "../src/output/orbit.ts";
import {terminal} from "../src/output/palette.ts";

let state: string;
beforeEach(() => {state = mkdtempSync(join(tmpdir(), "agentsfleet-pet-"));});
afterEach(() => rmSync(state, {recursive: true, force: true}));

function harness(args: readonly string[], {tty = false, interactive = false, term = "xterm-256color", columns = 80, noColor = ""} = {}) {
  let stdout = "";
  let stderr = "";
  let requests = 0;
  const raw: boolean[] = [];
  const fetchImpl = async () => {requests += 1; throw new Error("Orbit must stay local");};
  fetchImpl.preconnect = () => undefined;
  const input = Object.assign(new PassThrough(), {isTTY: interactive, isRaw: false, setRawMode: (mode: boolean) => raw.push(mode)});
  const screen = {isTTY: tty, columns, rows: 24, write: (text: string) => {stdout += text; return true;}};
  const code = runCli(args, {
    env: {AGENTSFLEET_STATE_DIR: state, TERM: term, NO_COLOR: noColor},
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
  await appeared(game.read, "Space serve");
  game.input.write("\u001b[D");
  game.input.write("\u001b[C");
  game.input.write("A");
  game.input.write("D");
  game.input.write("z");
  game.input.write(" ");
  const plain = () => stripVTControlCharacters(game.read());
  await appeared(plain, "Score 00050 Best 00050");
  game.input.write("R");
  expect(plain().split("Score 00050").at(-1)).toContain("Score 00000 Best 00050");
  game.input.write("Q");
  const result = await game.result();
  expect(result).toMatchObject({code: 0, requests: 0, stderr: ""});
  expect(result.stdout).toEndWith(terminal.clearRows(ORBIT_ROWS));
  expect(result.stdout).toContain("\u001b[38;5;");
  expect(game.raw).toEqual([true, false]);
  expect(game.input.listenerCount("data")).toBe(0);
});

test("keeps the game readable in a terminal with color disabled", async () => {
  const game = harness(["pet"], {tty: true, interactive: true, noColor: "1"});
  await appeared(game.read, "Orbit  Score 00000 Best 00000");
  game.input.write("q");
  const result = await game.result();
  expect(result.stdout).toContain("Lives [ooo]");
  expect(result.stdout).not.toContain("\u001b[38;5;");
});

test("starts muted and plays a star effect after sound is enabled", async () => {
  const lookup = spyOn(Bun, "which").mockReturnValue(null);
  const game = harness(["pet"], {tty: true, interactive: true});
  try {
    await appeared(game.read, "S sound off");
    expect(game.read()).not.toContain(terminal.bell);
    game.input.write("s ");
    await appeared(() => stripVTControlCharacters(game.read()), "Score 00050 Best 00050");
    expect(game.read().split(terminal.bell)).toHaveLength(3);
    expect(game.read()).toContain("S sound on");
    game.input.write("s");
    expect(game.read().split("S sound on").at(-1)).toContain("S sound off");
  } finally {
    game.input.write("q");
    await game.result();
    lookup.mockRestore();
  }
});

test.each([SIGINT, SIGTERM, "keyboard"] as const)("restores the terminal after %s", async signal => {
  const beforeInterrupt = process.listenerCount(SIGINT);
  const beforeTerminate = process.listenerCount(SIGTERM);
  const game = harness(["pet"], {tty: true, interactive: true});
  await appeared(game.read, "Space serve");
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
  await appeared(ended.read, "Space serve");
  ended.input.end();
  expect(await ended.result()).toMatchObject({code: 0, requests: 0});
  const resized = harness(["pet"], {tty: true, interactive: true});
  await appeared(resized.read, "Space serve");
  resized.screen.columns = 26;
  expect(await resized.result()).toMatchObject({code: 0, stdout: expect.stringContaining("Terminal resized.")});
  expect(resized.read()).not.toContain(terminal.clearRows(ORBIT_ROWS));
  expect(resized.raw).toEqual([true, false]);
});

test("resumes paused input and restores it after the game", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  game.input.pause();
  await appeared(game.read, "Space serve");
  game.input.write("q");
  expect(await game.result()).toMatchObject({code: 0, requests: 0});
  expect(game.input.isPaused()).toBe(true);
  expect(game.raw).toEqual([true, false]);
});

test("clears the game if input closes without EOF", async () => {
  const game = harness(["pet"], {tty: true, interactive: true});
  await appeared(game.read, "Space serve");
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
    if (stripVTControlCharacters(text).includes("Score 00000")) throw new Error("Screen unavailable");
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
