import { expect } from "@playwright/test";
import { z } from "zod";
import { MessagesSchema, type Message } from "@/lib/api/draft-types";
import { type ApiClient } from "./api-client";
import { CONTINUITY } from "./continuity-scenario";

const ThreadItem = z.object({
  event_id: z.string(), fleet_id: z.string(), workspace_id: z.string(), status: z.string(),
  event_type: z.string(), actor: z.string(),
  request_json: z.string(), response_text: z.string().nullable(),
  failure_label: z.string().nullable().optional(),
});
const ThreadPage = z.object({items: z.array(ThreadItem), next_cursor: z.string().nullable()});
export type ContinuityEvent = z.infer<typeof ThreadItem>;

/** Read every page; a duplicate cursor must fail rather than hide older turns. */
export async function draftHistory(client: ApiClient, scope: string): Promise<Message[]> {
  const rows: Message[] = [];
  const seen = new Set<string>();
  let cursor: string | null = null;
  for (let page = 0; page < CONTINUITY.maxHistoryPages; page++) {
    const query = new URLSearchParams({limit: String(CONTINUITY.pageLimit)});
    if (cursor) query.set("starting_after", cursor);
    const result = MessagesSchema.parse(await client.get<unknown>(`${scope}/messages?${query}`));
    rows.push(...result.items);
    if (!result.next_cursor) return rows;
    expect(seen.has(result.next_cursor), "setup history cursor progressed").toBe(false);
    seen.add(result.next_cursor);
    cursor = result.next_cursor;
  }
  throw new Error("Setup history exceeded the declared acceptance bound");
}

export async function threadPage(client: ApiClient, scope: string, cursor: string | null = null) {
  const query = new URLSearchParams({limit: String(CONTINUITY.pageLimit)});
  if (cursor) query.set("starting_after", cursor);
  return ThreadPage.parse(await client.get<unknown>(`${scope}/messages?${query}`));
}

export async function runtimeHistory(client: ApiClient, scope: string): Promise<ContinuityEvent[]> {
  const rows: ContinuityEvent[] = [];
  const seen = new Set<string>();
  let cursor: string | null = null;
  for (let page = 0; page < CONTINUITY.maxHistoryPages; page++) {
    const result = await threadPage(client, scope, cursor);
    rows.push(...result.items);
    if (!result.next_cursor) return rows;
    expect(seen.has(result.next_cursor), "fleet history cursor progressed").toBe(false);
    seen.add(result.next_cursor);
    cursor = result.next_cursor;
  }
  throw new Error("Fleet history exceeded the declared acceptance bound");
}
