import type {WritableStreamLike} from "./capability.ts";

const FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] as const;
const FRAME_MS = 80;

/** The caller owns the returned cleanup and runs it before rendering the result. */
export function startProgress(stream: WritableStreamLike, label: string, enabled: boolean): () => void {
  if (!enabled || !stream.isTTY) return () => undefined;
  let frame = 0;
  const draw = () => {stream.write(`\r${FRAMES[frame % FRAMES.length]} ${label}`); frame += 1;};
  draw();
  const timer = setInterval(draw, FRAME_MS);
  timer.unref();
  return () => {clearInterval(timer); stream.write(`\r${" ".repeat(label.length + 2)}\r`);};
}
