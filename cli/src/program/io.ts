// I/O primitives consumed by every command handler. Help rendering belongs
// to the CLI library's formatter — see program/entry/help-formatter.ts.

import type { WriteStream } from "../output/index.ts";
import { BLOCK_SEVERITY, splitFix, writeErrorBlock } from "../output/error-block.ts";
import type { WritableStreamLike } from "../output/capability.ts";

type AnyStream = WriteStream | NodeJS.WritableStream;

interface CtxLike {
  stderr?: NodeJS.WritableStream | null;
  jsonMode?: boolean;
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
): void {
  if (!ctx.stderr) return;
  if (ctx.jsonMode) {
    printJson(ctx.stderr, { error: { code, message } });
  } else {
    const { message: head, fix } = splitFix(message);
    writeErrorBlock(ctx.stderr as unknown as WritableStreamLike, {
      severity: BLOCK_SEVERITY.error,
      message: head,
      fields: { code, ...(fix !== undefined ? { suggestion: fix } : {}) },
    });
  }
}

export { printJson, writeError, writeLine };
