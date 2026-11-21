// Piped JSON and credentials are read to the end with caller-owned
// cancellation. No size cap: the server bounds what it accepts and names the
// bound when it refuses. Injecting the stream keeps tests away from the test
// runner's own stdin.

import { Context, Effect, Layer } from "effect";
import { UnexpectedError } from "../errors/index.ts";

export interface StdinShape {
  readonly isTTY: boolean;
  readonly readAll: () => Effect.Effect<string, UnexpectedError>;
}

export type Stdin = StdinShape;
const STREAM_DATA_EVENT = "data" as const;
const STREAM_END_EVENT = "end" as const;
const STATUS_ERROR = "error" as const;

export const Stdin = Context.Service<Stdin>("agentsfleet/runtime/Stdin");

export const makeLive = (stream: NodeJS.ReadableStream = process.stdin): StdinShape => ({
  isTTY: Boolean((stream as { isTTY?: boolean }).isTTY),
  readAll: () => Effect.tryPromise({
    try: signal => readStreamToEnd(stream, signal),
    catch: () => new UnexpectedError({detail: "Could not read standard input", suggestion: "check the input pipe and try again"}),
  }),
});
export const stdinLayer: Layer.Layer<Stdin> = Layer.succeed(Stdin, Stdin.of(makeLive()));

export const stdinFromStreamLayer = (stream: NodeJS.ReadableStream): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, Stdin.of(makeLive(stream)));

const readStreamToEnd = (stream: NodeJS.ReadableStream, signal: AbortSignal): Promise<string> =>
  new Promise<string>((resolve, reject) => {
    const chunks: Buffer[] = [];
    const onData = (chunk: Buffer | string): void => {
      chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
    };
    // Remove every listener on settle. `data` is registered with `on`, and
    // the `once` end/error pair only self-removes the handler that fires —
    // the others linger and trip MaxListenersExceededWarning if the stream
    // outlives one read (long-lived streams, test harnesses).
    const cleanup = (): void => {
      stream.removeListener(STREAM_DATA_EVENT, onData);
      stream.removeListener(STREAM_END_EVENT, onEnd);
      stream.removeListener(STATUS_ERROR, onError);
      signal.removeEventListener("abort", onAbort);
    };
    const onEnd = (): void => {
      cleanup();
      resolve(Buffer.concat(chunks).toString("utf8"));
    };
    const onError = (err: Error): void => {
      cleanup();
      reject(err);
    };
    const onAbort = (): void => {cleanup(); stream.pause(); reject(signal.reason);};
    signal.addEventListener("abort", onAbort, {once: true});
    stream.on(STREAM_DATA_EVENT, onData);
    stream.once(STREAM_END_EVENT, onEnd);
    stream.once(STATUS_ERROR, onError);
  });
