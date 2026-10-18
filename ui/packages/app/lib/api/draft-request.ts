// Private setup never reaches browser storage, analytics, or a browser bearer.

import { z } from "zod";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";

export const REQUEST_METHOD = {get: "GET", post: "POST", patch: "PATCH", delete: "DELETE"} as const;
const RESPONSE_LIMIT_BYTES = 4 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;
const JSON_TYPE = "application/json";
const ProblemSchema = z.object({
  user_message: z.string().optional(), error_code: z.string().optional(),
  review_digest: z.string().optional(),
});
export type SetupRequest = {readonly path: string; readonly method?: string; readonly body?: unknown; readonly headers?: Readonly<Record<string, string>>};

export class SetupUnavailable extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly etag: string | null;
  readonly reviewDigest: string | undefined;
  constructor(detail: string, status = 502, options?: {cause?: unknown; code?: string; etag?: string | null; reviewDigest?: string}) {
    super(detail, {cause: options?.cause}); this.name = "SetupUnavailable";
    this.status = status; this.code = options?.code; this.etag = options?.etag ?? null; this.reviewDigest = options?.reviewDigest;
  }
}

export function losesAccess(cause: unknown): cause is SetupUnavailable {
  return cause instanceof SetupUnavailable && [401, 403, 404, 410].includes(cause.status);
}

export function setupErrorMessage(cause: unknown, opening = false): string {
  if (cause instanceof SetupUnavailable) {
    if (cause.status === 401) return "Your sign-in session has expired. Reload this page to sign in again.";
    if (cause.status === 403) return "Your account doesn’t have access here. Switch workspace or ask the workspace owner for access.";
    if (cause.status === 404 || cause.status === 410) return opening
      ? "Fleet setup isn’t available in this workspace right now. Try again shortly, or return to Your fleets."
      : "This chat is no longer available. It may have been removed, or your access may have changed.";
  }
  if (opening) return "We couldn’t load your models and chats. Try again in a moment.";
  return cause instanceof SetupUnavailable ? cause.message : UNCONFIRMED_REQUEST_MESSAGE;
}

export function definiteRefusal(cause: unknown): boolean {
  return cause instanceof SetupUnavailable && cause.status >= 400 && cause.status < 500 && cause.status !== 408;
}

export async function setupRequest<T>(request: SetupRequest, schema: z.ZodType<T>, owner: AbortSignal): Promise<T> {
  const signal = AbortSignal.any([owner, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]);
  try {
    signal.throwIfAborted();
    const response = await fetch(request.path, {method: request.method ?? REQUEST_METHOD.get, credentials: "same-origin",
      headers: {Accept: JSON_TYPE, ...(request.body === undefined ? {} : {"Content-Type": JSON_TYPE}), ...request.headers},
      ...(request.body === undefined ? {} : {body: JSON.stringify(request.body)}), signal, cache: "no-store", redirect: "error"});
    const value = response.status === 204 ? null : await boundedJson(response, signal).catch((cause: unknown) => {
      if (response.ok) throw cause;
      return null;
    });
    if (!response.ok) {
      const problem = ProblemSchema.safeParse(value);
      throw new SetupUnavailable(problem.success && problem.data.user_message
        ? problem.data.user_message : "This request couldn’t be completed. Refresh the page before trying again.", response.status,
        {code: problem.success ? problem.data.error_code : undefined,
          etag: response.headers.get("etag"), reviewDigest: problem.success ? problem.data.review_digest : undefined});
    }
    const parsed = schema.safeParse(value);
    if (!parsed.success) throw new SetupUnavailable("We couldn’t load this response. Please try again shortly.", 502, {cause: parsed.error});
    signal.throwIfAborted();
    return parsed.data;
  } catch (cause) {
    if (cause instanceof SetupUnavailable) throw cause;
    throw new SetupUnavailable("The request could not be confirmed. Retry the same operation.", 502, {cause});
  }
}

async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  const reader = response.body?.getReader();
  if (!reader) throw new SetupUnavailable("The setup response was empty");
  const cancel = (): void => { void reader.cancel().catch(() => undefined); };
  signal.addEventListener("abort", cancel, {once: true});
  let size = 0;
  let text = "";
  const decoder = new TextDecoder();
  try {
    signal.throwIfAborted();
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
      size += chunk.value.byteLength;
      if (size > RESPONSE_LIMIT_BYTES) { await reader.cancel(); throw new SetupUnavailable("The setup response exceeds the supported limit"); }
      text += decoder.decode(chunk.value, {stream: true});
    }
    signal.throwIfAborted();
    return JSON.parse(text + decoder.decode());
  } finally { signal.removeEventListener("abort", cancel); reader.releaseLock(); }
}

export type DeleteDraft = (id: string, signal: AbortSignal) => Promise<void>;

export async function deleteDraft(base: string, id: string, signal: AbortSignal): Promise<void> {
  await setupRequest({path: `${base}/drafts/${encodeURIComponent(id)}`, method: REQUEST_METHOD.delete}, z.null(), signal);
}
