// Native setup preconditions use the same scoped identity as the terminal process.

import { resolve } from "node:path";
import { Schema } from "effect";
import { API_URL_ENV, STATE_DIR_ENV } from "../../../src/constants/env.ts";

export interface AuthContext {readonly apiUrl: string; readonly token: string; readonly workspaceId: string}
const credentials = Schema.Struct({token: Schema.String});
const workspaces = Schema.Struct({current_workspace_id: Schema.String});
const STATE_BYTES_MAX = 64 * 1024;
const RESPONSE_BYTES_MAX = 4 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 60_000;

export async function readAuthContext(env: Readonly<Record<string, string>>): Promise<AuthContext> {
  const apiUrl = env[API_URL_ENV]; const stateDir = env[STATE_DIR_ENV];
  if (!apiUrl || !stateDir) throw new Error("Native fixture requires an API origin and isolated terminal state directory");
  const credentialFile = Bun.file(resolve(stateDir, "credentials.json"));
  const workspaceFile = Bun.file(resolve(stateDir, "workspaces.json"));
  if (credentialFile.size > STATE_BYTES_MAX || workspaceFile.size > STATE_BYTES_MAX) throw new Error("Fixture state exceeds its bound");
  const credential = Schema.decodeUnknownSync(credentials)(await credentialFile.json());
  const workspace = Schema.decodeUnknownSync(workspaces)(await workspaceFile.json());
  return {apiUrl, token: credential.token, workspaceId: workspace.current_workspace_id};
}

export async function nativeRequest(ctx: AuthContext, suffix: string, method = "GET", body?: unknown,
  headers?: Readonly<Record<string, string>>): Promise<unknown> {
  const response = await fetch(`${ctx.apiUrl}/v1/workspaces/${encodeURIComponent(ctx.workspaceId)}${suffix}`, {
    method, body: body === undefined ? undefined : JSON.stringify(body),
    headers: {Authorization: `Bearer ${ctx.token}`, "Content-Type": "application/json", ...headers},
    cache: "no-store", redirect: "error", signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  });
  if (!response.ok) { await response.body?.cancel(); throw new Error(`Native fixture request refused with status ${response.status}`); }
  if (response.status === 204) return null;
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Native fixture response has no body");
  const chunks: Uint8Array[] = []; let length = 0;
  try {
    for (;;) {
      const chunk = await reader.read(); if (chunk.done) break;
      length += chunk.value.byteLength;
      if (length > RESPONSE_BYTES_MAX) throw new Error("Native fixture response exceeds its bound");
      chunks.push(chunk.value);
    }
    return JSON.parse(Buffer.concat(chunks).toString("utf8"));
  } finally { try { await reader.cancel(); } finally { reader.releaseLock(); } }
}
