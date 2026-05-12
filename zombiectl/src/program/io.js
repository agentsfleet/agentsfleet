// I/O primitives consumed by every command handler. printHelp was
// deleted when the commander refactor landed — commander.Help (via
// the ZombieHelp subclass in help.js) renders the help body now.

function writeLine(stream, line = "") {
  stream.write(`${line}\n`);
}

function printJson(stream, value) {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function writeError(ctx, code, message, opts = {}) {
  const pj = opts.printJson || printJson;
  const wl = opts.writeLine || writeLine;
  const u = opts.ui || { err: (s) => s };
  if (ctx.jsonMode) {
    pj(ctx.stderr, { error: { code, message } });
  } else {
    wl(ctx.stderr, u.err(message));
  }
}

export {
  printJson,
  writeError,
  writeLine,
};
