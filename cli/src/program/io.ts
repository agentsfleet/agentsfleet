// I/O primitives consumed by every command handler. Help rendering belongs
// to the CLI library's formatter — see program/entry/help-formatter.ts.

import type { UiTheme, WriteStream } from "../output/index.ts";

type AnyStream = WriteStream | NodeJS.WritableStream;

interface CtxLike {
  stderr?: NodeJS.WritableStream | null;
  jsonMode?: boolean;
}

interface WriteErrorOpts {
  ui: Pick<UiTheme, "err">;
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
  opts: WriteErrorOpts,
): void {
  if (!ctx.stderr) return;
  if (ctx.jsonMode) {
    printJson(ctx.stderr, { error: { code, message } });
  } else {
    writeLine(ctx.stderr, opts.ui.err(message));
  }
}

export { printJson, writeError, writeLine };
