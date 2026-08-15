// The entry point's one state read. A missing or empty state file already
// reads as signed-out inside the store (readJson folds ENOENT and parse
// failures into the fallback), so anything that rejects here is a REAL
// failure — EACCES, EIO, a directory where a file should be. Folding those
// into "signed out" silently is what sends someone to re-authenticate
// against a store that cannot be read, so the failures are RECORDED and
// returned; the caller reports them once it knows which endpoint it settled
// on, and the empty record keeps read-only commands working meanwhile.

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

export interface UnreadableFile {
  readonly file: string;
  readonly code: string;
}

export interface LoadedState {
  readonly creds: Credentials;
  readonly workspaces: Workspaces;
  // Files that exist but could NOT be read (EACCES, EIO, a directory in the
  // way). Absence is not failure — a missing file is simply the signed-out
  // baseline, and the common case for anyone who authenticates by environment
  // variable and never runs `login`. Failure means the record is unknown
  // rather than empty, including the deployment the credential was bound to,
  // so the caller reports it once it knows which endpoint it settled on.
  readonly unreadable: readonly UnreadableFile[];
}

export async function loadState(env: NodeJS.ProcessEnv): Promise<LoadedState> {
  const unreadable: UnreadableFile[] = [];
  const orRecord = <T>(read: Promise<T>, file: string, fallback: () => T): Promise<T> =>
    read.catch((cause: unknown) => {
      // The errno, not err.message: the message embeds the absolute path, which
      // puts the operator's home directory and username into stderr and from
      // there into any CI job log. The code is the actionable half.
      const code =
        cause !== null && typeof cause === "object" && "code" in cause
          ? String((cause as { code: unknown }).code)
          : String(cause);
      unreadable.push({ file, code });
      return fallback();
    });
  // Both reads stay in flight together; the catch arms only run on failure.
  const [creds, workspaces] = await Promise.all([
    orRecord(loadCredentials(env), STATE_FILE_CREDENTIALS, emptyCredentials),
    orRecord(loadWorkspaces(env), STATE_FILE_WORKSPACES, emptyWorkspaces),
  ]);
  return { creds, workspaces, unreadable };
}
