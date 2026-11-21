import { callbackPage, PAGE_STATUS } from "./login-callback-page.ts";
import { HTTP_METHOD } from "../../constants/http-method.ts";
import {
  CALLBACK_DECISION, CALLBACK_FORM_TYPE, CALLBACK_IDLE_TIMEOUT_SECONDS, CALLBACK_PATH,
  CALLBACK_STATUS, LOOPBACK_HOST, MAX_CALLBACK_HEADER_BYTES, callbackState, parseCallbackSubmission,
  type CallbackDecision,
} from "./login-callback-protocol.ts";

const STAGE = { waiting: "waiting", finalizing: "finalizing", closed: "closed" } as const;
const SHUTDOWN_TIMEOUT_MS = 1_000;
const REFUSAL = "This terminal cannot accept that authorization. Start a new sign-in.";

interface CallbackOptions {
  readonly sessionId: string;
  readonly dashboardOrigin: string;
}

export class LoginCallback {
  readonly #server: Bun.Server<undefined>;
  readonly #sessionId: string;
  readonly #dashboardOrigin: string;
  readonly #secretState = callbackState();
  readonly #decision = Promise.withResolvers<CallbackDecision>();
  readonly #response = Promise.withResolvers<Response>();
  #stage: (typeof STAGE)[keyof typeof STAGE] = STAGE.waiting;

  constructor(options: CallbackOptions) {
    const origin = new URL(options.dashboardOrigin);
    if (origin.origin !== options.dashboardOrigin || !/^https?:$/.test(origin.protocol)) {
      throw new Error("invalid configured dashboard origin");
    }
    this.#sessionId = options.sessionId;
    this.#dashboardOrigin = origin.origin;
    this.#server = Bun.serve({
      hostname: LOOPBACK_HOST, port: 0, development: false,
      idleTimeout: CALLBACK_IDLE_TIMEOUT_SECONDS,
      fetch: (request, server) => this.#handle(request, server),
      error: () => this.#refuse(),
    });
  }

  get callbackUrl(): string { return `http://${LOOPBACK_HOST}:${this.#server.port}${CALLBACK_PATH}`; }
  get state(): string { return this.#secretState; }
  get waiting(): boolean { return this.#stage === STAGE.waiting; }
  waitForDecision(): Promise<CallbackDecision> { return this.#decision.promise; }

  cancel(): boolean { return this.#endWaiting(CALLBACK_DECISION.canceled); }

  finish(success: boolean): void {
    if (this.#stage !== STAGE.finalizing) return;
    this.#stage = STAGE.closed;
    this.#response.resolve(callbackPage(success ? PAGE_STATUS.success : PAGE_STATUS.failed));
  }

  async close(): Promise<void> {
    this.cancel();
    this.finish(false);
    const timer = setTimeout(() => { void this.#server.stop(true); }, SHUTDOWN_TIMEOUT_MS);
    try { await this.#server.stop(false); } finally { clearTimeout(timer); }
  }

  #endWaiting(status: typeof CALLBACK_DECISION.canceled): boolean {
    if (!this.waiting) return false;
    this.#stage = STAGE.closed;
    this.#decision.resolve({ status });
    return true;
  }

  #refuse(status = 400): Response {
    return new Response(REFUSAL, { status, headers: { "Cache-Control": "no-store", Connection: "close" } });
  }

  #validHeaders(request: Request): boolean {
    let bytes = 0;
    for (const [key, value] of request.headers) bytes += Buffer.byteLength(key) + Buffer.byteLength(value);
    const url = new URL(request.url);
    const type = request.headers.get("content-type")?.split(";")[0]?.trim();
    return bytes <= MAX_CALLBACK_HEADER_BYTES && request.method === HTTP_METHOD.post &&
      url.pathname === CALLBACK_PATH && !url.search &&
      request.headers.get("host") === `${LOOPBACK_HOST}:${this.#server.port}` &&
      request.headers.get("origin") === this.#dashboardOrigin && type === CALLBACK_FORM_TYPE;
  }

  async #handle(request: Request, server: Bun.Server<undefined>): Promise<Response> {
    if (!this.waiting) return this.#refuse(409);
    if (!this.#validHeaders(request)) return this.#refuse();
    const body = await this.#readBody(request);
    if (body === null || request.signal.aborted) return this.#refuse();
    const submitted = parseCallbackSubmission(body, this.#sessionId, this.#secretState);
    if (!submitted) return this.#refuse();
    if (!this.waiting) return this.#refuse(409);
    this.#stage = submitted.status === CALLBACK_STATUS.approved ? STAGE.finalizing : STAGE.closed;
    this.#decision.resolve(submitted);
    if (submitted.status === CALLBACK_STATUS.denied) return callbackPage(CALLBACK_STATUS.denied);
    // Finalization owns the reply; its HTTP calls can outlive the body idle bound.
    server.timeout(request, 0);
    return this.#response.promise;
  }

  async #readBody(request: Request): Promise<string | null> {
    try { return new TextDecoder("utf-8", { fatal: true }).decode(await request.arrayBuffer()); }
    catch { return null; }
  }
}
