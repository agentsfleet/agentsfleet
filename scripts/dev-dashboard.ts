import { resolve } from "node:path";
import { z } from "zod";
import { WORKTREE_ROOT, sourceRevision } from "./build-source.ts";

export const DEV_DASHBOARD_TARGET = "development-agentsfleet";
const PROJECT_PATH = ".vercel/project.json";
const VERCEL_PROJECT_API = "https://api.vercel.com/v9/projects/agentsfleet-app";
const DASHBOARD_DOMAIN = "app-dev.agentsfleet.net";
const PROJECT_ROOT = "ui/packages/app";
const MAX_RESPONSE_BYTES = 1024 * 1024;
const COMMAND_TIMEOUT_MS = 30 * 60_000;
const READ_TIMEOUT_MS = 15_000;
const STDIO_INHERIT = "inherit";
const CONFIRM_FLAG = "--yes";
const METADATA_FLAG = "--meta";
const DEPLOY_COMMAND = "deploy";
const projectSchema = z.object({id: z.string().min(1), accountId: z.string().min(1), rootDirectory: z.literal(PROJECT_ROOT)});
const localProjectSchema = z.object({settings: z.record(z.string(), z.unknown())}).passthrough();
type Options = {root?: string; apiUrl?: string; executable?: string; env?: NodeJS.ProcessEnv; timeoutMs?: number};

export async function prepareDevDashboard(options: Options = {}): Promise<void> {
  const root = options.root ?? WORKTREE_ROOT;
  const env = options.env ?? Bun.env;
  if (!env.VERCEL_TOKEN) throw new Error("VERCEL_TOKEN is required for the development dashboard build");
  const response = await fetch(options.apiUrl ?? VERCEL_PROJECT_API, {
    headers: {Authorization: `Bearer ${env.VERCEL_TOKEN}`}, signal: AbortSignal.timeout(READ_TIMEOUT_MS), redirect: "error",
  });
  if (!response.ok || !response.body) throw new Error(`Vercel project read failed: ${response.status}`);
  const project = projectSchema.parse(JSON.parse(await readBounded(response.body)));
  await Bun.write(resolve(root, PROJECT_PATH), JSON.stringify({projectId: project.id, orgId: project.accountId}));
  await vercel(["pull", CONFIRM_FLAG, `--environment=${DEV_DASHBOARD_TARGET}`], options);
  const local = Bun.file(resolve(root, PROJECT_PATH));
  if (local.size > MAX_RESPONSE_BYTES) throw new Error("Vercel project settings exceed their bound");
  const settings = localProjectSchema.parse(await local.json());
  // Install dependencies before loading credentials; prevent another install during the framework build.
  await Bun.write(local, JSON.stringify({...settings, settings: {...settings.settings, installCommand: "true"}}));
}

export async function deployDevDashboard(options: Options = {}): Promise<string> {
  const revision = sourceRevision(options.root ?? WORKTREE_ROOT);
  const output = await vercel([DEPLOY_COMMAND, "--prebuilt", CONFIRM_FLAG, `--target=${DEV_DASHBOARD_TARGET}`,
    METADATA_FLAG, `githubCommitSha=${revision}`, METADATA_FLAG, "githubCommitRef=main"], options, true);
  const url = new URL(output.trim());
  if (url.protocol !== "https:" || !url.hostname.endsWith(".vercel.app") || url.username || url.password || url.search || url.hash) {
    throw new Error("Vercel returned an unexpected development deployment URL");
  }
  await vercel(["alias", "set", url.href, DASHBOARD_DOMAIN], options);
  return url.href;
}

async function vercel(args: string[], options: Options, capture = false): Promise<string> {
  const root = options.root ?? WORKTREE_ROOT;
  const child = Bun.spawn([options.executable ?? resolve(root, "node_modules/.bin/vercel"), ...args], {
    cwd: root, env: options.env ?? Bun.env, stdin: "ignore", stdout: capture ? "pipe" : STDIO_INHERIT, stderr: STDIO_INHERIT,
    signal: AbortSignal.timeout(options.timeoutMs ?? COMMAND_TIMEOUT_MS),
  });
  try {
    const output = capture && child.stdout ? await readBounded(child.stdout) : "";
    if (await child.exited !== 0) throw new Error(`Vercel ${args[0]} failed`);
    return output;
  } finally {
    if (child.exitCode === null) child.kill();
    await child.exited;
  }
}

async function readBounded(stream: ReadableStream<Uint8Array>): Promise<string> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let length = 0;
  let text = "";
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) return text + decoder.decode();
      length += next.value.byteLength;
      if (length > MAX_RESPONSE_BYTES) throw new Error("Vercel response exceeds its bound");
      text += decoder.decode(next.value, {stream: true});
    }
  } finally { await reader.cancel(); reader.releaseLock(); }
}

export async function runDevDashboard(mode: string | undefined, options: Options = {}): Promise<void> {
  if (mode === "prepare") await prepareDevDashboard(options);
  else if (mode === DEPLOY_COMMAND) process.stdout.write(`${await deployDevDashboard(options)}\n`);
  else throw new Error("Expected development dashboard prepare or deploy");
}
