// Shared fixtures and doubles for the token lifecycle acceptance suites.
//
// Extracted when lifecycle-with-token.spec.ts passed the repository's 350-line cap;
// the suites that read them are unchanged.

import crypto from "node:crypto";
import path from "node:path";
import url from "node:url";

export const HERE = path.dirname(url.fileURLToPath(import.meta.url));
export const CLI_ROOT = path.resolve(HERE, "..", "..");

export const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
export const isLive = target.startsWith("https://");

export interface ValidateResult {
  readonly ok: boolean;
  readonly message: string;
}

export interface ValidateModule {
  validateRequiredId(value: string, label: string): ValidateResult;
}

// Random uuidv7 for the invalid-arg-value sweep — backend's `isUuidV7`
// rejects v4, so `crypto.randomUUID()` would surface as a 400/validation
// error instead of 404. Hand-roll a v7 with valid version+variant bits
// and random payload so the server's not-found branch fires.
export function randomUuidv7(): string {
  const bytes = crypto.randomBytes(16);
  const tsMs = BigInt(Date.now());
  bytes[0] = Number((tsMs >> 40n) & 0xffn);
  bytes[1] = Number((tsMs >> 32n) & 0xffn);
  bytes[2] = Number((tsMs >> 24n) & 0xffn);
  bytes[3] = Number((tsMs >> 16n) & 0xffn);
  bytes[4] = Number((tsMs >> 8n) & 0xffn);
  bytes[5] = Number(tsMs & 0xffn);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x70;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
