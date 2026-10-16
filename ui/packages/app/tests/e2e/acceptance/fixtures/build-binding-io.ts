import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { open, writeFile, mkdir } from "node:fs/promises";

const exec = promisify(execFile);
const READ_TIMEOUT_MS = 15_000;
const INSPECTION_TIMEOUT_MS = 120_000;
export const RECORD_MAX_BYTES = 64 * 1024;
export const WORKTREE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../../../..");

export async function readProcessRecord(): Promise<unknown> {
  const path = process.env.ACCEPTANCE_RUNNER_PROCESS_EVIDENCE;
  if (!path) throw new Error("ACCEPTANCE_RUNNER_PROCESS_EVIDENCE must name a fresh trusted host process record");
  const file = await open(path, "r");
  try {
    const bytes = Buffer.alloc(RECORD_MAX_BYTES + 1);
    let length = 0;
    while (length < bytes.length) {
      const {bytesRead} = await file.read(bytes, length, bytes.length - length);
      if (bytesRead === 0) break;
      length += bytesRead;
    }
    if (length > RECORD_MAX_BYTES) throw new Error("Runner process record exceeds its bound");
    return JSON.parse(bytes.subarray(0, length).toString("utf8"));
  } finally { await file.close(); }
}

export async function writeBuildEvidence(markdown: string): Promise<void> {
  await mkdir(resolve(WORKTREE_ROOT, ".tmp"), {recursive: true});
  await writeFile(resolve(WORKTREE_ROOT, ".tmp/acceptance-preflight.md"), markdown);
}

export async function inspectBuilds(): Promise<unknown> {
  const { stdout } = await exec("bun", [resolve(WORKTREE_ROOT, "scripts/inspect-acceptance-build.ts")], {
    cwd: WORKTREE_ROOT, timeout: INSPECTION_TIMEOUT_MS, maxBuffer: RECORD_MAX_BYTES,
  });
  return JSON.parse(stdout);
}

export async function readBuildResponse(url: string, token?: string): Promise<{ headers: Headers; body: unknown }> {
  const response = await fetch(url, {
    signal: AbortSignal.timeout(READ_TIMEOUT_MS), cache: "no-store", redirect: "error",
    headers: token ? { Authorization: `Bearer ${token}` } : undefined,
  });
  if (!response.ok) throw new Error(`Build observation failed with status ${response.status}`);
  const reader = response.body?.getReader();
  if (!reader) throw new Error("Build observation has no body");
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      length += next.value.byteLength;
      if (length > RECORD_MAX_BYTES) throw new Error("Build observation exceeds its bound");
      chunks.push(next.value);
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
  const body: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  return { headers: response.headers, body };
}

export async function readAppBuild(url: string): Promise<string | null> {
  const response = await fetch(url, {
    method: "HEAD", signal: AbortSignal.timeout(READ_TIMEOUT_MS), cache: "no-store", redirect: "manual",
  });
  if (response.status >= 500) throw new Error("App build observation failed");
  return response.headers.get("x-agentsfleet-build");
}
