// Server-Sent Events proxy. EventSource cannot set headers, so the browser
// hits this same-origin Route Handler instead of the upstream API directly.
// We resolve the user's Clerk session, mint an API-audience JWT, and pipe
// the upstream stream body straight back to the client.
//
// See docs/AUTH.md "UI · SSE stream" for the full sequence.

import { auth } from "@clerk/nextjs/server";
import { API_ORIGIN } from "@/lib/api/client";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

type Params = {
  params: Promise<{ workspaceId: string; zombieId: string }>;
};

export async function GET(req: Request, { params }: Params) {
  const { workspaceId, zombieId } = await params;

  const { getToken } = await auth();
  // The Zig backend enforces aud=https://api.usezombie.com; mint an
  // API-audience JWT via the Clerk "api" template per docs/AUTH.md §"UI ·
  // SSE stream". Bare getToken() returns the default-aud session JWT and
  // 401s upstream.
  const token = await getToken({ template: "api" });
  if (!token) {
    return new Response(JSON.stringify({ error: "Unauthorized", code: "UZ-401" }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  const upstreamUrl =
    `${API_ORIGIN}/v1/workspaces/${encodeURIComponent(workspaceId)}` +
    `/zombies/${encodeURIComponent(zombieId)}/events/stream`;

  const upstream = await fetch(upstreamUrl, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      Accept: "text/event-stream",
    },
    signal: req.signal,
  });

  if (!upstream.ok) {
    const text = await upstream.text().catch(() => "");
    return new Response(text || `Upstream error ${upstream.status}`, {
      status: upstream.status,
      headers: {
        "Content-Type": upstream.headers.get("content-type") ?? "text/plain",
      },
    });
  }
  if (!upstream.body) {
    return new Response("Upstream returned no body", {
      status: 502,
      headers: { "Content-Type": "text/plain" },
    });
  }

  return new Response(upstream.body, {
    status: 200,
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
      // Defend against intermediary buffering (nginx, etc.) that would
      // bunch frames and defeat the live-tail UX.
      "X-Accel-Buffering": "no",
    },
  });
}
