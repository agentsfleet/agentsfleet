import type { Key } from "node:readline";
import type { WritableStreamLike } from "../../output/capability.ts";
import { emitKeypressEvents } from "node:readline";
import { PassThrough } from "node:stream";
import { SIGINT, SIGTERM } from "../../constants/signals.ts";
import { moveOrbit, orbitFrame, ORBIT_COLUMNS, ORBIT_ROWS, startOrbit, stepOrbit } from "../../output/orbit.ts";
import { terminal } from "../../output/palette.ts";

type RawInput = NodeJS.ReadableStream & {
  readonly isTTY: boolean;
  readonly isRaw?: boolean;
  readonly readableFlowing?: boolean | null;
  setRawMode(mode: boolean): unknown;
};

type Outcome = "quit" | "interrupted" | "resized" | "small";
type OrbitState = ReturnType<typeof startOrbit>;
type Controls = {
  key: (character: string | undefined, pressed: Key) => void;
  quit: () => void;
  interrupt: () => void;
  fail: (cause: unknown) => void;
};
const TICK_MS = 135;

/** Only an input that supports raw mode can move the paddle without Enter. */
export function isOrbitInput(input: NodeJS.ReadableStream | undefined): input is RawInput {
  return Boolean(input && "isTTY" in input && input.isTTY && "setRawMode" in input && typeof input.setRawMode === "function");
}

export async function playOrbit(input: RawInput, screen: WritableStreamLike, owner: AbortSignal): Promise<Outcome> {
  const {columns, rows} = screen;
  if (columns === undefined || rows === undefined || columns <= ORBIT_COLUMNS || rows <= ORBIT_ROWS) return "small";
  if (owner.aborted) return "interrupted";
  const finished = Promise.withResolvers<Outcome>();
  const resized = () => columns !== screen.columns || rows !== screen.rows;
  let reserved = false;
  let interval: ReturnType<typeof setInterval> | undefined;
  const game = orbitControls((state, started) => {
    try {
      if (resized()) finished.resolve("resized");
      else screen.write(terminal.repaintRows(orbitFrame(state, started)));
    } catch (cause) {
      finished.reject(cause);
    }
  }, finished.resolve);
  const controls = captureKeys(input, owner, {
    quit: () => finished.resolve("quit"),
    interrupt: () => finished.resolve("interrupted"),
    fail: finished.reject,
    key: game.key,
  });
  try {
    screen.write(terminal.reserveRows(ORBIT_ROWS));
    reserved = true;
    game.draw();
    interval = setInterval(() => {
      if (resized()) finished.resolve("resized");
      else game.tick();
    }, TICK_MS);
    return await finished.promise;
  } finally {
    clearInterval(interval);
    try {
      controls();
    } finally {
      if (reserved) screen.write(resized() ? "\r\n" : terminal.clearRows(ORBIT_ROWS));
    }
  }
}

function orbitControls(render: (state: OrbitState, started: boolean) => void, finish: (outcome: Outcome) => void) {
  let state = startOrbit();
  let started = false;
  const draw = () => render(state, started);
  return {
    draw,
    tick() {
      const next = started ? stepOrbit(state) : state;
      if (next === state) return;
      state = next;
      draw();
    },
    key(_character: string | undefined, pressed: Key) {
      switch (pressed.name) {
        case "q": finish("quit"); return;
        case "c": if (pressed.ctrl) finish("interrupted"); return;
        case "space": started = true; break;
        case "r": state = startOrbit(); started = false; break;
        case "left": case "a": state = moveOrbit(state, -1); break;
        case "right": case "d": state = moveOrbit(state, 1); break;
        default: return;
      }
      draw();
    },
  };
}

function captureKeys(input: RawInput, owner: AbortSignal, controls: Controls): () => void {
  const {key, quit, interrupt, fail} = controls;
  const raw = input.isRaw === true;
  const flowing = input.readableFlowing === true;
  const keys = new PassThrough();
  const data = (chunk: Buffer | string) => {keys.write(chunk);};
  input.setRawMode(true);
  emitKeypressEvents(keys);
  keys.on("keypress", key);
  input.on("data", data);
  input.once("end", quit);
  input.once("close", quit);
  input.once("error", fail);
  owner.addEventListener("abort", interrupt, {once: true});
  process.once(SIGINT, interrupt);
  process.once(SIGTERM, interrupt);
  input.resume();
  return () => {
    input.removeListener("data", data);
    keys.destroy();
    input.removeListener("end", quit);
    input.removeListener("close", quit);
    input.removeListener("error", fail);
    owner.removeEventListener("abort", interrupt);
    process.removeListener(SIGINT, interrupt);
    process.removeListener(SIGTERM, interrupt);
    try {input.setRawMode(raw);}
    finally {if (!flowing) input.pause();}
  };
}
