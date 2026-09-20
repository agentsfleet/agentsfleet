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
import { CliConfig } from "./config.ts";
import {
  ui as defaultUi,
  printKeyValue as printKeyValueRaw,
  printSection as printSectionRaw,
  printTable as printTableRaw,
  type TableColumn,
  type TableRow,
} from "../output/index.ts";

type Stream = NodeJS.WritableStream;

/**
 * The registers this CLI writes in.
 *
 * Named as a table rather than a boolean because the fork now lives ON this
 * service, and a service that answers "am I in json mode" with a bool has no
 * room for the third register a streaming format would need. The `--json`
 * flag still selects it; the flag is the user's vocabulary and this is the
 * renderer's.
 *
 * Mirrors `OutputFormat` in supabase/cli `shared/output/output.service.ts`,
 * minus its `stream-json` arm, which nothing here emits yet.
 */
export const OUTPUT_FORMAT = {
  text: "text",
  json: "json",
} as const;

export type OutputFormat = (typeof OUTPUT_FORMAT)[keyof typeof OUTPUT_FORMAT];

export interface OutputShape {
  /**
   * Which register this invocation writes in.
   *
   * On the service, not on CliConfig, so a handler asks the thing that does
   * the writing. Forty-seven sites used to read `config.jsonMode` and then
   * call `output.printJson`, pairing a fact from one service with an action
   * on another — and nothing stopped a site reading one and forgetting the
   * other.
   */
  readonly format: OutputFormat;
  readonly intro: (msg: string) => Effect.Effect<void>;
  readonly info: (msg: string) => Effect.Effect<void>;
  /**
   * One result, in whichever register is active.
   *
   * `data` is the machine payload. In `json` format it IS the output and
   * `msg` is dropped; in `text` the message is printed and `data` is the
   * analytics correlate. So a command cannot answer a script with something
   * it never told a person, or the reverse — both registers are supplied at
   * one call site.
   */
  readonly success: (
    msg: string,
    data?: Record<string, unknown>,
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

export type Output = OutputShape;
export const Output = Context.Service<Output>("agentsfleet/runtime/Output");

interface StreamPair {
  readonly stdout: Stream;
  readonly stderr: Stream;
}

/** Streams plus the register they are written in. */
interface OutputConfig extends StreamPair {
  readonly format: OutputFormat;
}

const writeLine = (stream: Stream, line: string): void => {
  stream.write(`${line}\n`);
};

const JSON_INDENT = 2;

export const makeStdioOutput = ({ stdout, stderr, format }: OutputConfig): OutputShape => ({
  format,
  intro: (msg) => Effect.sync(() => writeLine(stdout, `\n${msg}`)),
  info: (msg) => Effect.sync(() => writeLine(stdout, msg)),
  // The one call that answers both registers. In json the payload IS the
  // answer, so the human sentence is dropped rather than wrapped — a script
  // parsing this wants the record, not a record with a message in it.
  success: (msg, data) =>
    format === OUTPUT_FORMAT.json
      ? Effect.sync(() =>
          writeLine(stdout, JSON.stringify(data ?? { message: msg }, null, JSON_INDENT)),
        )
      : Effect.sync(() => writeLine(stdout, defaultUi.ok(msg))),
  warn: (msg) => Effect.sync(() => writeLine(stderr, defaultUi.warn(msg))),
  error: (msg) => Effect.sync(() => writeLine(stderr, defaultUi.err(`error: ${msg}`))),
  outro: (msg) => Effect.sync(() => writeLine(stdout, `\n${msg}`)),
  printJson: (payload) =>
    Effect.sync(() => writeLine(stdout, JSON.stringify(payload, null, JSON_INDENT))),
  printJsonErr: (payload) =>
    Effect.sync(() => writeLine(stderr, JSON.stringify(payload, null, JSON_INDENT))),
  printKeyValue: (record) =>
    Effect.sync(() => {
      printKeyValueRaw(
        stdout as unknown as Parameters<typeof printKeyValueRaw>[0],
        record,
      );
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

/** The register `--json` selects, as the renderer names it. */
const formatFor = (jsonMode: boolean): OutputFormat =>
  jsonMode ? OUTPUT_FORMAT.json : OUTPUT_FORMAT.text;

// Both layers now read CliConfig, because the format is a property of the
// invocation and the renderer is what needs it. `httpClientLayer` already
// takes its base URL this way.
export const outputStdioLayer: Layer.Layer<Output, never, CliConfig> = Layer.effect(
  Output,
  Effect.gen(function* () {
    const config = yield* CliConfig;
    return Output.of(
      makeStdioOutput({
        stdout: process.stdout,
        stderr: process.stderr,
        format: formatFor(config.jsonMode),
      }),
    );
  }),
);

export const outputFromStreamsLayer = (
  pair: StreamPair,
): Layer.Layer<Output, never, CliConfig> =>
  Layer.effect(
    Output,
    Effect.gen(function* () {
      const config = yield* CliConfig;
      return Output.of(makeStdioOutput({ ...pair, format: formatFor(config.jsonMode) }));
    }),
  );
