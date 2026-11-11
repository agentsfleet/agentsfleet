import type {WritableStreamLike} from "./capability.ts";
import {Effect} from "effect";
import {palette} from "./palette.ts";
import {REDUCED_MOTION_ENV} from "../constants/env.ts";

const FRAMES = [" ,(o,o), ", " /(o,o)\\ ", " \\(o,o)/ ", " /(o,o)\\ ", " ,(o,o), ", " ,(o,o), ", " ,(-,-), ", " ,(o,o), "] as const;
const FRAME_MS = 250;

/** The request scope interrupts the animation before clearing its status line. */
export function startProgress(stream: WritableStreamLike, label: string, enabled: boolean, env: NodeJS.ProcessEnv = process.env) {
  return Effect.gen(function* () {
    if (!enabled || !stream.isTTY) return;
    const width = label.length + FRAMES[0].length + 1;
    if (env[REDUCED_MOTION_ENV] || env.CI || env.TERM === "dumb" || (stream.columns !== undefined && stream.columns <= width)) {
      stream.write(`${label}\n`);
      return;
    }
    let frame = 0;
    const frames = FRAMES.map(value => palette.pulse(value, {env, stream, warnStream: stream}));
    const draw = Effect.sync(() => {stream.write(`\r${frames[frame % frames.length]} ${label}`); frame += 1;});
    yield* Effect.addFinalizer(() => Effect.sync(() => {
      stream.write(`\r${" ".repeat(width)}\r`);
    }));
    yield* draw;
    yield* Effect.forkScoped(Effect.forever(Effect.sleep(FRAME_MS).pipe(Effect.andThen(draw))));
  });
}
