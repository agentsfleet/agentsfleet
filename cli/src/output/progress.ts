import type {WritableStreamLike} from "./capability.ts";
import {Effect} from "effect";
import {palette, terminal} from "./palette.ts";
import {ColorMode, detectColorMode} from "./capability.ts";
import {orbitFrame} from "./orbit.ts";
import {PET_DISPLAY, PET_FRAME_MS, PET_IDLE_FRAMES, PET_MIN_COLUMNS, PET_MIN_ROWS, PET_ROWS, PET_STATE,
  PET_WORKING_END_FRAME, type PetDisplay} from "../constants/pet.ts";
import {REDUCED_MOTION_ENV} from "../constants/env.ts";

const FRAMES = [" (o.o)~ ", " (o.o)~~", " ~(o.o)~", "~(o.o)~ ", " (o.o)~ ", " (-.-)~ "] as const;

/** The request scope interrupts the animation before clearing its status line. */
export function startProgress(stream: WritableStreamLike, label: string, enabled: boolean, env: NodeJS.ProcessEnv = process.env,
  display: PetDisplay = PET_DISPLAY.compact) {
  return Effect.gen(function* () {
    if (!enabled || !stream.isTTY) return;
    const width = label.length + FRAMES[0].length + 2;
    if (env[REDUCED_MOTION_ENV] || env.CI || env.TERM === "dumb" || (stream.columns !== undefined && stream.columns <= width)) {
      stream.write(`${display === PET_DISPLAY.preview ? `${FRAMES[0]} ` : ""}${label}\n`);
      return;
    }
    const mode = detectColorMode(env, stream);
    const preview = display === PET_DISPLAY.preview && mode === ColorMode.XTERM256 &&
      (stream.columns ?? 0) >= PET_MIN_COLUMNS && (stream.rows ?? 0) >= PET_MIN_ROWS;
    const columns = stream.columns, rows = stream.rows;
    const sameSize = () => stream.columns === columns && stream.rows === rows;
    let resized = false;
    let frame = 0;
    const frames = FRAMES.map(value => palette.pulse(value, {env, stream, warnStream: stream}));
    const draw = Effect.sync(() => {
      if (resized) return;
      if (preview && !sameSize()) { resized = true; stream.write("\r\n"); return; }
      if (preview) {
        const state = frame < PET_IDLE_FRAMES ? PET_STATE.idle : frame < PET_WORKING_END_FRAME ? PET_STATE.working : PET_STATE.done;
        const tick = frame - (state === PET_STATE.working ? PET_IDLE_FRAMES : state === PET_STATE.done ? PET_WORKING_END_FRAME : 0);
        stream.write(terminal.repaintRows(orbitFrame(tick, state, {env, stream, mode})));
      } else stream.write(`\r${frames[frame % frames.length]} ${label}`);
      frame += 1;
    });
    yield* Effect.addFinalizer(() => Effect.sync(() => {
      // Resizing can reflow the block, so its old cursor coordinates no longer belong to us.
      if (preview && (resized || !sameSize())) { if (!resized) stream.write("\r\n"); return; }
      stream.write(preview ? terminal.clearRows(PET_ROWS) : `\r${" ".repeat(width)}\r`);
    }));
    if (preview) stream.write(terminal.reserveRows(PET_ROWS));
    yield* draw;
    yield* Effect.forkScoped(Effect.forever(Effect.sleep(PET_FRAME_MS).pipe(Effect.andThen(draw))));
  });
}
