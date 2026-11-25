import type { Key } from "node:readline";
import type { WritableStreamLike } from "../../output/capability.ts";
import { emitKeypressEvents } from "node:readline";
import { PassThrough } from "node:stream";
import { SIGINT, SIGTERM } from "../../constants/signals.ts";
import { moveOrbit, orbitFrame, orbitTickMs, ORBIT_COLUMNS, ORBIT_ROWS, serveOrbit, startOrbit, stepOrbit } from "../../output/orbit.ts";
import { terminal, type StyleOpts } from "../../output/palette.ts";
import { orbitAudio, type OrbitSound } from "./audio.ts";

type RawInput = NodeJS.ReadableStream & {
  readonly isTTY: boolean;
  readonly isRaw?: boolean;
  readonly readableFlowing?: boolean | null;
  setRawMode(mode: boolean): unknown;
};

type Outcome = "quit" | "interrupted" | "resized" | "small";
type Controls = {
  key: (character: string | undefined, pressed: Key) => void;
  quit: () => void;
  interrupt: () => void;
  fail: (cause: unknown) => void;
};

/** Only an input that supports raw mode can move the paddle without Enter. */
export function isOrbitInput(input: NodeJS.ReadableStream | undefined): input is RawInput {
  return Boolean(input && "isTTY" in input && input.isTTY && "setRawMode" in input && typeof input.setRawMode === "function");
}

export async function playOrbit(input: RawInput, screen: WritableStreamLike, owner: AbortSignal, style?: StyleOpts): Promise<Outcome> {
  const {columns, rows} = screen;
  if (columns === undefined || rows === undefined || columns <= ORBIT_COLUMNS || rows <= ORBIT_ROWS) return "small";
  if (owner.aborted) return "interrupted";
  const finished = Promise.withResolvers<Outcome>();
  const resized = () => columns !== screen.columns || rows !== screen.rows;
  let reserved = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const draw = (frame: readonly string[]) => {
    try {
      if (resized()) finished.resolve("resized");
      else screen.write(terminal.repaintRows(frame));
    } catch (cause) {
      finished.reject(cause);
    }
  };
  const audio = orbitAudio(screen);
  const game = orbitControls(draw, finished.resolve, audio, style);
  const tick = () => {
    if (resized()) {
      finished.resolve("resized");
      return;
    }
    game.tick();
    timer = setTimeout(tick, game.delay());
  };
  const controls = captureKeys(input, owner, {
    quit: () => finished.resolve("quit"),
    interrupt: () => finished.resolve("interrupted"),
    fail: finished.reject,
    key: game.key,
  });
  try {
    // Resolve the palette's basic-color notice before reserving screen rows.
    const opening = game.frame();
    screen.write(terminal.reserveRows(ORBIT_ROWS));
    reserved = true;
    draw(opening);
    timer = setTimeout(tick, game.delay());
    return await finished.promise;
  } finally {
    clearTimeout(timer);
    try {
      controls();
    } finally {
      try {audio.stop();}
      finally {if (reserved) screen.write(resized() ? "\r\n" : terminal.clearRows(ORBIT_ROWS));}
    }
  }
}

function orbitControls(
  render: (frame: readonly string[]) => void, finish: (outcome: Outcome) => void,
  audio: ReturnType<typeof orbitAudio>, style?: StyleOpts,
) {
  let state = startOrbit();
  let sound = false;
  const frame = () => orbitFrame(state, style, sound);
  return {
    frame,
    delay: () => orbitTickMs(state),
    tick() {
      const next = stepOrbit(state);
      if (next === state) return;
      if (sound) {
        const effect = orbitEffect(state, next);
        if (effect) audio.play(effect);
      }
      state = next;
      render(frame());
    },
    key(_character: string | undefined, pressed: Key) {
      switch (pressed.name) {
        case "q": finish("quit"); return;
        case "c": if (pressed.ctrl) finish("interrupted"); return;
        case "space": state = serveOrbit(state); break;
        case "r": state = startOrbit(state.best); break;
        case "s":
          sound = !sound;
          if (sound) audio.play("bounce");
          else audio.stop();
          break;
        case "left": case "a": state = moveOrbit(state, -1); break;
        case "right": case "d": state = moveOrbit(state, 1); break;
        default: return;
      }
      render(frame());
    },
  };
}

function orbitEffect(previous: ReturnType<typeof startOrbit>, next: ReturnType<typeof startOrbit>): OrbitSound | undefined {
  if (next.lives < previous.lives) return "miss";
  if (next.target !== previous.target) return "star";
  if (next.returns > previous.returns) return "bounce";
  return undefined;
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
