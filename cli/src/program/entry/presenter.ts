// How the library's own output reaches the terminal.
//
// Help and version pass through to the library's presenter unchanged. An
// invalid invocation writes nothing here: `renderLibraryFailure` in cli.ts is
// its only writer, so a usage error prints one aligned block (or, under
// `--json`, one envelope) instead of a help document with the error beneath.

import { Effect } from "effect";
import { CliOutput } from "effect/cli";

const INVALID_INVOCATION = "InvalidInvocation" as const;

export const housePresenter: CliOutput.Presenter = {
  present: (event) =>
    event._tag === INVALID_INVOCATION ? Effect.void : CliOutput.defaultPresenter.present(event),
};
