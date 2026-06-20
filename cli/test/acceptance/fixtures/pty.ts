/**
 * Pseudo-terminal CLI runner for the login-handshake acceptance scenario.
 *
 * `runFleetctl` (cli.js) pipes stdin, so the spawned binary sees a non-TTY
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
// ANSI control sequences (CSI). readline renders the steer-REPL prompt as
// `\x1b[1G\x1b[0J> \x1b[3G`, so a `^`-anchored line matcher would never see
// the leading `>`. Strip them so line matchers + the transcript are
// escape-insensitive (NO_COLOR suppresses colour, not cursor moves).
const ANSI_CONTROL_SEQ = /\x1b\[[0-9;?]*[ -/]*[@-~]/g;
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
  readonly fail: (err: Error) => void;
}

export class PtyProcess {
  readonly #proc: Bun.Subprocess<"pipe", "pipe", "pipe">;
  #output = "";
  #stderr = "";
  #waiters: LineWaiter[] = [];
  #exited = false;
  #exitCode: number | null = null;

  private constructor(proc: Bun.Subprocess<"pipe", "pipe", "pipe">) {
    this.#proc = proc;
    // Both pumps complete when the child closes its pty fds (i.e. it exited),
    // by which point every chunk has been pumped + matched. Any waiter still
    // pending then will never see its line, so reject it immediately instead
    // of hanging until the timeout — a non-zero exit before the awaited line
    // becomes a fast, informative failure.
    const pumps = [
      this.#pump(proc.stdout, (text) => { this.#output += text; }),
      this.#pump(proc.stderr, (text) => { this.#stderr += text; }),
    ];
    void Promise.all(pumps).then(async () => { this.#onExit(await proc.exited); });
  }

  /** Spawn the agentsfleet binary inside a pty, mirroring runFleetctl's resolution. */
  static spawnFleetctl(args: ReadonlyArray<string>, opts: PtySpawnOptions): PtyProcess {
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
      const matches = (): boolean => this.#clean().split("\n").some(predicate);
      if (matches()) { resolve(this.#output); return; }
      if (this.#exited) { reject(this.#exitError()); return; }
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
        fail: (err) => { clearTimeout(timer); reject(err); },
      };
      this.#waiters.push(waiter);
    });
  }

  /** Write a line to the pty (cooked mode delivers it to the child on newline). */
  writeLine(text: string): void {
    this.#proc.stdin.write(`${text}\n`);
    this.#proc.stdin.flush();
  }

  /** Send Ctrl-C (ETX) — the pty line discipline raises SIGINT in the child. */
  interrupt(): void {
    this.#proc.stdin.write("\x03");
    this.#proc.stdin.flush();
  }

  get exited(): Promise<number> {
    return this.#proc.exited;
  }

  /** Child output with carriage returns + ANSI control sequences stripped. */
  get output(): string {
    return this.#clean();
  }

  #clean(): string {
    return this.#output.replace(CARRIAGE_RETURN, "").replace(ANSI_CONTROL_SEQ, "");
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

  // Streams drained → the child exited. Reject every still-pending waiter so
  // an awaited line that will never arrive fails fast instead of timing out.
  #onExit(code: number): void {
    this.#exited = true;
    this.#exitCode = code;
    const pending = this.#waiters;
    this.#waiters = [];
    for (const waiter of pending) waiter.fail(this.#exitError());
  }

  #exitError(): Error {
    return new Error(
      `pty: process exited (code ${this.#exitCode}) before a matching line arrived. ` +
      `saw: ${this.#output.slice(0, OUTPUT_PREVIEW_CHARS)} | ` +
      `stderr: ${this.#stderr.slice(0, STDERR_PREVIEW_CHARS)}`,
    );
  }
}
