// Spinner. Per docs/DESIGN_SYSTEM.md: a minimal monochrome Braille spinner
// is permitted; no rainbow, no gradient, no decoration. Disables when
// !isTTY so log captures stay clean. Success/fail glyphs come from the
// output/glyph module — no hard-coded ✔/✖ pairs.

import { glyph } from "./output/index.js";

function spinnerFrames(style) {
  if (style === "dotmatrix" || style === "matrix") {
    return ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
  }
  return ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
}

export async function withSpinner(opts, work) {
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

export function createSpinner(opts = {}) {
  const stream = opts.stream || process.stderr;
  // Spinner only runs when explicitly enabled by caller AND the target
  // stream is a TTY. !isTTY → no spinner, no escape codes; the work
  // still runs, the user just doesn't see frames.
  const enabled = opts.enabled === true && stream && stream.isTTY === true;
  const label = opts.label || "working";
  const style = opts.style || process.env.ZOMBIE_PROGRESS_STYLE || "spinner";
  const frames = spinnerFrames(style);
  let i = 0;
  let timer = null;

  return {
    start() {
      if (!enabled || timer) return;
      timer = setInterval(() => {
        stream.write(`\r${frames[i % frames.length]} ${label}`);
        i += 1;
      }, 80);
    },
    succeed(message) {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write(`\r${glyph.ok({ stream }).render()} ${message || label}\n`);
    },
    fail(message) {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write(`\r${glyph.error({ stream }).render()} ${message || label}\n`);
    },
    stop() {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write("\r");
    },
  };
}
