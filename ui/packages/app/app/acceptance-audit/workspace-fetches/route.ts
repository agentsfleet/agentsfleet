import { NextResponse } from "next/server";
import {
  isWorkspaceFetchAuditEnabled,
  readWorkspaceFetchAudit,
  resetWorkspaceFetchAudit,
} from "@/lib/acceptance/workspace-fetch-audit";

const DISABLED_STATUS = 404;
const DISABLED_BODY = { error: "acceptance_audit_disabled" } as const;
const UNAUTHORIZED_STATUS = 401;
const UNAUTHORIZED_BODY = { error: "acceptance_audit_unauthorized" } as const;
const AUDIT_TOKEN_ENV_NAME = "AGENTSFLEET_E2E_AUDIT_TOKEN";
const AUDIT_TOKEN_HEADER = "x-acceptance-token";

function disabledResponse() {
  return NextResponse.json(DISABLED_BODY, { status: DISABLED_STATUS });
}

function unauthorizedResponse() {
  return NextResponse.json(UNAUTHORIZED_BODY, { status: UNAUTHORIZED_STATUS });
}

function isAuthorized(request: Request): boolean {
  const token = process.env[AUDIT_TOKEN_ENV_NAME];
  return Boolean(token) && request.headers.get(AUDIT_TOKEN_HEADER) === token;
}

function guardAuditRequest(request: Request): Response | null {
  if (!isWorkspaceFetchAuditEnabled()) return disabledResponse();
  if (!isAuthorized(request)) return unauthorizedResponse();
  return null;
}

export function GET(request: Request) {
  const guarded = guardAuditRequest(request);
  if (guarded) return guarded;
  return NextResponse.json(readWorkspaceFetchAudit());
}

export function POST(request: Request) {
  const guarded = guardAuditRequest(request);
  if (guarded) return guarded;
  return NextResponse.json(resetWorkspaceFetchAudit());
}
