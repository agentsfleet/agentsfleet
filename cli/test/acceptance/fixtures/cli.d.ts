// Ambient declaration for the still-JS acceptance spawner. cli.js stays
// JS until §14 D43 flips the spawner from `bun` back to `node` against
// the tsc-emitted dist/. For now the .d.ts gives strict TS callers
// honest signatures at the import seam — mirrors src/lib/analytics.d.ts.
//
// Shapes match cli.js verbatim. Adding fields here without adding them
// to the .js file is a lie; removing fields the .js file produces is
// also a lie.

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

// Resolves the spawn target (worktree `node dist/bin/...` vs global
// `agentsfleet`) — shared by runFleetctl and the pty harness.
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
