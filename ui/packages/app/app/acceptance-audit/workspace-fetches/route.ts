import { NextResponse } from "next/server";
import {
  isWorkspaceFetchAuditEnabled,
  readWorkspaceFetchAudit,
  resetWorkspaceFetchAudit,
} from "@/lib/acceptance/workspace-fetch-audit";

const DISABLED_STATUS = 404;
const DISABLED_BODY = { error: "acceptance_audit_disabled" } as const;

function disabledResponse() {
  return NextResponse.json(DISABLED_BODY, { status: DISABLED_STATUS });
}

export function GET() {
  if (!isWorkspaceFetchAuditEnabled()) return disabledResponse();
  return NextResponse.json(readWorkspaceFetchAudit());
}

export function POST() {
  if (!isWorkspaceFetchAuditEnabled()) return disabledResponse();
  return NextResponse.json(resetWorkspaceFetchAudit());
}
