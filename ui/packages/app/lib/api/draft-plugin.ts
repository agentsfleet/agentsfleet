import { z } from "zod";
import { Instant } from "./decode";
import { DefinitionSchema, ReviewSchema } from "./draft-types";
import { setupRequest, SetupUnavailable } from "./draft-request";

export const PluginCardSchema = z.object({
  id: z.uuidv7(), name: z.string(), description: z.string(),
  head_revision_id: z.uuidv7(), head_revision: z.string().regex(/^[1-9]\d*$/), etag: z.string(), requirements: z.record(z.string(), z.unknown()),
  created_at: Instant, required_credentials_reasons: z.record(z.string(), z.string()),
});
export const PluginCardsSchema = z.object({items: z.array(PluginCardSchema).max(100), next_cursor: z.string().nullable()});
export const PluginRevisionSchema = z.object({id: z.uuidv7(), plugin_id: z.uuidv7(), revision: z.string(),
  definition: DefinitionSchema, canonical_digest: z.string()});
export const PluginSchema = z.object({card: PluginCardSchema, head: PluginRevisionSchema, review: ReviewSchema, etag: z.string()});
const RevisionsSchema = z.object({items: z.array(z.object({id: z.uuidv7(), revision: z.string(), canonical_digest: z.string(), created_at: Instant})).max(25), next_cursor: z.string().nullable()});
export type PluginCard = z.infer<typeof PluginCardSchema>;
export type Plugin = z.infer<typeof PluginSchema>;

export function pluginPage(workspace: string, cursor: string | null, signal: AbortSignal) {
  const query = new URLSearchParams({limit: "25"});
  if (cursor) query.set("starting_after", cursor);
  return setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/plugins?${query}`}, PluginCardsSchema, signal);
}

export async function inspectPlugin(workspace: string, card: PluginCard, signal: AbortSignal, revision = card.head_revision_id) {
  const query = new URLSearchParams({revision_id: revision});
  const entry = await setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/plugins/${encodeURIComponent(card.id)}?${query}`}, PluginSchema, signal);
  if (entry.card.id !== card.id || entry.head.plugin_id !== card.id || entry.head.id !== revision) throw new SetupUnavailable("The selected Plugin revision could not be confirmed.", 409);
  return entry;
}

export function pluginHistory(workspace: string, plugin: string, cursor: string | null, signal: AbortSignal) {
  const query = new URLSearchParams({limit: "25"});
  if (cursor) query.set("starting_after", cursor);
  return setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/plugins/${encodeURIComponent(plugin)}/revisions?${query}`}, RevisionsSchema, signal);
}
