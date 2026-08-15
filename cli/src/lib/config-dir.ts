import os from "node:os";
import path from "node:path";

// The one declaration site for the CLI config-directory resolution: the
// environment key and the home default. `lib/state.ts` resolves through here
// with its caller's environment; `services/telemetry/consent.ts` resolves
// through here with an explicit process-environment argument, until the
// telemetry Effect graph carries an environment of its own. Before this file,
// each carried its own copy of the same expression.

export const STATE_DIR_ENV = "AGENTSFLEET_STATE_DIR" as const;

export function resolveConfigDir(env: NodeJS.ProcessEnv): string {
  return env[STATE_DIR_ENV] || path.join(os.homedir(), ".config", "agentsfleet");
}
