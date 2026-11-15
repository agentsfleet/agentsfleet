// Failure identity and copy stay independent of the setup transport.
import type { DraftList } from "./draft-list-types";
import { UNCONFIRMED_REQUEST_MESSAGE } from "@/lib/errors";
import { DRAFT_LIST } from "./draft-list-types";

export class SetupUnavailable extends Error {
  readonly status: number;
  readonly code: string | undefined;
  readonly etag: string | null;
  readonly reviewDigest: string | undefined;
  readonly list: DraftList | undefined;
  readonly userMessage: string | undefined;
  constructor(detail: string, status = 502, options?: {cause?: unknown; code?: string; etag?: string | null; reviewDigest?: string; list?: DraftList; userMessage?: string}) {
    super(detail, {cause: options?.cause}); this.name = "SetupUnavailable";
    this.status = status; this.code = options?.code; this.etag = options?.etag ?? null; this.reviewDigest = options?.reviewDigest;
    this.list = options?.list; this.userMessage = options?.userMessage;
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
  if (opening && cause instanceof SetupUnavailable && cause.list) {
    const subject = cause.list === DRAFT_LIST.models ? "model choices" : "chats";
    return `We couldn’t load your ${subject}. ${cause.userMessage || "Try again in a moment."}`;
  }
  if (opening) return "We couldn’t load fleet setup. Try again in a moment.";
  return cause instanceof SetupUnavailable ? cause.message : UNCONFIRMED_REQUEST_MESSAGE;
}

export function definiteRefusal(cause: unknown): boolean {
  return cause instanceof SetupUnavailable && cause.status >= 400 && cause.status < 500 && cause.status !== 408;
}
