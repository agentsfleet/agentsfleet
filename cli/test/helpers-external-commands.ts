// The external commands this suite shells out to, and whether the box has them.
//
// A test that spawns a missing binary fails with `Executable not found in
// $PATH`, once per case, with nothing naming the box as the cause. Nine of
// those scrolled past in a clipped Continuous Integration (CI) log and read as
// nine broken tests. The commands every case may assume are checked once by
// the lane's runner (`scripts/enforce-coverage.mjs`) before anything spawns;
// this probe answers for the ones only some boxes carry, which a case skips
// without rather than failing.

const PROBE_SHELL = "sh" as const;
const PROBE_FLAG = "-c" as const;

/** Whether `name` resolves on this box. */
export function commandPresent(name: string): boolean {
  const probe = Bun.spawnSync([PROBE_SHELL, PROBE_FLAG, `command -v ${name}`], {
    stdout: "ignore",
    stderr: "ignore",
  });
  return probe.exitCode === 0;
}
