import { z } from "zod";
import { DefinitionSchema, ReviewSchema } from "./draft-types";
import { setupRequest, SetupUnavailable } from "./draft-request";

export const LibraryCardSchema = z.object({
  id: z.string(), name: z.string(), description: z.string(), visibility: z.enum(["platform", "tenant"]),
  head_revision_id: z.uuidv7(), etag: z.string(), requirements: z.record(z.string(), z.unknown()),
  created_at: z.number().int(), required_credentials_reasons: z.record(z.string(), z.string()),
});
export const LibraryCardsSchema = z.object({items: z.array(LibraryCardSchema).max(100), next_cursor: z.string().nullable()});
export const LibraryRevisionSchema = z.object({id: z.uuidv7(), library_id: z.string(), revision: z.string(),
  definition: DefinitionSchema, canonical_digest: z.string()});
export const LibrarySchema = z.object({card: LibraryCardSchema, head: LibraryRevisionSchema, review: ReviewSchema, etag: z.string()});
const RevisionsSchema = z.object({items: z.array(z.object({id: z.uuidv7(), revision: z.string(), canonical_digest: z.string(), created_at: z.number().int()})).max(25), next_cursor: z.string().nullable()});
export type LibraryCard = z.infer<typeof LibraryCardSchema>;
export type Library = z.infer<typeof LibrarySchema>;

export function libraryPage(workspace: string, owned: boolean, cursor: string | null, signal: AbortSignal) {
  const query = new URLSearchParams({limit: "25"});
  if (cursor) query.set("starting_after", cursor);
  return setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleet-libraries${owned ? "/owned" : ""}?${query}`}, LibraryCardsSchema, signal);
}

export async function inspectLibrary(workspace: string, card: LibraryCard, signal: AbortSignal, revision = card.head_revision_id) {
  const query = new URLSearchParams({revision_id: revision});
  const entry = await setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleet-libraries/${encodeURIComponent(card.id)}?${query}`}, LibrarySchema, signal);
  if (entry.card.id !== card.id || entry.head.library_id !== card.id || entry.head.id !== revision) throw new SetupUnavailable("The selected Library revision could not be confirmed.", 409);
  return entry;
}

export function libraryHistory(workspace: string, library: string, cursor: string | null, signal: AbortSignal) {
  const query = new URLSearchParams({limit: "25"});
  if (cursor) query.set("starting_after", cursor);
  return setupRequest({path: `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleet-libraries/${encodeURIComponent(library)}/revisions?${query}`}, RevisionsSchema, signal);
}
