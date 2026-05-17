// Spinner. Per docs/DESIGN_SYSTEM.md: a minimal monochrome Braille spinner
// is permitted; no rainbow, no gradient, no decoration. Disables when
// !isTTY so log captures stay clean. Success/fail glyphs come from the
// output/glyph module — no hard-coded ✔/✖ pairs.

import { glyph } from "./output/index.ts";
import type { SpinnerHandle, SpinnerOptions } from "./commands/types.ts";

function spinnerFrames(style: string | undefined): string[] {
  if (style === "dotmatrix" || style === "matrix") {
    return ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
  }
  return ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
}

export async function withSpinner<T>(
  opts: SpinnerOptions,
  work: () => Promise<T>,
): Promise<T> {
  const spin = createSpinner(opts);
  spin.start();
  try {
    const out = await work();
    spin.succeed();
    return out;
  } catch (err) {
    spin.fail();
    throw err;
  }
}

interface TtyWritableStream extends NodeJS.WritableStream {
  readonly isTTY?: boolean;
}

export function createSpinner(opts: SpinnerOptions = {}): SpinnerHandle {
  const stream = (opts.stream ?? process.stderr) as TtyWritableStream;
  // Spinner only runs when explicitly enabled by caller AND the target
  // stream is a TTY. !isTTY → no spinner, no escape codes; the work
  // still runs, the user just doesn't see frames.
  const enabled = opts.enabled === true && stream != null && stream.isTTY === true;
  const label = opts.label || "working";
  const style = opts.style || process.env["ZOMBIE_PROGRESS_STYLE"] || "spinner";
  const frames = spinnerFrames(style);
  let i = 0;
  let timer: ReturnType<typeof setInterval> | null = null;

  return {
    start(): void {
      if (!enabled || timer) return;
      timer = setInterval(() => {
        const frame = frames[i % frames.length] ?? "";
        stream.write(`\r${frame} ${label}`);
        i += 1;
      }, 80);
    },
    succeed(message?: string): void {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write(`\r${glyph.ok({ stream }).render()} ${message || label}\n`);
    },
    fail(message?: string): void {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write(`\r${glyph.error({ stream }).render()} ${message || label}\n`);
    },
    stop(): void {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write("\r");
    },
  };
}
