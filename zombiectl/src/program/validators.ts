// Commander option/argument validators. Each parser throws
// InvalidArgumentError on rejection — commander catches and renders
// `error: option '--foo <bar>' argument '<value>' is invalid. <message>`
// then exits with code 2. The factory variants (parseIntOption,
// parseEnumOption, parsePathOption, parseJsonObjectOption) return a
// (value) => parsed callback so they wire directly into commander's
// `.option(flags, description, fn)` signature.
//
// Direct vs factory split mirrors oracle's options.ts: configurable
// parsers are factories, parameterless ones are direct callbacks.

import { InvalidArgumentError } from "commander";
import path from "node:path";
import fs from "node:fs";
import { isValidId, EXAMPLE_UUIDV7 } from "./validate.ts";

const INTEGER_RE = /^-?\d+$/;
const NUMBER_RE = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;
const DURATION_RE = /^(\d+)(ms|s|m|h)$/;
const DURATION_FACTOR: Record<"ms" | "s" | "m" | "h", number> = {
  ms: 1,
  s: 1000,
  m: 60_000,
  h: 3_600_000,
};
const DEFAULT_JSON_MAX_BYTES = 4096;

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
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new InvalidArgumentError("must be a non-empty string");
  }
  return value.trim();
}

export function parseIntOption({ min, max }: IntBounds = {}): CommanderParser<number> {
  return (value: unknown): number => {
    const trimmed = String(value ?? "").trim();
    if (!INTEGER_RE.test(trimmed)) {
      throw new InvalidArgumentError("must be an integer");
    }
    const parsed = Number.parseInt(trimmed, 10);
    if (!Number.isFinite(parsed)) {
      throw new InvalidArgumentError("must be an integer");
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
    throw new InvalidArgumentError("must be a number");
  }
  const parsed = Number.parseFloat(trimmed);
  if (!Number.isFinite(parsed)) {
    throw new InvalidArgumentError("must be a number");
  }
  return parsed;
}

export function parseIdOption(value: unknown): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new InvalidArgumentError("required");
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
    if (typeof value !== "string" || !allowed.includes(value as T)) {
      throw new InvalidArgumentError(`must be one of: ${allowed.join(", ")}`);
    }
    return value as T;
  };
}

export function parsePathOption({ mustExist = false }: PathOptions = {}): CommanderParser<string> {
  return (value: unknown): string => {
    if (typeof value !== "string" || value.length === 0) {
      throw new InvalidArgumentError("required");
    }
    const resolved = path.resolve(value);
    if (mustExist && !fs.existsSync(resolved)) {
      throw new InvalidArgumentError(`path does not exist: ${value}`);
    }
    return resolved;
  };
}

export function parseDurationOption(value: unknown): number {
  const match = DURATION_RE.exec(String(value ?? "").trim());
  if (!match) {
    throw new InvalidArgumentError("expected a duration like 30m, 10s, 500ms, or 2h");
  }
  const digits = match[1] ?? "";
  const unit = match[2] as "ms" | "s" | "m" | "h" | undefined;
  const n = Number.parseInt(digits, 10);
  if (n <= 0) {
    throw new InvalidArgumentError("duration must be positive");
  }
  if (!unit || !(unit in DURATION_FACTOR)) {
    throw new InvalidArgumentError("expected a duration like 30m, 10s, 500ms, or 2h");
  }
  return n * DURATION_FACTOR[unit];
}

export function parseJsonObjectOption(
  { maxBytes = DEFAULT_JSON_MAX_BYTES }: JsonObjectOptions = {},
): CommanderParser<Record<string, unknown>> {
  return (value: unknown): Record<string, unknown> => {
    if (typeof value !== "string") {
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
