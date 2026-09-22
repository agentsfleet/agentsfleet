// The external commands this suite shells out to, and whether the box has them.
//
// A test that spawns a missing binary fails with `Executable not found in
// $PATH`, once per case, with nothing naming the box as the cause. Nine of
// those scrolled past in a clipped Continuous Integration (CI) log and read as
// nine broken tests. A probe that answers the question up front turns that
// into one sentence.

const PROBE_SHELL = "sh" as const;
const PROBE_FLAG = "-c" as const;

/** Commands a case cannot be written without: absence is a broken box. */
export const REQUIRED_COMMANDS = ["bash", "python3"] as const;

/**
 * Commands only some boxes carry, where the case is genuinely unrunnable
 * without them rather than failing.
 *
 * `zsh` is the standard shell on macOS and is absent from GitHub's
 * `ubuntu-latest` image, so a case that parses a zsh script runs on a
 * developer's machine and cannot run in CI.
 */
export const OPTIONAL_COMMANDS = ["zsh"] as const;

/** Whether `name` resolves on this box. */
export function commandPresent(name: string): boolean {
  const probe = Bun.spawnSync([PROBE_SHELL, PROBE_FLAG, `command -v ${name}`], {
    stdout: "ignore",
    stderr: "ignore",
  });
  return probe.exitCode === 0;
}

/** The named commands that this box does NOT have. */
export function missingFrom(names: ReadonlyArray<string>): string[] {
  return names.filter((name) => !commandPresent(name));
}
