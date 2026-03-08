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
  const enabled = opts.enabled === true;
  const stream = opts.stream || process.stderr;
  const label = opts.label || "working";
  const frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
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
      stream.write(`\r✔ ${message || label}\n`);
    },
    fail(message) {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write(`\r✖ ${message || label}\n`);
    },
    stop() {
      if (!enabled) return;
      if (timer) clearInterval(timer);
      timer = null;
      stream.write("\r");
    },
  };
}
