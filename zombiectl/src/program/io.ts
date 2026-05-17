// I/O primitives consumed by every command handler. Help rendering
// lives in help.js (commander.Help subclass).

import type { UiTheme, WriteStream } from "../output/index.ts";

type AnyStream = WriteStream | NodeJS.WritableStream;

interface CtxLike {
  stderr?: NodeJS.WritableStream | null;
  jsonMode?: boolean;
}

interface WriteErrorOpts {
  printJson?: (stream: AnyStream, value: unknown) => void;
  writeLine?: (stream: AnyStream, line?: string) => void;
  ui?: Pick<UiTheme, "err"> | { err: (s: string) => string };
}

function writeLine(stream: AnyStream, line: string = ""): void {
  stream.write(`${line}\n`);
}

function printJson(stream: AnyStream, value: unknown): void {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function writeError(
  ctx: CtxLike,
  code: string,
  message: string,
  opts: WriteErrorOpts = {},
): void {
  const pj = opts.printJson || printJson;
  const wl = opts.writeLine || writeLine;
  const u = opts.ui || { err: (s: string) => s };
  if (!ctx.stderr) return;
  if (ctx.jsonMode) {
    pj(ctx.stderr, { error: { code, message } });
  } else {
    wl(ctx.stderr, u.err(message));
  }
}

export { printJson, writeError, writeLine };
