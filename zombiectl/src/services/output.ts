// Output service — single audit-bearing surface for everything a
// command writes to the user. intro/info/success/warn/error render via
// the existing ui theme (preserved for visual parity); promptText and
// promptConfirm front the readline interaction.
//
// Every success/error emit carries a `meta` record. Commands attach
// `{ command, ... }` so a downstream analytics layer can correlate the
// emit with the command that produced it. The Output service itself
// is intentionally side-effect-only — the analytics correlation is
// driven by the command code, not by this service.

import { Effect, Layer, Context } from "effect";
import {
  ui as defaultUi,
  printSection as printSectionRaw,
  printTable as printTableRaw,
  type TableColumn,
  type TableRow,
} from "../output/index.ts";

type Stream = NodeJS.WritableStream;

export interface OutputShape {
  readonly intro: (msg: string) => Effect.Effect<void>;
  readonly info: (msg: string) => Effect.Effect<void>;
  readonly success: (
    msg: string,
    meta?: Record<string, unknown>,
  ) => Effect.Effect<void>;
  readonly warn: (msg: string) => Effect.Effect<void>;
  readonly error: (
    msg: string,
    meta?: Record<string, unknown>,
  ) => Effect.Effect<void>;
  readonly outro: (msg: string) => Effect.Effect<void>;
  readonly printJson: (payload: unknown) => Effect.Effect<void>;
  readonly printJsonErr: (payload: unknown) => Effect.Effect<void>;
  readonly printKeyValue: (record: Record<string, string>) => Effect.Effect<void>;
  readonly printSection: (title: string) => Effect.Effect<void>;
  readonly printTable: (
    columns: ReadonlyArray<TableColumn>,
    rows: ReadonlyArray<TableRow>,
  ) => Effect.Effect<void>;
}

export class Output extends Context.Service<Output, OutputShape>()(
  "zombiectl/runtime/Output",
) {}

interface StreamPair {
  readonly stdout: Stream;
  readonly stderr: Stream;
}

const writeLine = (stream: Stream, line: string): void => {
  stream.write(`${line}\n`);
};

const JSON_INDENT = 2;

export const makeStdioOutput = ({ stdout, stderr }: StreamPair): OutputShape => ({
  intro: (msg) => Effect.sync(() => writeLine(stdout, `\n${msg}`)),
  info: (msg) => Effect.sync(() => writeLine(stdout, msg)),
  success: (msg) => Effect.sync(() => writeLine(stdout, defaultUi.ok(msg))),
  warn: (msg) => Effect.sync(() => writeLine(stderr, defaultUi.warn(msg))),
  error: (msg) => Effect.sync(() => writeLine(stderr, defaultUi.err(`error: ${msg}`))),
  outro: (msg) => Effect.sync(() => writeLine(stdout, `\n${msg}`)),
  printJson: (payload) =>
    Effect.sync(() => writeLine(stdout, JSON.stringify(payload, null, JSON_INDENT))),
  printJsonErr: (payload) =>
    Effect.sync(() => writeLine(stderr, JSON.stringify(payload, null, JSON_INDENT))),
  printKeyValue: (record) =>
    Effect.sync(() => {
      const keyWidth = Object.keys(record).reduce((m, k) => Math.max(m, k.length), 0);
      for (const [key, value] of Object.entries(record)) {
        writeLine(stdout, `  ${key.padEnd(keyWidth)}  ${value}`);
      }
    }),
  printSection: (title) =>
    Effect.sync(() => {
      printSectionRaw(stdout as unknown as Parameters<typeof printSectionRaw>[0], title);
    }),
  printTable: (columns, rows) =>
    Effect.sync(() => {
      printTableRaw(stdout as unknown as Parameters<typeof printTableRaw>[0], columns, rows);
    }),
});

export const outputStdioLayer: Layer.Layer<Output> = Layer.succeed(
  Output,
  Output.of(makeStdioOutput({ stdout: process.stdout, stderr: process.stderr })),
);

export const outputFromStreamsLayer = (pair: StreamPair): Layer.Layer<Output> =>
  Layer.succeed(Output, Output.of(makeStdioOutput(pair)));
