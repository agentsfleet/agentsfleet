// Shared terminal REPL helpers. The loop is intentionally small and
// dependency-injected so command tests can drive stdin, stdout, and
// interrupt delivery without a real terminal.

import { EventEmitter } from "node:events";
import * as readline from "node:readline";
import { SIGINT } from "../constants/signals.ts";
import { isTty, type IsTtyStream } from "../output/capability.ts";

export const STEER_REPL_PROMPT = "> ";

export interface ReplInputStream extends NodeJS.ReadableStream, IsTtyStream {}
export interface ReplOutputStream extends NodeJS.WritableStream {}

export interface ReplSignalSource {
  on(event: typeof SIGINT, listener: () => void): unknown;
  off(event: typeof SIGINT, listener: () => void): unknown;
}

export type ReplTurn = (
  message: string,
  signal: AbortSignal,
) => Promise<void>;

export interface ReplOptions {
  readonly input: ReplInputStream;
  readonly output: ReplOutputStream;
  readonly runTurn: ReplTurn;
  readonly onTurnError?: (err: unknown) => Promise<void> | void;
  readonly signalSource?: ReplSignalSource;
}

const EMPTY_LINE = "";
const TEXT_ENCODING = "utf8";
const EXIT_OK = 0;
const EXIT_SIGINT = 130;
const EVENT_CLOSE = "close";

export const shouldEnterSteerRepl = (
  stdin: IsTtyStream,
  message: string | undefined,
  forceTty: boolean,
): boolean => message === undefined && (forceTty || isTty(stdin));

export async function readPipedMessage(input: NodeJS.ReadableStream): Promise<string> {
  input.setEncoding(TEXT_ENCODING);
  let body = EMPTY_LINE;
  for await (const chunk of input) {
    body += typeof chunk === "string" ? chunk : String(chunk);
  }
  return body.trim();
}

export async function runSteerRepl(options: ReplOptions): Promise<typeof EXIT_OK | typeof EXIT_SIGINT> {
  const signalSource = options.signalSource ?? process;
  const rl = readline.createInterface({
    input: options.input,
    output: options.output,
    terminal: isTty(options.input),
  });

  let interrupted = false;
  let currentTurn = new AbortController();
  const interrupt = (): void => {
    interrupted = true;
    currentTurn.abort();
    rl.close();
  };

  signalSource.on(SIGINT, interrupt);
  rl.on(SIGINT, interrupt);
  // Bun 1.4 hardened readline: prompt() on a closed interface throws
  // ERR_USE_AFTER_CLOSE instead of returning silently. Both call sites can run
  // against a closed interface — an interrupt closes it, and stdin EOF closes
  // it before the loop body's re-prompt — so track the close and ask, rather
  // than catching a throw that would be indistinguishable from a real fault.
  // The runtime carries `rl.closed`, but the readline typings do not declare
  // it, and a cast to reach it would buy nothing this listener does not.
  let closed = false;
  rl.on(EVENT_CLOSE, () => {
    closed = true;
  });
  const prompt = (): void => {
    if (!closed) rl.prompt();
  };

  try {
    rl.setPrompt(STEER_REPL_PROMPT);
    prompt();
    for await (const raw of rl) {
      const line = raw.trim();
      if (line !== EMPTY_LINE) {
        currentTurn = new AbortController();
        try {
          await options.runTurn(line, currentTurn.signal);
        } catch (err) {
          if (interrupted) return EXIT_SIGINT;
          if (!options.onTurnError) throw err;
          await options.onTurnError(err);
        }
      }
      if (interrupted) return EXIT_SIGINT;
      prompt();
    }
    return interrupted ? EXIT_SIGINT : EXIT_OK;
  } finally {
    signalSource.off(SIGINT, interrupt);
    rl.off(SIGINT, interrupt);
    if (!currentTurn.signal.aborted) currentTurn.abort();
    rl.close();
  }
}

export const ReplSignalEmitter = EventEmitter as {
  new(): EventEmitter & ReplSignalSource;
};
