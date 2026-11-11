import { z } from "zod";
import { credential } from "@/lib/auth/credential";
import { getFleet } from "@/lib/api/fleets";
import { ApiError } from "@/lib/api/errors";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { sameOrigin } from "@/lib/api/setup-proxy/body";
import { UPLOAD_PREVIEW_REPLY } from "@/lib/api/upload-preview";

export const runtime = "nodejs";

type Params = {params: Promise<{workspaceId: string; fleetId: string}>};
const scopeSchema = z.object({workspaceId: z.uuidv7(), fleetId: z.uuidv7()});
const HEADERS = {"Cache-Control": "no-store"};

/** Temporary acknowledgement only. Replace with forwarding when the backend is defined. */
export async function POST(request: Request, {params}: Params): Promise<Response> {
  if (!sameOrigin(request)) return refusal(403, "Upload previews require the current app origin.");
  const scope = scopeSchema.safeParse(await params);
  if (!scope.success) return refusal(400, "Invalid fleet or workspace.");
  const token = await credential();
  if (!token) return refusal(401, "Sign in before uploading a preview.");
  try {
    const {fleet} = await getFleet(scope.data.workspaceId, scope.data.fleetId, token);
    if (fleet.status === AGENTSFLEET_STATUS.KILLED) return refusal(409, "Archived fleets are read-only.");
    return Response.json({preview: true, stored: false, reply: UPLOAD_PREVIEW_REPLY}, {headers: HEADERS});
  } catch (error) {
    return refusal(error instanceof ApiError && [401, 403, 404].includes(error.status) ? error.status : 503,
      "This fleet could not be authorized for an upload preview.");
  }
}

function refusal(status: number, error: string): Response {
  return Response.json({error}, {status, headers: HEADERS});
}
