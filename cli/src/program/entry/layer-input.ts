// The invocation, resolved into the shape MainLayer is built from.
//
// Everything the layer needs is known before the tree parses: the target
// server, the register, the credential, the streams. `mainLayerFor` is called
// once with this, and every command behind it reads services rather than
// arguments.
//
// The three `undefined` returns below are not missing values — they are the
// layer's own defaults being left alone. Passing `process.stdout` to a field
// that defaults to `process.stdout` would be a no-op that reads like an
// override, and the field's absence is what tells the layer to use its
// default.

import { Option, Redacted } from "effect";
import type { MainLayerInput } from "../../runtime/main-layer.ts";
import type { FetchImpl } from "../../lib/http.ts";
import type { WritableStreamLike } from "../../output/capability.ts";

/** One invocation's resolved environment, before any of it becomes a layer. */
export interface ResolvedInvocation {
  readonly apiUrl: string;
  readonly dashboardUrl: string;
  readonly apiKey: string | null;
  readonly jsonMode: boolean;
  readonly noOpen: boolean;
  readonly commandPath: ReadonlyArray<string>;
  readonly env: NodeJS.ProcessEnv;
  readonly stdout: WritableStreamLike;
  readonly stderr: WritableStreamLike;
  readonly stdin: NodeJS.ReadableStream | undefined;
  readonly fetchImpl: FetchImpl | undefined;
}

// The env-credential slot. The stored login token reaches commands through the
// Credentials service off disk instead; `resolveToken` gives this one
// precedence at the wire, so an exported key beats a stale login.
const accessTokenOf = (apiKey: string | null): Option.Option<Redacted.Redacted<string>> =>
  apiKey !== null && apiKey.length > 0 ? Option.some(Redacted.make(apiKey)) : Option.none();

// Injected test streams only. When the invocation is running on the real
// process streams the layer's own stdio binding is already correct, and
// handing it back the same objects would claim an override that isn't one.
const streamsOf = (
  invocation: ResolvedInvocation,
): { stdout: NodeJS.WritableStream; stderr: NodeJS.WritableStream } | undefined => {
  const { stdout, stderr } = invocation;
  if (stdout === process.stdout && stderr === process.stderr) return undefined;
  return {
    // CommandCtx and the layer both declare the richer NodeJS.WritableStream
    // because that is the production runtime; tests inject partial mocks
    // carrying just `.write` and `isTTY`. Narrowing the field would ripple
    // through every consumer, so the cast stays here, at the one seam that
    // knows both sides.
    stdout: stdout as unknown as NodeJS.WritableStream,
    stderr: stderr as unknown as NodeJS.WritableStream,
  };
};

const stdinOf = (
  stdin: NodeJS.ReadableStream | undefined,
): NodeJS.ReadableStream | undefined =>
  stdin === undefined || stdin === process.stdin ? undefined : stdin;

export const layerInputFor = (invocation: ResolvedInvocation): MainLayerInput => {
  const streams = streamsOf(invocation);
  const stdin = stdinOf(invocation.stdin);
  return {
    config: {
      apiUrl: invocation.apiUrl,
      dashboardUrl: invocation.dashboardUrl,
      accessToken: accessTokenOf(invocation.apiKey),
      jsonMode: invocation.jsonMode,
      noOpen: invocation.noOpen,
      ...(invocation.fetchImpl !== undefined ? { fetchImpl: invocation.fetchImpl } : {}),
    },
    commandPath: invocation.commandPath.length > 0 ? invocation.commandPath : ["unknown"],
    env: invocation.env,
    ...(streams !== undefined ? { streams } : {}),
    ...(stdin !== undefined ? { stdin } : {}),
  };
};
