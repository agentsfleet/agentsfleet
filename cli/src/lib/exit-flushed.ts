// `process.exit` discards whatever a piped stream has not handed to the
// kernel yet, and under Bun a piped write larger than the pipe's buffer is
// still in flight when the next statement runs: `library --json` over a large
// gallery reached its reader cut off at 128 KiB. Ending a stream calls back
// once everything written to it has flushed, so the exit waits for that.

/** The part of a writable stream this module needs. */
export interface EndableStream {
  end(callback: () => void): unknown;
}

/** Resolves once every stream has flushed what was written to it. */
export const flushed = async (streams: readonly EndableStream[]): Promise<void> => {
  await Promise.all(
    streams.map(
      (stream) =>
        new Promise<void>((resolve) => {
          stream.end(resolve);
        }),
    ),
  );
};

/** Exits with `code` once `streams` hold nothing unwritten. */
export const exitFlushed = async (
  code: number,
  streams: readonly EndableStream[],
  exit: (code: number) => never = process.exit,
): Promise<never> => {
  await flushed(streams);
  return exit(code);
};
