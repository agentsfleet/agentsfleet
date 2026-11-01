import type { z } from "zod";
import { callSchema, eventSchema } from "./native.ts";
import { canonical, requireProof } from "./records.ts";

type Inventory = NonNullable<z.infer<typeof eventSchema>["tool_calls"]>["calls"][number];

/** Native trace projection rules: afr_agent/trace and afd_wire/tool_trace. */
export function checkProjection(call: Inventory, detail: z.infer<typeof callSchema>["response"], field: string): void {
  requireProof(call.output_line_count === undefined || call.output_line_count === detail.output_line_count, field, "native output counts contradict the full detail");
  requireProof(call.output_head === undefined || detail.output.startsWith(call.output_head), field, "native output head contradicts the full detail");
  requireProof(detail.truncated || call.output_tail === undefined || detail.output.endsWith(call.output_tail), field, "native output tail contradicts the full detail");
  if (call.arguments !== undefined) {
    const bounded = argumentProjection(detail.arguments);
    requireProof(canonical(call.arguments) === canonical(bounded), field, "native arguments contradict the full detail");
  }
}

export function argumentProjection(value: unknown): unknown {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return {};
  const bounded = rewrite(value);
  return Buffer.byteLength(JSON.stringify(bounded)) <= 2048 ? bounded : {};
}

function rewrite(value: unknown): unknown {
  if (typeof value === "string") return clip(value);
  if (Array.isArray(value)) return value.map(rewrite);
  if (typeof value === "object" && value !== null) return Object.fromEntries(Object.entries(value).map(([key, child]) => [clip(key), rewrite(child)]));
  return value;
}

function clip(value: string): string {
  const bytes = Buffer.from(value);
  let end = Math.min(bytes.length, 256);
  while (end > 0 && end < bytes.length && (bytes.readUInt8(end) & 0xc0) === 0x80) end--;
  return bytes.subarray(0, end).toString();
}
