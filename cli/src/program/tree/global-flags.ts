// The two genuinely global flags.
//
// `--api` names the server every command talks to, and `--json` picks the
// register every command answers in. They are declared on the root so `--help`
// lists them and an unknown-flag error does not fire on them. Their VALUES are
// read off argv by the entry point before the tree runs, because they
// configure the layer the tree is provided with — a flag cannot configure the
// thing that is about to execute it.
//
// The other flags that look global are not: `--no-open` and `--no-input` are
// login's and `--tty` is steer's, so each is declared on the command that
// reads it. Declaring a flag on both a parent and a child is refused outright:
// the parent would always claim it, and the child's copy would never fire.

import { Flag, GlobalFlag } from "effect/cli";

export const globalFlags = {
  api: Flag.String("api").pipe(
    Flag.withDescription("API base URL"),
    Flag.withMetavar("<url>"),
    Flag.optional,
  ),
  json: Flag.Boolean("json").pipe(
    Flag.withDescription("Machine-readable JSON output"),
    Flag.withDefault(false),
  ),
} as const;

// The library's `--completions`, with a description that says what the script
// is: a snapshot of what this credential could see when it was generated.
// Sourcing `$(agentsfleet --completions zsh)` from an rc file would make an
// identity read at every shell start, so the script belongs in a file.
export const completionsFlag = GlobalFlag.Action({
  flag: GlobalFlag.Completions.flag.pipe(
    Flag.withDescription(
      "Print a static completion script; write it to a file and regenerate after login, a target change or a permission change",
    ),
  ),
  run: GlobalFlag.Completions.run,
});
