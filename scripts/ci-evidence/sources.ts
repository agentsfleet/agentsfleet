import { z } from "zod";
import type { Case } from "./schema.ts";
import { callSchema, eventSchema } from "./native.ts";
import { Records, requireProof } from "./records.ts";

export type CallDetail = z.infer<typeof callSchema>;
const request = z.object({url: z.url(), method: z.string().optional(), body: z.string().optional()}).passthrough();
const STOP_CODES = new Set(["https_required", "method_not_allowed", "host_not_allowed", "address_not_allowed",
  "credential_placement_not_allowed", "credential_host_not_allowed", "secret_not_found", "request_policy_not_allowed",
  "credential_mint_refused", "workspace_memory_not_granted", "tool_not_offered", "path_not_allowed", "subcommand_not_allowed"]);

/** Parse only the HTTP tool's machine envelope; the response body is untrusted data. */
export function httpEnvelope(output: string) {
  const separator = "\nResponse Body:\n";
  const boundary = output.indexOf(separator);
  const headers = boundary < 0 ? output : output.slice(0, boundary);
  const status = /^(?:\[[a-z_]+\] )?Status: ([1-5][0-9]{2})\r?$/m.exec(headers)?.[1];
  return {status: status === undefined ? undefined : Number(status), body: boundary < 0 ? "" : output.slice(boundary + separator.length),
    plain: /^Content-Type: text\/plain(?:\s*;[^\n]*)?\r?$/im.test(headers),
    truncated: /^\[Response truncated at [0-9]+ bytes\]\r?$/m.test(headers),
    lossy: /^\[Response contains replaced non-UTF-8 bytes\]\r?$/m.test(headers)};
}

export function terminalRefusal(output: string): boolean {
  const code = /^\[([a-z_]+)\]/.exec(output)?.[1];
  const status = httpEnvelope(output).status;
  return (code !== undefined && STOP_CODES.has(code)) || status === 401 || status === 403;
}

export function failedSource(detail: CallDetail): boolean {
  const args = request.safeParse(detail.response.arguments);
  if (!args.success || (args.data.method ?? "GET").toUpperCase() !== "GET") return false;
  const envelope = httpEnvelope(detail.response.output);
  return envelope.status !== 200 || envelope.truncated || envelope.lossy || detail.response.truncated;
}

/** The one allowed absence probe is immediately reconciled against all PR states. */
export function expectedRefAbsence(detail: CallDetail, next: CallDetail | undefined, eventId: string): boolean {
  const args = request.safeParse(detail.response.arguments);
  if (!args.success || httpEnvelope(detail.response.output).status !== 404 || !next) return false;
  const branch = `agentsfleet-repair/${Buffer.from(eventId).toString("base64url")}`;
  const prefix = "https://api.github.com/repos/agentsfleet/linkwarden";
  if (decodeURIComponent(args.data.url) !== `${prefix}/git/ref/heads/${branch}`) return false;
  const following = request.safeParse(next.response.arguments);
  if (!following.success) return false;
  const url = new URL(following.data.url);
  if (url.origin + url.pathname !== `${prefix}/pulls` || url.searchParams.get("state") !== "all"
    || url.searchParams.get("base") !== "dev" || url.searchParams.get("head") !== `agentsfleet:${branch}`) return false;
  try {
    const pulls: unknown = JSON.parse(completeGet(next, following.data.url, "source.ref_absence"));
    return Array.isArray(pulls) && pulls.length === 0;
  }
  catch {return false;}
}

export function getArguments(detail: CallDetail, field: string) {
  const parsed = request.safeParse(detail.response.arguments);
  requireProof(parsed.success && (parsed.data.method ?? "GET").toUpperCase() === "GET" && parsed.data.body === undefined,
    field, "required source was not read with GET");
  return parsed.data;
}

export function completeGet(detail: CallDetail, address: string, field: string, plain = false): string {
  const args = getArguments(detail, field);
  const envelope = httpEnvelope(detail.response.output);
  requireProof(args.url === address && envelope.status === 200 && envelope.body.trim().length > 0
    && (!plain || envelope.plain) && !envelope.truncated && !envelope.lossy
    && !detail.response.truncated && !detail.response.truncated_arguments, field, "required source is unavailable, incomplete or belongs to another address");
  return envelope.body;
}

export function intentionalLogTruncation(detail: CallDetail): boolean {
  const args = request.safeParse(detail.response.arguments);
  const envelope = httpEnvelope(detail.response.output);
  return args.success && (args.data.method ?? "GET").toUpperCase() === "GET" && args.data.body === undefined
    && new URL(args.data.url).origin === "https://api.github.com"
    && /^\/repos\/[^/]+\/[^/]+\/actions\/jobs\/[1-9][0-9]*\/logs$/.test(new URL(args.data.url).pathname)
    && envelope.status === 200 && envelope.plain && envelope.truncated && !envelope.lossy;
}

/** A preparation read cannot substitute for the fleet's own successful tool call. */
export function ownedGet(tools: Case["tool_records"], event: z.infer<typeof eventSchema>, records: Records,
  address: string, field: string, plain = false): string {
  const matching = tools.flatMap(call => {
    const detail = records.json(call.record, callSchema, field);
    const args = request.safeParse(detail.response.arguments);
    return args.success && args.data.url === address ? [{call, detail}] : [];
  });
  requireProof(matching.length > 0, field, "fleet-owned source read is missing");
  const bodies = matching.map(({call, detail}) => {
    requireProof(event.tool_calls?.calls.some(item => item.call_id === call.call_id && item.name === "http_request"
      && item.status === "succeeded"), field, "source tool identity or outcome differs");
    return completeGet(detail, address, field, plain);
  });
  return bodies.at(-1) ?? "";
}
