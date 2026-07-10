// agentsfleet input validators — two flavors:
//
//   1. Commander option/argument parsers (parseStringOption, parseIntOption,
//      parseIdOption, …). Each parser throws InvalidArgumentError on rejection
//      — commander catches and renders `error: option '--foo <bar>' argument
//      '<value>' is invalid. <message>` then exits with code 2. The factory
//      variants (parseIntOption, parseEnumOption, parsePathOption,
//      parseJsonObjectOption) return a (value) => parsed callback so they
//      wire directly into commander's `.option(flags, description, fn)`.
//      Direct vs factory split mirrors oracle's options.ts.
//
//   2. Handler-side type guards / result-bag validators (isValidId,
//      validateRequiredId). Called by leaf handlers AFTER commander has
//      handed off, when a positional/flag has to be re-checked before
//      round-tripping — e.g. workspace.show accepts either a positional or
//      `--workspace-id`, and the handler chooses which one to validate.
//      validateRequiredId returns `{ ok, message }` instead of throwing so
//      the handler can format the failure into a normal `ui.err()` line
//      without unwinding through commander's exit path.
//
// One module = one mental model. The uuidv7 check + EXAMPLE_UUIDV7 literal
// live exactly once: isValidId is the impl; parseIdOption + validateRequiredId
// both call it. Single source of truth: server ids are uuidv7
// (`src/types/id_format.zig → allocUuidV7`); the CLI rejects malformed shape
// client-side to save a round-trip. `uuid` npm package (Apache-2.0, no
// postinstall, single dep tree) is vetted supply-chain posture for runtime.

import { InvalidArgumentError } from "commander";
import path from "node:path";
import fs from "node:fs";
import { validate as isValidUuid, version as uuidVersion } from "uuid";
import { HTTPS_SCHEME_PREFIX } from "../constants/custom-endpoint.ts";

export const EXAMPLE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";

export const INTEGER_RE = /^-?\d+$/;
const NUMBER_RE = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;
const DURATION_RE = /^(\d+)(ms|s|m|h)$/;
const MUST_BE_A_NUMBER = "must be a number" as const;
const MUST_BE_AN_INTEGER = "must be an integer" as const;
const REQUIRED = "required" as const;
const TYPE_STRING = "string" as const;
// `URL.protocol` returns the scheme with a trailing colon, no slashes.
const HTTPS_PROTOCOL = "https:" as const;
const MS_PER_SECOND = 1000 as const;
const DURATION_FACTOR: Record<"ms" | "s" | "m" | "h", number> = {
  ms: 1,
  s: MS_PER_SECOND,
  m: 60_000,
  h: 3_600_000,
};
const DEFAULT_JSON_MAX_BYTES = 4096;

const isString = (value: unknown): value is string => typeof value === TYPE_STRING;

export interface IntBounds {
  min?: number | undefined;
  max?: number | undefined;
}

export interface PathOptions {
  mustExist?: boolean | undefined;
}

export interface JsonObjectOptions {
  maxBytes?: number | undefined;
}

export type CommanderParser<T> = (value: unknown) => T;

export function parseStringOption(value: unknown): string {
  if (!isString(value) || value.trim().length === 0) {
    throw new InvalidArgumentError("must be a non-empty string");
  }
  return value.trim();
}

export function parseIntOption({ min, max }: IntBounds = {}): CommanderParser<number> {
  return (value: unknown): number => {
    const trimmed = String(value ?? "").trim();
    if (!INTEGER_RE.test(trimmed)) {
      throw new InvalidArgumentError(MUST_BE_AN_INTEGER);
    }
    const parsed = Number.parseInt(trimmed, 10);
    if (!Number.isFinite(parsed)) {
      throw new InvalidArgumentError(MUST_BE_AN_INTEGER);
    }
    if (min !== undefined && parsed < min) {
      throw new InvalidArgumentError(`must be ≥ ${min}`);
    }
    if (max !== undefined && parsed > max) {
      throw new InvalidArgumentError(`must be ≤ ${max}`);
    }
    return parsed;
  };
}

export function parseFloatOption(value: unknown): number {
  const trimmed = String(value ?? "").trim();
  if (!NUMBER_RE.test(trimmed)) {
    throw new InvalidArgumentError(MUST_BE_A_NUMBER);
  }
  const parsed = Number.parseFloat(trimmed);
  if (!Number.isFinite(parsed)) {
    throw new InvalidArgumentError(MUST_BE_A_NUMBER);
  }
  return parsed;
}

export function parseIdOption(value: unknown): string {
  if (!isString(value) || value.length === 0) {
    throw new InvalidArgumentError(REQUIRED);
  }
  if (!isValidId(value)) {
    throw new InvalidArgumentError(`expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`);
  }
  return value;
}

export function parseEnumOption<T extends string>(allowed: readonly T[]): CommanderParser<T> {
  if (!Array.isArray(allowed) || allowed.length === 0) {
    throw new Error("parseEnumOption requires a non-empty allowed array");
  }
  return (value: unknown): T => {
    if (!isString(value) || !allowed.includes(value as T)) {
      throw new InvalidArgumentError(`must be one of: ${allowed.join(", ")}`);
    }
    return value as T;
  };
}

export function parsePathOption({ mustExist = false }: PathOptions = {}): CommanderParser<string> {
  return (value: unknown): string => {
    if (!isString(value) || value.length === 0) {
      throw new InvalidArgumentError(REQUIRED);
    }
    const resolved = path.resolve(value);
    if (mustExist && !fs.existsSync(resolved)) {
      throw new InvalidArgumentError(`path does not exist: ${value}`);
    }
    return resolved;
  };
}

// Client-side https-scheme gate for a custom-endpoint `--base-url`. Commander
// runs this during parse — BEFORE any handler dispatches — so a non-https URL
// is rejected (exit 2, human-text stderr) with NO network call. This is the
// only client-side check; full SSRF validation (loopback/private/metadata
// hosts) stays server-side in base_url_guard.zig, surfaced as a typed UZ-* JSON
// error. Parses as a WHATWG URL so a malformed value is rejected for the same
// reason rather than slipping through a bare prefix test.
export function parseHttpsUrlOption(value: unknown): string {
  if (!isString(value) || value.trim().length === 0) {
    throw new InvalidArgumentError(REQUIRED);
  }
  const trimmed = value.trim();
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new InvalidArgumentError(`must be a valid URL (e.g. ${HTTPS_SCHEME_PREFIX}host/v1)`);
  }
  if (parsed.protocol !== HTTPS_PROTOCOL) {
    throw new InvalidArgumentError(`must use https:// (got ${parsed.protocol}//)`);
  }
  return trimmed;
}

export function parseDurationOption(value: unknown): number {
  const match = DURATION_RE.exec(String(value ?? "").trim());
  if (!match) {
    throw new InvalidArgumentError("expected a duration like 30m, 10s, 500ms, or 2h");
  }
  const digits = match[1] ?? "";
  // DURATION_RE constrains match[2] to keyof DURATION_FACTOR — the
  // earlier null/`unit in DURATION_FACTOR` guard was dead code (regex
  // can't produce any other unit). If a future regex change adds a
  // new unit, extend DURATION_FACTOR in the same diff.
  const unit = match[2] as keyof typeof DURATION_FACTOR;
  const n = Number.parseInt(digits, 10);
  if (n <= 0) {
    throw new InvalidArgumentError("duration must be positive");
  }
  return n * DURATION_FACTOR[unit];
}

export function parseJsonObjectOption(
  { maxBytes = DEFAULT_JSON_MAX_BYTES }: JsonObjectOptions = {},
): CommanderParser<Record<string, unknown>> {
  return (value: unknown): Record<string, unknown> => {
    if (!isString(value)) {
      throw new InvalidArgumentError("must be a string of JSON");
    }
    if (Buffer.byteLength(value, "utf8") > maxBytes) {
      throw new InvalidArgumentError(`payload must be ≤ ${maxBytes} bytes`);
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(value);
    } catch {
      throw new InvalidArgumentError("must be valid JSON");
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new InvalidArgumentError("must be a JSON object (not array or primitive)");
    }
    return parsed as Record<string, unknown>;
  };
}

// ── Handler-side type guards / result-bag validators ─────────────────

export type ValidateResult =
  | { ok: true }
  | { ok: false; message: string };

export function isValidId(value: unknown): value is string {
  if (!value || !isString(value)) return false;
  if (!isValidUuid(value)) return false;
  return uuidVersion(value) === 7;
}

export function validateRequiredId(
  value: unknown,
  name: string,
): ValidateResult {
  if (!value || !isString(value) || value.trim().length === 0) {
    return { ok: false, message: `${name} is required` };
  }
  if (!isValidId(value)) {
    return {
      ok: false,
      message: `invalid ${name}: expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`,
    };
  }
  return { ok: true };
}
