// The library's help document, held back until the run says whether it belongs.
//
// `effect/unstable/cli` does not fail with a parse error on its own: it wraps
// one in `ShowHelp` so the help document renders beneath it. That is the right
// shape for a person and the wrong shape for `--json`, where a consumer reading
// stdout gets a screen of help text followed by the error envelope and
// `JSON.parse` throws on the first character.
//
// Suppressing help whenever `--json` is set would be wrong the other way:
// `agentsfleet --help --json` is a contracted invocation that exits 0 and
// prints the help body (`flags-and-env.spec.ts`, `json-contract.test.ts`).
// The two cases are not distinguishable at the moment the library writes — the
// document is identical, and only the run's outcome says which one happened.
//
// So in JSON mode the library's Console writes land here instead of stdout, and
// the entry point resolves them once it knows: `discard()` where a rejection was
// rendered as an envelope, `flush()` everywhere else. A person's help still
// prints; a machine's rejection stays parseable.

import type { WritableStreamLike } from "../../output/capability.ts";

export interface DeferredHelp {
  /** The sink handed to the library in place of stdout. */
  readonly stream: WritableStreamLike;
  /** Write everything held back to the real stream. */
  readonly flush: () => void;
  /** Drop what was held back — a rejection already spoke for this run. */
  readonly discard: () => void;
}

/**
 * A sink that holds the library's writes until the outcome is known.
 *
 * `isTTY` is copied from the destination rather than left undefined: the help
 * formatter reads it to decide on colour, and a buffered run must not render
 * differently from an unbuffered one.
 */
export const deferredHelp = (destination: WritableStreamLike): DeferredHelp => {
  const held: string[] = [];
  let resolved = false;

  return {
    stream: {
      // Spread rather than assign: `exactOptionalPropertyTypes` treats an
      // explicit `isTTY: undefined` as a different thing from an absent one,
      // and the help formatter reads the property to decide on colour.
      ...(destination.isTTY === undefined ? {} : { isTTY: destination.isTTY }),
      write(chunk: string): boolean {
        if (resolved) return destination.write(chunk) as boolean;
        held.push(chunk);
        return true;
      },
    },
    flush(): void {
      if (resolved) return;
      resolved = true;
      for (const chunk of held) destination.write(chunk);
      held.length = 0;
    },
    discard(): void {
      resolved = true;
      held.length = 0;
    },
  };
};
