/**
 * Pseudo-terminal CLI runner for the login-handshake acceptance scenario.
 *
 * `runAgentctl` (cli.js) pipes stdin, so the spawned binary sees a non-TTY
 * stdin and `agentsfleet login` fast-fails before printing `login_url`.
 * The device flow is terminal-only by design (a human types the 6-digit
 * code), so this harness allocates a real pty via `pty-spawn.py` and drives
 * the prompt programmatically: wait for a line, write the code, await exit.
 *
 * Output note: under a pty the child's stdout AND stderr both land on the
 * pty slave, so everything arrives on one stream (`output`). Python's own
 * errors (rare) arrive separately on `stderr`.
 */

import path from "node:path";
import url from "node:url";

import { resolveBinary } from "./cli.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const PTY_LAUNCHER = path.join(HERE, "pty-spawn.py");
const PYTHON_BIN = "python3";
const DEFAULT_WAIT_MS = 30_000;
const CARRIAGE_RETURN = /\r/g;
const OUTPUT_PREVIEW_CHARS = 600;
const STDERR_PREVIEW_CHARS = 300;

export interface PtySpawnOptions {
  readonly env: Record<string, string>;
  readonly cwd?: string;
  readonly binary?: "worktree" | "global";
}

interface LineWaiter {
  readonly matches: () => boolean;
  readonly settle: () => void;
}

export class PtyProcess {
  readonly #proc: Bun.Subprocess<"pipe", "pipe", "pipe">;
  #output = "";
  #stderr = "";
  #waiters: LineWaiter[] = [];

  private constructor(proc: Bun.Subprocess<"pipe", "pipe", "pipe">) {
    this.#proc = proc;
    void this.#pump(proc.stdout, (text) => { this.#output += text; });
    void this.#pump(proc.stderr, (text) => { this.#stderr += text; });
  }

  /** Spawn the agentsfleet binary inside a pty, mirroring runAgentctl's resolution. */
  static spawnAgentctl(args: ReadonlyArray<string>, opts: PtySpawnOptions): PtyProcess {
    const { command, prefixArgs } = resolveBinary(opts);
    const proc = Bun.spawn([PYTHON_BIN, PTY_LAUNCHER, command, ...prefixArgs, ...args], {
      env: opts.env,
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
      // exactOptionalPropertyTypes: omit cwd entirely rather than pass undefined.
      ...(opts.cwd !== undefined ? { cwd: opts.cwd } : {}),
    });
    return new PtyProcess(proc);
  }

  /** Resolve once any line in the accumulated output satisfies `predicate`. */
  waitForLine(predicate: (line: string) => boolean, timeoutMs = DEFAULT_WAIT_MS): Promise<string> {
    return new Promise((resolve, reject) => {
      const matches = (): boolean => this.#output.split(/\r?\n/).some(predicate);
      if (matches()) { resolve(this.#output); return; }
      const timer = setTimeout(() => {
        this.#waiters = this.#waiters.filter((w) => w !== waiter);
        reject(new Error(
          `pty: timed out after ${timeoutMs}ms waiting for line. ` +
          `saw: ${this.#output.slice(0, OUTPUT_PREVIEW_CHARS)} | ` +
          `stderr: ${this.#stderr.slice(0, STDERR_PREVIEW_CHARS)}`,
        ));
      }, timeoutMs);
      const waiter: LineWaiter = {
        matches,
        settle: () => { clearTimeout(timer); resolve(this.#output); },
      };
      this.#waiters.push(waiter);
    });
  }

  /** Write a line to the pty (cooked mode delivers it to the child on newline). */
  writeLine(text: string): void {
    this.#proc.stdin.write(`${text}\n`);
    this.#proc.stdin.flush();
  }

  get exited(): Promise<number> {
    return this.#proc.exited;
  }

  /** Child output with carriage returns stripped (pty echoes them). */
  get output(): string {
    return this.#output.replace(CARRIAGE_RETURN, "");
  }

  kill(): void {
    this.#proc.kill();
  }

  async #pump(stream: ReadableStream<Uint8Array>, sink: (text: string) => void): Promise<void> {
    const reader = stream.getReader();
    const decoder = new TextDecoder();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        sink(decoder.decode(value, { stream: true }));
        this.#checkWaiters();
      }
    } finally {
      reader.releaseLock();
    }
  }

  #checkWaiters(): void {
    this.#waiters = this.#waiters.filter((waiter) => {
      if (waiter.matches()) { waiter.settle(); return false; }
      return true;
    });
  }
}
