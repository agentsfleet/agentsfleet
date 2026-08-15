// The entry point's one state read. A missing or empty state file already
// reads as logged-out inside the store (readJson folds ENOENT and parse
// failures into the fallback), so anything that rejects here is a REAL
// failure — EACCES, EIO, a directory where a file should be. Folding those
// into "logged-out" sends the user to re-login against a store that cannot
// be read; warn with the cause instead, then continue with the empty record
// so read-only commands still work.

import {
  emptyCredentials,
  emptyWorkspaces,
  loadCredentials,
  loadWorkspaces,
  STATE_FILE_CREDENTIALS,
  STATE_FILE_WORKSPACES,
  type Credentials,
  type Workspaces,
} from "./state.ts";

export interface LoadedState {
  readonly creds: Credentials;
  readonly workspaces: Workspaces;
}

export async function loadStateOrWarn(
  env: NodeJS.ProcessEnv,
  warn: (line: string) => void,
): Promise<LoadedState> {
  const warnFor = (file: string) => (cause: unknown) => {
    const detail = cause instanceof Error ? cause.message : String(cause);
    warn(`warning: could not read ${file} (${detail}); continuing as logged out`);
  };
  const [creds, workspaces] = await Promise.all([
    loadCredentials(env).catch((cause: unknown) => {
      warnFor(STATE_FILE_CREDENTIALS)(cause);
      return emptyCredentials();
    }),
    loadWorkspaces(env).catch((cause: unknown) => {
      warnFor(STATE_FILE_WORKSPACES)(cause);
      return emptyWorkspaces();
    }),
  ]);
  return { creds, workspaces };
}
