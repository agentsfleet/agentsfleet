import { Effect } from "effect";
import { ExpiredSessionError, InterruptedError, InvalidSessionError, SessionAbortedError, type CliError } from "../errors/index.ts";
import type { CliKeypair } from "../lib/cli-flow.ts";
import { SIGINT } from "../constants/signals.ts";
import { Browser } from "../services/browser.service.ts";
import { HttpClient } from "../services/http-client.ts";
import { OUTPUT_FORMAT, Output } from "../services/output.ts";
import { verifyAndDecrypt } from "./login-device-flow.ts";
import { LoginCallback } from "./login-callback.ts";
import {
  CALLBACK_DECISION, CALLBACK_STATUS, browserHandoffUrl, loginComparisonCode,
  trustedLoginUrl, type CallbackDecision,
} from "./login-callback-protocol.ts";

const RETRY_LOGIN = "retry `agentsfleet login`";
const INPUT_EVENT = "data";
const CANCEL_KEY = { escape: "\u001b", interrupt: "\u0003" } as const;

interface TerminalInput {
  readonly isTTY?: boolean;
  readonly isRaw?: boolean;
  setRawMode(mode: boolean): unknown;
  isPaused(): boolean;
  on(event: string, listener: (chunk: Buffer | string) => void): unknown;
  removeListener(event: string, listener: (chunk: Buffer | string) => void): unknown;
  pause(): unknown;
}
interface SignalSource {
  on(event: string, listener: () => void): unknown;
  removeListener(event: string, listener: () => void): unknown;
}

export class TerminalLoginControls {
  readonly #input: TerminalInput;
  readonly #signals: SignalSource;
  readonly #rawBefore: boolean;
  readonly #pausedBefore: boolean;
  readonly #cancel: () => void;
  readonly #onData: (chunk: Buffer | string) => void;
  #reading = false;

  constructor(callback: LoginCallback, input: TerminalInput = process.stdin, signals: SignalSource = process) {
    this.#input = input;
    this.#signals = signals;
    this.#rawBefore = Boolean(input.isRaw);
    this.#pausedBefore = input.isPaused();
    this.#cancel = () => { callback.cancel(); };
    this.#onData = (chunk) => {
      const key = typeof chunk === "string" ? chunk : chunk.toString("utf8");
      if (key === CANCEL_KEY.escape || key === CANCEL_KEY.interrupt) this.#cancel();
    };
    signals.on(SIGINT, this.#cancel);
    try {
      if (input.isTTY) {
        this.#reading = true;
        input.setRawMode(true);
        input.on(INPUT_EVENT, this.#onData);
      }
    } catch (error) { this.close(); throw error; }
  }

  stopReading(): void {
    if (!this.#reading) return;
    this.#reading = false;
    this.#input.removeListener(INPUT_EVENT, this.#onData);
    this.#input.setRawMode(this.#rawBefore);
    if (this.#pausedBefore) this.#input.pause();
  }

  close(): void {
    try { this.stopReading(); }
    finally { this.#signals.removeListener(SIGINT, this.#cancel); }
  }
}

interface BrowserLoginOptions {
  readonly sessionId: string;
  readonly keypair: CliKeypair;
  readonly dashboardUrl: string;
  readonly loginUrl: string;
  readonly deadlineMs: number;
}

function decisionFailure(decision: CallbackDecision): CliError {
  if (decision.status === CALLBACK_DECISION.canceled) {
    return new InterruptedError({ detail: "login canceled", suggestion: RETRY_LOGIN });
  }
  if (decision.status === CALLBACK_DECISION.expired) {
    return new ExpiredSessionError({ detail: "login session expired", suggestion: RETRY_LOGIN });
  }
  return new SessionAbortedError({ detail: "login was denied in your browser", suggestion: RETRY_LOGIN });
}

const announceBrowserSession = Effect.fnUntraced(function* (url: string, code: string) {
  const output = yield* Output;
  if (output.format !== OUTPUT_FORMAT.text) return;
  yield* output.printSection("Connect your terminal");
  yield* output.info("Open this link on this computer:");
  yield* output.printKeyValue({ login_url: url });
  yield* output.info(`Check code: ${code}`);
  yield* output.info("Waiting for authorization… (Esc to cancel)");
});

export const browserLogin = <A, E, R>(
  options: BrowserLoginOptions,
  complete: (token: string) => Effect.Effect<A, E, R>,
): Effect.Effect<A, CliError | E, Browser | Output | HttpClient | R> =>
  Effect.gen(function* () {
    const login = yield* Effect.try({
      try: () => trustedLoginUrl(options.dashboardUrl, options.loginUrl, options.sessionId),
      catch: () => new InvalidSessionError({ detail: "the login URL does not match the configured dashboard", suggestion: RETRY_LOGIN }),
    });
    return yield* Effect.acquireUseRelease(
      Effect.try({
        try: () => new LoginCallback({ sessionId: options.sessionId, dashboardOrigin: login.origin, deadlineMs: options.deadlineMs }),
        catch: () => new SessionAbortedError({ detail: "could not start the local authorization listener", suggestion: "retry or use `agentsfleet login --no-open` for manual code entry" }),
      }),
      (callback) => useBrowserCallback(options, login, callback, complete),
      (callback) => Effect.promise(() => callback.close()),
    );
  });

const useBrowserCallback = <A, E, R>(
  options: BrowserLoginOptions, login: URL, callback: LoginCallback,
  complete: (token: string) => Effect.Effect<A, E, R>,
): Effect.Effect<A, CliError | E, Browser | Output | HttpClient | R> =>
  Effect.acquireUseRelease(
    Effect.try({
      try: () => new TerminalLoginControls(callback),
      catch: () => new SessionAbortedError({ detail: "could not read terminal cancellation keys", suggestion: RETRY_LOGIN }),
    }),
    (controls) => Effect.gen(function* () {
      const output = yield* Output;
      const browser = yield* Browser;
      const url = browserHandoffUrl(login, options.sessionId, options.keypair.publicKeyBase64Url, callback.callbackUrl, callback.state);
      const code = yield* Effect.promise(() => loginComparisonCode(options.sessionId, options.keypair.publicKeyBase64Url));
      yield* announceBrowserSession(url, code);
      const opened = yield* browser.open(url);
      if (output.format === OUTPUT_FORMAT.text) {
        yield* output.info(opened ? "browser: opened" : "Browser not opened. Use the link on this computer, or restart with --no-open for another device.");
      }
      const decision = yield* Effect.promise(() => callback.waitForDecision());
      controls.stopReading();
      if (decision.status !== CALLBACK_STATUS.approved) return yield* Effect.fail(decisionFailure(decision));
      if (output.format === OUTPUT_FORMAT.text) yield* output.info("Finalizing sign-in…");
      const token = yield* verifyAndDecrypt(options.sessionId, options.keypair, decision.verificationCode);
      const result = yield* complete(token);
      callback.finish(true);
      return result;
    }),
    (controls) => Effect.sync(() => controls.close()),
  );
