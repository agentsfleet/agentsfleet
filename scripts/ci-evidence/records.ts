import type { ZodType } from "zod";
import type { Ref, Report } from "./schema.ts";

export class EvidenceError extends Error {
  constructor(readonly field: string, readonly reason: string, readonly status: 1 | 2 = 1) {
    super(`${field}: ${reason}`);
  }
}

export function requireProof(condition: unknown, field: string, reason: string): asserts condition {
  if (!condition) throw new EvidenceError(field, reason);
}

export function hash(text: string): string {return new Bun.CryptoHasher("sha256").update(text).digest("hex");}

export function canonical(value: unknown): string {
  if (value === null || typeof value === "boolean") return JSON.stringify(value);
  if (typeof value === "string" && value.isWellFormed()) return JSON.stringify(value);
  if (typeof value === "number" && Number.isFinite(value)) return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
    return `{${entries.map(([key, child]) => `${canonical(key)}:${canonical(child)}`).join(",")}}`;
  }
  throw new EvidenceError("canonical_json", "contains an unsupported number, string or value", 2);
}

export function decode<T>(schema: ZodType<T>, value: unknown, field: string): T {
  const parsed = schema.safeParse(value);
  if (!parsed.success) throw new EvidenceError(field, "invalid required shape", 2);
  return parsed.data;
}

export class Records {
  readonly #records = new Map<string, string>();
  constructor(records: Report["records"]) {
    for (const record of records) {
      requireProof(!this.#records.has(record.name), "records", "duplicate record name");
      this.#records.set(record.name, record.text);
    }
  }

  text(ref: Ref, field: string): string {
    const text = this.#records.get(ref.path);
    requireProof(text !== undefined, field, "record is missing");
    requireProof(hash(text) === ref.sha256, field, "record digest differs");
    return text;
  }

  json<T>(ref: Ref, schema: ZodType<T>, field: string): T {
    let value: unknown;
    try {value = JSON.parse(this.text(ref, field));}
    catch (error) {
      if (error instanceof EvidenceError) throw error;
      throw new EvidenceError(field, "record is not JSON", 2);
    }
    return decode(schema, value, field);
  }
}

export function bytesAt(text: string, span: [number, number], field: string): Buffer {
  const bytes = Buffer.from(text);
  const [start, end] = span;
  requireProof(start < end && end <= bytes.length, field, "span is empty or outside retained text");
  const selected = bytes.subarray(start, end);
  try {new TextDecoder("utf-8", {fatal: true}).decode(selected);}
  catch {throw new EvidenceError(field, "span splits UTF-8 text");}
  return selected;
}
