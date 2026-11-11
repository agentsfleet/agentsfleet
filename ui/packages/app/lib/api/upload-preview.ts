import { z } from "zod";

export const PREVIEW_FILE_LIMIT = 5 * 1024 * 1024;
export const PREVIEW_BODY_LIMIT = PREVIEW_FILE_LIMIT + 64 * 1024;
export const PREVIEW_FILE_COUNT = 5;
export const PREVIEW_RECEIPT_LIMIT = 8 * 1024;
export const PREVIEW_UPLOAD_TIMEOUT_MS = 15_000;
export const UPLOAD_PREVIEW_LABEL = "Upload preview · Not saved to fleet history";

export const uploadReceiptSchema = z.object({
  id: z.uuidv7(), filename: z.string().min(1).max(255), bytes: z.number().int().min(0).max(PREVIEW_FILE_LIMIT),
  preview: z.literal(true), stored: z.literal(false), reply: z.string().max(1000),
});
export type UploadPreviewReceipt = z.infer<typeof uploadReceiptSchema>;

export function uploadPreviewUrl(workspace: string, fleet: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets/${encodeURIComponent(fleet)}/uploads`;
}
