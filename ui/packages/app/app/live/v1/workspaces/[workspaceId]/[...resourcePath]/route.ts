// Kept outside /backend so the rewrite cannot bypass server authentication.

import { proxyWorkspace } from "@/lib/api/workspace-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";
export const GET = proxyWorkspace;
export const POST = proxyWorkspace;
export const PATCH = proxyWorkspace;
export const DELETE = proxyWorkspace;
