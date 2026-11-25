// A browser request through the same-origin `/live` proxy. The browser holds
// no bearer: the route mints it, so this sends cookies, never a token, and
// reads every answer against a schema before a caller sees it.

import { z } from "zod";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";

export const REQUEST_METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const JSON_TYPE = "application/json";
const REFUSED = "This request couldn’t be completed. Refresh the page before trying again.";
const UNREADABLE = "We couldn’t load this response. Please try again shortly.";
const UNCONFIRMED = "The request could not be confirmed. Retry the same operation.";
const EMPTY = "The response was empty";
// Only the curated `user_message` reaches a person; a problem's `detail` is a diagnostic.
const ProblemSchema = z.object({user_message: z.string().optional(), error_code: z.string().optional()});

export type LiveRequest = {readonly path: string; readonly method?: string; readonly body?: unknown; readonly headers?: Readonly<Record<string, string>>};

/** A request the daemon refused, or one whose answer never arrived (status 502). */
export class RequestFailed extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly etag: string | null;
  constructor(detail: string, status = 502, options?: {cause?: unknown; code?: string; etag?: string | null}) {
    super(detail, {cause: options?.cause}); this.name = "RequestFailed";
    this.status = status; this.code = options?.code; this.etag = options?.etag ?? null;
  }
}

export async function liveRequest<T>(request: LiveRequest, schema: z.ZodType<T>, signal: AbortSignal): Promise<T> {
  try {
    signal.throwIfAborted();
    const response = await fetch(request.path, {method: request.method ?? REQUEST_METHOD.get, credentials: "same-origin",
      headers: {Accept: JSON_TYPE, ...(request.body === undefined ? {} : {"Content-Type": JSON_TYPE}), ...request.headers},
      ...(request.body === undefined ? {} : {body: JSON.stringify(request.body)}), signal, cache: "no-store", redirect: "error"});
    const value = response.status === 204 ? null : await readJson(response, signal).catch((cause: unknown) => {
      if (response.ok) throw cause;
      return null;
    });
    if (!response.ok) {
      const problem = ProblemSchema.safeParse(value);
      throw new RequestFailed((problem.success && problem.data.user_message) || REFUSED, response.status,
        {code: problem.success ? problem.data.error_code : undefined, etag: response.headers.get("etag")});
    }
    const parsed = schema.safeParse(value);
    if (!parsed.success) throw new RequestFailed(UNREADABLE, 502, {cause: parsed.error});
    signal.throwIfAborted();
    return parsed.data;
  } catch (cause) {
    if (cause instanceof RequestFailed) throw cause;
    throw new RequestFailed(UNCONFIRMED, 502, {cause});
  }
}

/** The caller lost its access to the resource, or the resource is gone. */
export function losesAccess(cause: unknown): cause is RequestFailed {
  return cause instanceof RequestFailed && [401, 403, 404, 410].includes(cause.status);
}

export function requestErrorMessage(cause: unknown): string {
  if (cause instanceof RequestFailed) {
    if (cause.status === 401) return "Your sign-in session has expired. Reload this page to sign in again.";
    if (cause.status === 403) return "Your account doesn’t have access here. Switch workspace or ask the workspace owner for access.";
    if (cause.status === 404 || cause.status === 410) return "This is no longer available. It may have been removed, or your access may have changed.";
    return cause.message;
  }
  return UNCONFIRMED_REQUEST_MESSAGE;
}

/** The daemon answered no; a retry of the same request gets the same answer. */
export function definiteRefusal(cause: unknown): boolean {
  return cause instanceof RequestFailed && cause.status >= 400 && cause.status < 500 && cause.status !== 408;
}

async function readJson(response: Response, signal: AbortSignal): Promise<unknown> {
  const reader = response.body?.getReader();
  if (!reader) throw new RequestFailed(EMPTY);
  const cancel = (): void => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, {once: true});
  let text = "";
  const decoder = new TextDecoder();
  try {
    signal.throwIfAborted();
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
      text += decoder.decode(chunk.value, {stream: true});
    }
    signal.throwIfAborted();
    return JSON.parse(text + decoder.decode());
  } finally {
    signal.removeEventListener("abort", cancel);
    try { await reader.cancel(); } catch { /* Cancellation must still release an errored body. */ }
    finally { reader.releaseLock(); }
  }
}
