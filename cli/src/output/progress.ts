import type { WritableStreamLike } from "./capability.ts";
import { Effect } from "effect";
import { palette } from "./palette.ts";
import { detectColorMode } from "./capability.ts";
import { REDUCED_MOTION_ENV } from "../constants/env.ts";
import { visibleLength } from "./text.ts";

const FRAME_MS = 135;
const LANE = 6;

/** The request scope interrupts the animation before clearing its status line. */
export function startProgress(stream: WritableStreamLike, label: string, enabled: boolean, env: NodeJS.ProcessEnv = process.env) {
  return Effect.gen(function* () {
    if (!enabled || !stream.isTTY) return;
    const width = visibleLength(label) + LANE + 4;
    const fits = () => stream.columns === undefined || stream.columns > width;
    if (env[REDUCED_MOTION_ENV] || env.CI || env.TERM === "dumb" || !fits()) {
      stream.write(`${label}\n`);
      return;
    }
    const style = {env, stream, mode: detectColorMode(env, stream), warnStream: stream};
    let position = 0;
    let direction = 1;
    let reflowed = false;
    const draw = Effect.sync(() => {
      if (reflowed) return;
      if (!fits()) {
        reflowed = true;
        stream.write("\r\n");
        return;
      }
      const frame = `[${" ".repeat(position)}o${" ".repeat(LANE - position)}]`;
      stream.write(`\r${palette.pulse(frame, style)} ${label}`);
      if (position === LANE) direction = -1;
      if (position === 0) direction = 1;
      position += direction;
    });
    yield* Effect.addFinalizer(() => Effect.sync(() => {
      if (!reflowed) stream.write(fits() ? `\r${" ".repeat(width)}\r` : "\r\n");
    }));
    yield* draw;
    yield* Effect.forkScoped(Effect.forever(Effect.sleep(FRAME_MS).pipe(Effect.andThen(draw))));
  });
}
