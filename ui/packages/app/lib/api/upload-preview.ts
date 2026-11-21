import { z } from "zod";

export const PREVIEW_FILE_COUNT = 5;
export const UPLOAD_PREVIEW_LABEL = "Upload preview · Not saved to fleet history";
export const UPLOAD_PREVIEW_REPLY = "Upload preview accepted. The file was not read, stored, or analyzed.";

export const uploadReceiptSchema = z.object({
  preview: z.literal(true), stored: z.literal(false), reply: z.string(),
});

export function uploadPreviewUrl(workspace: string, fleet: string): string {
  return `/live/v1/workspaces/${encodeURIComponent(workspace)}/fleets/${encodeURIComponent(fleet)}/uploads`;
}
