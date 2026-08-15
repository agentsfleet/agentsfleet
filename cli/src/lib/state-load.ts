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
  // True when a read FAILED, as distinct from a file that was simply absent.
  // Absence is the logged-out baseline; failure means the record — including
  // the deployment the credential was bound to — is unknown rather than empty.
  readonly readFailed: boolean;
}

export async function loadStateOrWarn(
  env: NodeJS.ProcessEnv,
  warn: (line: string) => void,
): Promise<LoadedState> {
  let failed = false;
  const orWarn = <T>(read: Promise<T>, file: string, fallback: () => T): Promise<T> =>
    read.catch((cause: unknown) => {
      // The errno, not err.message: the message embeds the absolute path, which
      // puts the operator's home directory and username into stderr and from
      // there into any CI job log. The code is the actionable half.
      const code =
        cause !== null && typeof cause === "object" && "code" in cause
          ? String((cause as { code: unknown }).code)
          : String(cause);
      failed = true;
      warn(`warning: could not read ${file} (${code}); continuing as logged out`);
      return fallback();
    });
  // Both reads stay in flight together; the catch arms only run on failure.
  const [creds, workspaces] = await Promise.all([
    orWarn(loadCredentials(env), STATE_FILE_CREDENTIALS, emptyCredentials),
    orWarn(loadWorkspaces(env), STATE_FILE_WORKSPACES, emptyWorkspaces),
  ]);
  return { creds, workspaces, readFailed: failed };
}
