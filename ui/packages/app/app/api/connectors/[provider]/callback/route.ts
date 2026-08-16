import { auth } from "@clerk/nextjs/server";
import { requireApiOrigin } from "@/lib/api/client";

type Params = {
  params: Promise<{ provider: string }>;
};

const PROVIDER_ID_PATTERN = /^[a-z][a-z0-9_-]*$/;
const COMPLETE_PATH_PREFIX = "/v1/connectors/";
const COMPLETE_PATH_SUFFIX = "/callback";
const AUTHORIZATION_HEADER = "Authorization";
const BEARER_PREFIX = "Bearer ";
const CONTENT_TYPE_HEADER = "content-type";
const JSON_CONTENT_TYPE = "application/json";
const REDIRECT_STATUS = 302;
const BAD_GATEWAY_STATUS = 502;
const UNAUTHORIZED_STATUS = 401;
const INVALID_PROVIDER_STATUS = 400;

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

// The provider returns here in the signed-in browser. The route forwards only
// the browser's existing Bearer token and original query to the backend; the
// backend remains the sole authority for state, workspace, and persistence.
export async function GET(req: Request, { params }: Params): Promise<Response> {
  const { provider } = await params;
  if (!PROVIDER_ID_PATTERN.test(provider)) {
    return Response.json({ error: "Invalid connector provider" }, { status: INVALID_PROVIDER_STATUS });
  }

  const { getToken } = await auth();
  const token = await getToken();
  if (!token) {
    return Response.json({ error: "Not authenticated" }, { status: UNAUTHORIZED_STATUS });
  }

  const upstream = new URL(`${COMPLETE_PATH_PREFIX}${encodeURIComponent(provider)}${COMPLETE_PATH_SUFFIX}`, requireApiOrigin());
  upstream.search = new URL(req.url).search;
  const response = await fetch(upstream.toString(), {
    headers: { [AUTHORIZATION_HEADER]: `${BEARER_PREFIX}${token}` },
    method: "POST",
    redirect: "manual",
  });

  if (response.status !== REDIRECT_STATUS) {
    return new Response(response.body, {
      headers: { [CONTENT_TYPE_HEADER]: response.headers.get(CONTENT_TYPE_HEADER) ?? JSON_CONTENT_TYPE },
      status: response.status,
    });
  }

  const location = response.headers.get("location");
  if (!location || new URL(location, req.url).origin !== new URL(req.url).origin) {
    return Response.json({ error: "Connector completion returned an invalid redirect" }, { status: BAD_GATEWAY_STATUS });
  }
  return Response.redirect(location, REDIRECT_STATUS);
}
