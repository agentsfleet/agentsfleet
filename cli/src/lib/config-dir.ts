import os from "node:os";
import path from "node:path";

import { STATE_DIR_ENV } from "../constants/env.ts";

// The one resolution site for the CLI config directory: the home default and
// the env override applied together. `lib/state.ts` resolves through here with
// its caller's environment; `services/telemetry/consent.ts` resolves through
// here with an explicit process-environment argument, until the telemetry
// Effect graph carries an environment of its own. Before this file, each
// carried its own copy of the same expression. The env-var name itself lives
// in `constants/env.ts`, which is the one declaration site for all of them.

export function resolveConfigDir(env: NodeJS.ProcessEnv): string {
  return env[STATE_DIR_ENV] || path.join(os.homedir(), ".config", "agentsfleet");
}
