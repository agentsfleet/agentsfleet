import { z } from "zod";
import { Instant } from "./decode";
import { LibraryCardSchema } from "./draft-library";

export const PlatformEntrySchema = z.object({
  card: LibraryCardSchema.extend({visibility: z.enum(["platform", "platform_draft"])}),
  published: z.boolean(), updated_at: Instant,
}).transform(({card, published, updated_at}) => ({...card, published, updated_at}));
export const PlatformEntriesSchema = z.object({entries: z.array(PlatformEntrySchema)});
export type PlatformEntry = z.infer<typeof PlatformEntrySchema>;
export type PlatformPatch = {name?: string; description?: string; required_credentials_reasons?: Record<string, string>; published?: boolean};
export type StagePlatform = {workspace_id: string; draft_candidate_id: string; label: string};
