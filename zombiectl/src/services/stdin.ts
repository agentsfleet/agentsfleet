// Stdin service — non-interactive input surface for the login
// direct-token path. Fronts the two facts a piped credential read needs:
// whether stdin is a terminal (`isTTY`) and the full piped payload read to
// EOF (`readToEnd`). Kept separate from Input (readline-backed, prompt
// shaped) because a bulk drain and a line prompt are different jobs.
//
// The stream is injectable so runCli can thread its `io.stdin` (production:
// process.stdin; tests: a fake) through to here — the same seam Output uses
// for its streams. Reading a generic Readable rather than the global
// `Bun.stdin` is what lets integration tests pin TTY-ness without consuming
// the test runner's real stdin.

import { Context, Effect, Layer } from "effect";

export interface StdinShape {
  // True when stdin is attached to a terminal. False for pipes, redirects,
  // and CI — the contexts where `readToEnd` carries a token.
  readonly isTTY: boolean;
  // Drains stdin to EOF and returns the raw text. Only evaluated on the
  // non-TTY direct-token branch, so an interactive shell never blocks here.
  readonly readToEnd: Effect.Effect<string>;
}

export class Stdin extends Context.Service<Stdin, StdinShape>()(
  "zombiectl/runtime/Stdin",
) {}

const readStreamToEnd = (stream: NodeJS.ReadableStream): Promise<string> =>
  new Promise<string>((resolve, reject) => {
    const chunks: Buffer[] = [];
    stream.on("data", (chunk: Buffer | string) =>
      chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk),
    );
    stream.once("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    stream.once("error", reject);
  });

export const makeLive = (stream: NodeJS.ReadableStream = process.stdin): StdinShape => ({
  isTTY: Boolean((stream as { isTTY?: boolean }).isTTY),
  readToEnd: Effect.promise(() => readStreamToEnd(stream)),
});

export const stdinLayer: Layer.Layer<Stdin> = Layer.succeed(Stdin, Stdin.of(makeLive()));

export const stdinFromStreamLayer = (stream: NodeJS.ReadableStream): Layer.Layer<Stdin> =>
  Layer.succeed(Stdin, Stdin.of(makeLive(stream)));
