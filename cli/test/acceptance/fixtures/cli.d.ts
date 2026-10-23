// TypeScript signatures for the JavaScript acceptance spawner in cli.js.

import type { ChildProcessWithoutNullStreams } from "node:child_process";

export class TimeoutError extends Error {
  readonly args: ReadonlyArray<string>;
  readonly timeoutMs: number | undefined;
  constructor(message: string, opts?: { args?: ReadonlyArray<string>; timeoutMs?: number });
}

export interface RunResult {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly durationMs: number;
  readonly signal?: NodeJS.Signals | null;
}

export interface RunOptions {
  readonly env: Readonly<Record<string, string>>;
  readonly stdin?: string;
  readonly timeoutMs?: number;
  readonly cwd?: string;
  readonly binary?: "worktree" | "global";
}

export interface BinarySpec {
  readonly command: string;
  readonly prefixArgs: ReadonlyArray<string>;
}

// The ordinary runner and pseudo-terminal harness share the Bun spawn target.
export function resolveBinary(opts?: Pick<RunOptions, "binary">): BinarySpec;

export function runFleetctl(args: ReadonlyArray<string>, opts: RunOptions): Promise<RunResult>;

export function spawnFleetctl(
  args: ReadonlyArray<string>,
  opts: Pick<RunOptions, "env" | "cwd" | "binary">,
): ChildProcessWithoutNullStreams;

// composeEnv only forwards string-coercible scalars (string|number|boolean)
// and drops null/undefined entries — the implementation calls String(value).
export function composeEnv(
  fields: Readonly<Record<string, string | number | boolean | null | undefined>>,
): Record<string, string>;
