import type { ActionResult } from "@/lib/actions/with-token";
import { z } from "zod";

export const profileNamesSchema = z.object({firstName: z.string().trim().max(256), lastName: z.string().trim().max(256)});
export type ProfileNames = z.infer<typeof profileNamesSchema>;
export type AccountProfile = ProfileNames & {subject: string; email: string | null; imageUrl: string | null};
export const accountSessionSchema = z.object({
  id: z.string().min(1).max(256), browser: z.string(), device: z.string(), location: z.string().nullable(),
  mobile: z.boolean(), current: z.boolean(), lastActiveAt: z.iso.datetime().nullable(),
});
export const accountSessionPageSchema = z.object({sessions: z.array(accountSessionSchema).max(50), nextOffset: z.number().int().nonnegative().nullable()});
export type AccountSession = z.infer<typeof accountSessionSchema>;
export type AccountSessionPage = z.infer<typeof accountSessionPageSchema>;
export type AccountSettingsClient = {
  updateProfile: (names: ProfileNames) => Promise<ActionResult<null>>;
  listSessions: (offset?: number) => Promise<ActionResult<AccountSessionPage>>;
  revokeSession: (id: string) => Promise<ActionResult<{id: string}>>;
  manageSignIn: () => void;
};
export type AccountAccess = {status: "loading" | "signed-out"} | {status: "ready"; contextKey: string; profile: AccountProfile; client: AccountSettingsClient};
