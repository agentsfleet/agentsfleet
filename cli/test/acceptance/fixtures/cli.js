/**
 * Per-spawn CLI runner for the acceptance suite.
 *
 * Two binary modes:
 *   - worktree: spawn `node ./dist/bin/agentsfleet.js` from the repo (DEV jobs)
 *   - global:   spawn `agentsfleet` from PATH (PROD jobs, post-`npm i -g`)
 *
 * Both modes run the tsc-emitted .js artifact. Worktree mode requires
 * a prior `npm run build` (the test script chains build before bun test);
 * global mode resolves `agentsfleet` from PATH after `npm i -g`.
 *
 * The caller composes the full child env — `runAgentctl` NEVER mutates
 * `process.env`. The minted Clerk JWT is per-call; leaking it into the
 * parent env would inherit into every later spawn.
 */

import { spawn } from "node:child_process";
import path from "node:path";
import url from "node:url";

import { ACCEPTANCE_BINARY, ACCEPTANCE_BINARY_ENV } from "./constants.ts";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const CLI_ROOT = path.resolve(HERE, "..", "..", "..");
const WORKTREE_ENTRY = path.join(CLI_ROOT, "dist", "bin", "agentsfleet.js");

const DEFAULT_TIMEOUT_MS = 60_000;

export class TimeoutError extends Error {
  constructor(message, opts) {
    super(message);
    this.name = "TimeoutError";
    this.args = opts?.args ?? [];
    this.timeoutMs = opts?.timeoutMs;
  }
}

export function resolveBinary(opts) {
  const requested = opts?.binary ?? process.env[ACCEPTANCE_BINARY_ENV] ?? ACCEPTANCE_BINARY.worktree;
  if (requested === ACCEPTANCE_BINARY.global) return { command: "agentsfleet", prefixArgs: [] };
  if (requested === ACCEPTANCE_BINARY.worktree) {
    return { command: "node", prefixArgs: [WORKTREE_ENTRY] };
  }
  throw new Error(`unknown AGENTSFLEET_ACCEPTANCE_BINARY: ${requested}`);
}

function assertEnvComposed(env) {
  if (env === undefined || env === null) {
    throw new Error("runAgentctl requires explicit env (per-call composition is the contract)");
  }
  if (typeof env !== "object") {
    throw new Error("runAgentctl env must be an object");
  }
}

/**
 * Spawn the CLI with a fully-composed child env.
 *
 * @param {string[]} args - argv passed to agentsfleet (e.g. ["workspace","list","--json"])
 * @param {object} opts
 * @param {Record<string,string>} opts.env - COMPLETE child env (no merge with process.env)
 * @param {string} [opts.stdin] - data to pipe to stdin; if absent, stdin is closed
 * @param {number} [opts.timeoutMs] - default 60_000
 * @param {string} [opts.cwd] - default: agentsfleet/
 * @param {"worktree"|"global"} [opts.binary] - default from env
 * @returns {Promise<{code:number, stdout:string, stderr:string, durationMs:number}>}
 */
export function runAgentctl(args, opts) {
  assertEnvComposed(opts?.env);
  const { command, prefixArgs } = resolveBinary(opts);
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const cwd = opts.cwd ?? CLI_ROOT;
  const started = Date.now();

  return new Promise((resolve, reject) => {
    const child = spawn(command, [...prefixArgs, ...args], {
      cwd,
      env: opts.env,
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let timedOut = false;

    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, timeoutMs);

    child.stdout.on("data", (chunk) => { stdout += String(chunk); });
    child.stderr.on("data", (chunk) => { stderr += String(chunk); });

    child.on("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });

    child.on("close", (code, signal) => {
      clearTimeout(timer);
      const durationMs = Date.now() - started;
      if (timedOut) {
        reject(new TimeoutError(`agentsfleet ${args.join(" ")} timed out after ${timeoutMs}ms`, { args, timeoutMs }));
        return;
      }
      resolve({ code: code ?? (signal ? 128 : 0), stdout, stderr, durationMs, signal });
    });

    if (opts.stdin !== undefined) {
      child.stdin.write(opts.stdin);
    }
    child.stdin.end();
  });
}

/**
 * Spawn the CLI and return the child handle for streaming-output cases
 * (`agentsfleet login` poll loop, SIGINT scenarios). Caller is responsible
 * for awaiting exit and cleaning up.
 */
export function spawnAgentctl(args, opts) {
  assertEnvComposed(opts?.env);
  const { command, prefixArgs } = resolveBinary(opts);
  const cwd = opts.cwd ?? CLI_ROOT;
  return spawn(command, [...prefixArgs, ...args], {
    cwd,
    env: opts.env,
    stdio: ["pipe", "pipe", "pipe"],
  });
}

/**
 * Compose a child env from a known-allowlist of fixture-supplied vars.
 * Never reads `process.env` for fields the caller did not list — keeps
 * the parent's `AGENTSFLEET_API_KEY` (if any) out of every spawn unless the
 * caller asks for it explicitly.
 */
export function composeEnv(fields) {
  const env = {
    PATH: process.env.PATH,
    HOME: process.env.HOME,
    // Acceptance specs spawn the real CLI, whose telemetry flush reaches
    // PostHog over the network and hangs ~5s when the runner is offline.
    // Default every spawn to telemetry-off so the suite is hermetic under
    // any runner; a caller may still override by listing the key in fields.
    AGENTSFLEET_TELEMETRY_DISABLED: "1",
  };
  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined || value === null) continue;
    env[key] = String(value);
  }
  return env;
}
