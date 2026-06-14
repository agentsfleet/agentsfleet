/**
 * Stubbed `ZOMBIE_STATE_DIR` for tests that need the CLI's auth-guard
 * and workspace-context layers to pass WITHOUT hitting a real API.
 *
 * The token is a syntactically-valid 3-segment string; the workspace is
 * a deterministic stub id. Per-call: each invocation gets its own tmpdir,
 * cleaned up by the returned `cleanup` callback.
 *
 * Use cases:
 *   - unknown-subcommand sweep: needs to reach the per-group dispatcher,
 *     which is guarded by auth + workspace-context resolution. The
 *     dispatcher's "unknown action" branch fires BEFORE any fetch, so
 *     `ZOMBIE_API_URL=http://127.0.0.1:1` plus this stub yields the
 *     expected stem without touching the network.
 */

import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

const STUB_TOKEN = "header.payload.sig";

export interface StubbedStateDirOptions {
  readonly workspaceId?: string;
  readonly workspaceName?: string;
  readonly token?: string;
  readonly sessionId?: string;
  readonly apiUrl?: string | null;
}

export interface StubbedStateDir {
  readonly dir: string;
  readonly workspaceId: string;
  cleanup(): Promise<void>;
}

export async function makeStubbedStateDir(opts?: StubbedStateDirOptions): Promise<StubbedStateDir> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-accept-"));
  const credsPath = path.join(dir, "credentials.json");
  const workspacesPath = path.join(dir, "workspaces.json");
  const workspaceId = opts?.workspaceId ?? "ws_acceptance_stub";
  const workspaceName = opts?.workspaceName ?? "acceptance-stub";

  await fs.writeFile(
    credsPath,
    JSON.stringify({
      token: opts?.token ?? STUB_TOKEN,
      saved_at: Date.now(),
      session_id: opts?.sessionId ?? "sess_acceptance_stub",
      api_url: opts?.apiUrl ?? null,
    }),
    { mode: 0o600 },
  );

  await fs.writeFile(
    workspacesPath,
    JSON.stringify({
      current_workspace_id: workspaceId,
      items: [{ workspace_id: workspaceId, name: workspaceName, created_at: Date.now() }],
    }),
  );

  return {
    dir,
    workspaceId,
    cleanup: () => fs.rm(dir, { recursive: true, force: true }),
  };
}
