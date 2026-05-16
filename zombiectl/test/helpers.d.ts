// Ambient declarations for ./helpers.js. helpers.js stays as `.js` until
// its D42 wave migrates the full file; this .d.ts surfaces the shape that
// already-migrated `.ts` tests need. Mirrors the pattern src/lib/analytics.d.ts
// uses for analytics.js.

import type { Writable } from "node:stream";
import type { ParsedArgs, CommandCtx, CommandDeps, Workspaces } from "../src/commands/types.ts";

export { ApiError } from "../src/lib/http.ts";

export function makeNoop(): Writable;
export function makeBufferStream(): { stream: Writable; read: () => string };
export const ui: {
  ok: (s: string) => string;
  err: (s: string) => string;
  info: (s: string) => string;
  dim: (s: string) => string;
  head: (s: string) => string;
};

// Test-only shim re-creating the legacy createCoreHandlers shape. The
// JS implementation returns a heterogeneous bag of arity-1 handlers;
// individual tests narrow per-call so the loose Record type is honest.
export function createCoreHandlers(
  ctx: CommandCtx,
  workspaces: Workspaces,
  deps: CommandDeps,
): Record<string, (args?: readonly string[]) => Promise<number>>;

export function commandTenant(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number>;

export function commandBilling(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number>;

export function commandZombieDispatch(
  ctx: CommandCtx,
  args: readonly string[],
  workspaces: Workspaces,
  deps: CommandDeps,
): Promise<number>;

export function buildParsed(tokens?: readonly string[]): ParsedArgs;

export const AGENT_ID: string;
export const AGENT_NAME: string;
export const WS_ID: string;
export const SCORE_ID_1: string;
export const SCORE_ID_2: string;
export const RUN_ID_1: string;
export const RUN_ID_2: string;
export const PVER_ID: string;
