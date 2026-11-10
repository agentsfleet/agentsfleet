// Piped JSON and credentials use bounded reads with caller-owned cancellation.
// Injecting the stream keeps tests away from the test runner's own stdin.

import { Context, Effect, Layer } from "effect";
import { UnexpectedError, ValidationError } from "../errors/index.ts";

export interface StdinShape {
  readonly isTTY: boolean;
  readonly readBounded: (maximumBytes: number) => Effect.Effect<string, ValidationError | UnexpectedError>;
}

export type Stdin = StdinShape;
const STREAM_DATA_EVENT = "data" as const;
const STREAM_END_EVENT = "end" as const;
const STATUS_ERROR = "error" as const;

export const Stdin = Context.Service<Stdin>("agentsfleet/runtime/Stdin");

export const makeLive = (stream: NodeJS.ReadableStream = process.stdin): StdinShape => ({
  isTTY: Boolean((stream as { isTTY?: boolean }).isTTY),
  readBounded: (maximumBytes) => Effect.tryPromise({
    try: signal => readStreamToEnd(stream, maximumBytes, signal),
    catch: cause => cause instanceof ValidationError ? cause
      : new UnexpectedError({detail: "Could not read standard input", suggestion: "check the input pipe and try again"}),
  }),
});
export const stdinLayer: Layer.Layer<Stdin> = Layer.succeed(Stdin, Stdin.of(makeLive()));

export const stdinFromStreamLayer = (stream: NodeJS.ReadableStream): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, Stdin.of(makeLive(stream)));

const readStreamToEnd = (stream: NodeJS.ReadableStream, maximumBytes: number, signal: AbortSignal): Promise<string> =>
  new Promise<string>((resolve, reject) => {
    const chunks: Buffer[] = [];
    let bytes = 0;
    const onData = (chunk: Buffer | string): void => {
      const buffer = typeof chunk === "string" ? Buffer.from(chunk) : chunk;
      bytes += buffer.byteLength;
      if (bytes > maximumBytes) {
        cleanup(); stream.pause();
        reject(new ValidationError({detail: `Standard input exceeds ${maximumBytes} bytes`, suggestion: "pipe a smaller input"}));
        return;
      }
      chunks.push(buffer);
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
