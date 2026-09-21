// One place that mirrors the Output service.
//
// Thirty test doubles used to restate all sixteen fields of `OutputShape` by
// hand. That has two costs, and both have been paid:
//
//   - Adding a field to the service meant thirty edits. `stdoutIsTty` cost
//     exactly that.
//   - Worse, a double can be WRONG rather than incomplete, and the type
//     checker cannot see it. When `--json` moved from `CliConfig.jsonMode`
//     onto `Output.format`, thirty-three tests kept passing against doubles
//     that hardcoded `format: text` while their CliConfig double said
//     `jsonMode: true` — green tests proving nothing about the branch they
//     were named after.
//
// So the defaults live here, once, and a test spreads only what it asserts
// on. The register is expressed as `jsonMode` rather than a raw format, so a
// test that means "json" cannot quietly get "text".

import { Effect } from "effect";
import {
  OUTPUT_FORMAT,
  type OutputFormat,
  type OutputShape,
} from "../src/services/output.ts";

export interface OutputDoubleOptions {
  /** The register under test. Maps to `format`, so the two cannot disagree. */
  readonly jsonMode?: boolean;
  /**
   * The register, stated directly.
   *
   * For a helper whose own caller already varies a format; `jsonMode` is the
   * spelling to prefer, because it says what the test is about rather than
   * what the service field happens to be called.
   */
  readonly format?: OutputFormat;
  /** Whether the invocation's stdout is a terminal. Recorders are not. */
  readonly stdoutIsTty?: boolean;
}

/**
 * A silent `OutputShape` carrying the invocation's register.
 *
 * Every method is a no-op; a test overrides the handful it observes by
 * spreading over the result. Silence is the right default because a test
 * asserts on what it captured, and an un-captured write is noise in the
 * suite's own output.
 */
export const outputDouble = (
  options: OutputDoubleOptions = {},
): OutputShape => ({
  format: options.format ?? (options.jsonMode === true ? OUTPUT_FORMAT.json : OUTPUT_FORMAT.text),
  stdoutIsTty: options.stdoutIsTty ?? false,
  intro: () => Effect.void,
  info: () => Effect.void,
  success: () => Effect.void,
  warn: () => Effect.void,
  error: () => Effect.void,
  outro: () => Effect.void,
  printJson: () => Effect.void,
  printJsonErr: () => Effect.void,
  printKeyValue: () => Effect.void,
  printSection: () => Effect.void,
  printTable: () => Effect.void,
});

/** The register a double was built with, for a test that branches on it. */
export const formatFor = (jsonMode: boolean): OutputFormat =>
  jsonMode ? OUTPUT_FORMAT.json : OUTPUT_FORMAT.text;
