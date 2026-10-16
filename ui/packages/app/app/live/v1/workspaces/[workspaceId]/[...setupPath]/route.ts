// Kept outside /backend so the rewrite cannot bypass server authentication.

import { proxySetup } from "@/lib/api/setup-proxy";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";
export const GET = proxySetup;
export const POST = proxySetup;
export const PATCH = proxySetup;
export const DELETE = proxySetup;
