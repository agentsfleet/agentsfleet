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
import { validate as isValidUuid, version as uuidVersion } from "uuid";

const EXAMPLE_UUIDV7 = "0192a3b4-c5d6-7e8f-9012-345678901234";
const INTEGER_RE = /^-?\d+$/;
const NUMBER_RE = /^-?\d+(\.\d+)?([eE][-+]?\d+)?$/;
const DURATION_RE = /^(\d+)(ms|s|m|h)$/;
const DURATION_FACTOR = { ms: 1, s: 1000, m: 60_000, h: 3_600_000 };
const DEFAULT_JSON_MAX_BYTES = 4096;

export function parseStringOption(value) {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new InvalidArgumentError("must be a non-empty string");
  }
  return value.trim();
}

export function parseIntOption({ min, max } = {}) {
  return (value) => {
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

export function parseFloatOption(value) {
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

export function parseIdOption(value) {
  if (typeof value !== "string" || value.length === 0) {
    throw new InvalidArgumentError("required");
  }
  if (!isValidUuid(value) || uuidVersion(value) !== 7) {
    throw new InvalidArgumentError(`expected uuidv7 format (e.g. ${EXAMPLE_UUIDV7})`);
  }
  return value;
}

export function parseEnumOption(allowed) {
  if (!Array.isArray(allowed) || allowed.length === 0) {
    throw new Error("parseEnumOption requires a non-empty allowed array");
  }
  return (value) => {
    if (!allowed.includes(value)) {
      throw new InvalidArgumentError(`must be one of: ${allowed.join(", ")}`);
    }
    return value;
  };
}

export function parsePathOption({ mustExist = false } = {}) {
  return (value) => {
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

export function parseDurationOption(value) {
  const match = DURATION_RE.exec(String(value ?? "").trim());
  if (!match) {
    throw new InvalidArgumentError("expected a duration like 30m, 10s, 500ms, or 2h");
  }
  const n = Number.parseInt(match[1], 10);
  if (n <= 0) {
    throw new InvalidArgumentError("duration must be positive");
  }
  return n * DURATION_FACTOR[match[2]];
}

export function parseJsonObjectOption({ maxBytes = DEFAULT_JSON_MAX_BYTES } = {}) {
  return (value) => {
    if (typeof value !== "string") {
      throw new InvalidArgumentError("must be a string of JSON");
    }
    if (Buffer.byteLength(value, "utf8") > maxBytes) {
      throw new InvalidArgumentError(`payload must be ≤ ${maxBytes} bytes`);
    }
    let parsed;
    try {
      parsed = JSON.parse(value);
    } catch {
      throw new InvalidArgumentError("must be valid JSON");
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new InvalidArgumentError("must be a JSON object (not array or primitive)");
    }
    return parsed;
  };
}
