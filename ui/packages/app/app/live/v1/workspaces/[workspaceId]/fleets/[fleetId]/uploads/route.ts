import { z } from "zod";
import { v7 } from "uuid";
import { credential } from "@/lib/auth/credential";
import { getFleet } from "@/lib/api/fleets";
import { ApiError } from "@/lib/api/errors";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { sameOrigin, boundedBody } from "@/lib/api/setup-proxy/body";
import { PREVIEW_BODY_LIMIT, PREVIEW_FILE_LIMIT, PREVIEW_UPLOAD_TIMEOUT_MS, uploadReceiptSchema } from "@/lib/api/upload-preview";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {params: Promise<{workspaceId: string; fleetId: string}>};
const scopeSchema = z.object({workspaceId: z.uuidv7(), fleetId: z.uuidv7()});
const filenameSchema = z.string().min(1).max(255);
const HEADERS = {"Cache-Control": "no-store"};

/** Preview only: authorize the fleet, bound and discard the bytes, return text. */
export async function POST(request: Request, {params}: Params): Promise<Response> {
  if (!sameOrigin(request)) return refusal(403, "Upload previews require the current app origin.");
  const scope = scopeSchema.safeParse(await params);
  if (!scope.success) return refusal(400, "Invalid fleet or workspace.");
  const token = await credential();
  if (!token) return refusal(401, "Sign in before uploading a preview.");
  try {
    const {fleet} = await getFleet(scope.data.workspaceId, scope.data.fleetId, token);
    if (fleet.status === AGENTSFLEET_STATUS.KILLED) return refusal(409, "Archived fleets are read-only.");
  } catch (error) {
    return refusal(error instanceof ApiError && [401, 403, 404].includes(error.status) ? error.status : 503,
      "This fleet could not be authorized for an upload preview.");
  }
  const mediaType = request.headers.get("content-type");
  if (!mediaType?.toLowerCase().startsWith("multipart/form-data;")) return refusal(415, "Choose a file to upload.");
  const signal = AbortSignal.any([request.signal, AbortSignal.timeout(PREVIEW_UPLOAD_TIMEOUT_MS)]);
  try {
    const bytes = await boundedBody(request, signal, PREVIEW_BODY_LIMIT);
    if (signal.aborted) return refusal(408, "The upload preview was interrupted.");
    if (bytes === null) return refusal(413, "Upload previews support files up to 5 MiB.");
    const form = await new Response(bytes, {headers: {"content-type": mediaType}}).formData();
    const files = form.getAll("file");
    const file = files[0];
    if (files.length !== 1 || file === undefined || typeof file === "string" || !filenameSchema.safeParse(file.name).success)
      return refusal(400, "Choose one named file per upload.");
    if (file.size > PREVIEW_FILE_LIMIT) return refusal(413, "Upload previews support files up to 5 MiB.");
    const receipt = uploadReceiptSchema.parse({id: v7(), filename: file.name, bytes: file.size, preview: true, stored: false,
      reply: `Received ${file.name} (${file.size} bytes) as a preview upload. The file was not stored or analyzed.`});
    return Response.json(receipt, {status: 201, headers: HEADERS});
  } catch {
    return refusal(signal.aborted ? 408 : 400, "The upload preview could not be read.");
  }
}

function refusal(status: number, error: string): Response {
  return Response.json({error}, {status, headers: HEADERS});
}
