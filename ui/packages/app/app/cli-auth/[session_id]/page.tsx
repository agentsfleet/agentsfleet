"use client";

import { use, useCallback, useEffect, useState } from "react";
import { useAuth } from "@clerk/nextjs";
import { RouteLoading } from "@/components/layout/RouteLoading";
import {
  deriveSharedKey,
  encryptJwt,
  generateEphemeralKeypair,
  generateVerificationCode,
} from "@/lib/auth/cli-flow";
import { APPROVE, LOAD, SESSION_STATUS, AuthNotice, ManualApprovalView,
  type ActiveSession, type ApproveState, type LoadState } from "./cli-auth-ui";
import { LOOPBACK_KIND, type BrowserHandoff, type LoopbackHandoff } from "@/lib/auth/cli-loopback";
import { readBrowserHandoff } from "@/lib/auth/cli-loopback-store";
import { CliLoopbackApproval, type ApprovalRequest, type ApprovalResult } from "./cli-loopback-approval";

const JSON_CONTENT_TYPE = "application/json";
const REQUEST_TIMEOUT_MS = 15_000;

type UpdateLoad = (state: LoadState) => void;
type UpdateApproval = (state: ApproveState) => void;
type GetToken = ReturnType<typeof useAuth>["getToken"];

function ManualCliAuthPage({ session_id }: { session_id: string }) {
  const { isLoaded, isSignedIn, getToken } = useAuth();
  const [load, setLoad] = useState<LoadState>({ kind: LOAD.loading });
  const [approve, setApprove] = useState<ApproveState>({ kind: APPROVE.idle });
  useEffect(() => { void loadManualSession(session_id, setLoad, setApprove); }, [session_id]);
  const onApprove = useCallback((session: ActiveSession) => {
    void approveManual(session_id, session, getToken, setApprove);
  }, [getToken, session_id]);
  return <ManualApprovalView isLoaded={isLoaded} isSignedIn={isSignedIn} load={load} approve={approve}
    onApprove={onApprove} />;
}

function activeSession(body: unknown): body is ActiveSession {
  if (typeof body !== "object" || body === null) return false;
  return "status" in body && (body.status === SESSION_STATUS.pending || body.status === SESSION_STATUS.verification_pending) &&
    "cli_public_key" in body && typeof body.cli_public_key === "string" &&
    "token_name" in body && typeof body.token_name === "string" &&
    "expires_at" in body && typeof body.expires_at === "string";
}

async function loadManualSession(sessionId: string, update: UpdateLoad, updateApproval: UpdateApproval): Promise<void> {
  try {
    const response = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(sessionId)}`, {
      method: "GET", headers: { Accept: JSON_CONTENT_TYPE },
    });
    if (response.ok) {
      const body: unknown = await response.json();
      if (activeSession(body)) {
        update({ kind: LOAD.active, session: body });
        if (body.status === SESSION_STATUS.verification_pending) {
          updateApproval({ kind: APPROVE.failed, message: "This session has already been approved on another tab." });
        }
      } else update({ kind: LOAD.error, message: "Something went wrong loading this login session — start over from your terminal." });
      return;
    }
    if (response.status === 404) {
      update({ kind: LOAD.terminal, message: "This login session is not recognized — start over from your terminal." });
    } else if ([410, 409, 400].includes(response.status)) {
      update({ kind: LOAD.terminal, message: "This login session is no longer accepting approval." });
    } else update({ kind: LOAD.error, message: "Couldn't load this login session. Try again, or start over from your terminal." });
  } catch { update({ kind: LOAD.error, message: "Network error loading the login session." }); }
}

function showManualApproval(response: Response, verificationCode: string, update: UpdateApproval): void {
  if (response.redirected) {
    update({ kind: APPROVE.failed, message: "Your dashboard session expired. Refresh and try again." });
  } else if (response.ok) {
    update({ kind: APPROVE.approved, verificationCode });
  } else {
    const message = response.status === 409 ? "This session is already approved — check your terminal."
      : response.status === 410 ? "This session is no longer accepting approval."
      : response.status === 401 ? "Your dashboard session is not authorized for this action."
      : "Couldn't approve this login. Try again, or start over from your terminal.";
    update({ kind: APPROVE.failed, message });
  }
}

async function approveManual(sessionId: string, session: ActiveSession, getToken: GetToken,
  update: UpdateApproval): Promise<void> {
  update({ kind: APPROVE.working });
  // A lost approval response can leave the server waiting for this code.
  let pendingCode: string | null = null;
  try {
    const sessionJwt = await getToken();
    const jwt = sessionJwt ? await getToken({ template: "api" }) : null;
    if (!sessionJwt || !jwt) {
      update({ kind: APPROVE.failed, message: "Your dashboard session expired. Refresh and try again." });
      return;
    }
    const dash = await generateEphemeralKeypair();
    const key = await deriveSharedKey(dash.privateKey, session.cli_public_key);
    const encrypted = await encryptJwt(jwt, key);
    pendingCode = generateVerificationCode();
    const response = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(sessionId)}/approve`, {
      method: "PATCH", headers: { "Content-Type": JSON_CONTENT_TYPE, Authorization: `Bearer ${sessionJwt}` },
      body: JSON.stringify({ dashboard_public_key: dash.publicKeyBase64Url, ...encrypted, verification_code: pendingCode }),
    });
    showManualApproval(response, pendingCode, update);
  } catch {
    update(pendingCode ? { kind: APPROVE.uncertain, verificationCode: pendingCode }
      : { kind: APPROVE.failed, message: "Something went wrong while approving the CLI login." });
  }
}

export default function CliAuthPage({ params }: { params: Promise<{ session_id: string }> }) {
  const { session_id } = use(params);
  const { getToken } = useAuth();
  const [handoff, setHandoff] = useState<BrowserHandoff | null>(null);
  useEffect(() => setHandoff(readBrowserHandoff(session_id)), [session_id]);
  if (!handoff) return <RouteLoading fullScreen />;
  if (handoff.kind === LOOPBACK_KIND.manual) return <ManualCliAuthPage session_id={session_id} />;
  if (handoff.kind === LOOPBACK_KIND.automatic) return <CliLoopbackApproval key={handoff.handoff.sessionId}
    handoff={handoff.handoff} approve={request => approveLocal(handoff.handoff, request, getToken)} />;
  return <AuthNotice title="Sign-in unavailable" message={handoff.message} />;
}

function tokenMatches(token: string, userId: string, sessionId?: string): boolean {
  try {
    const parts = token.split(".");
    if (parts.length !== 3 || !parts[1]) return false;
    const claims: unknown = JSON.parse(atob(parts[1].replaceAll("-", "+").replaceAll("_", "/")));
    return typeof claims === "object" && claims !== null && "sub" in claims && claims.sub === userId &&
      (sessionId === undefined || ("sid" in claims && claims.sid === sessionId));
  } catch { return false; }
}

async function approveLocal(handoff: LoopbackHandoff, request: ApprovalRequest,
  getToken: ReturnType<typeof useAuth>["getToken"]): Promise<ApprovalResult> {
  const { userId, sessionId, current, signal } = request;
  const accountFailure: ApprovalResult = { ok: false, message: "Your signed-in account changed. Start a new sign-in from your terminal." };
  let sent = false;
  try {
    if (!current()) return accountFailure;
    const sessionToken = await getToken();
    if (!sessionToken || !current() || !tokenMatches(sessionToken, userId, sessionId)) return accountFailure;
    const token = await getToken({ template: "api" });
    if (!token || !current() || !tokenMatches(token, userId)) return accountFailure;
    const dashboard = await generateEphemeralKeypair();
    const key = await deriveSharedKey(dashboard.privateKey, handoff.publicKey);
    const encrypted = await encryptJwt(token, key);
    const verificationCode = generateVerificationCode();
    if (!current()) return accountFailure;
    sent = true;
    const response = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(handoff.sessionId)}/approve`, {
      method: "PATCH", headers: { "Content-Type": JSON_CONTENT_TYPE, Authorization: `Bearer ${sessionToken}` },
      body: JSON.stringify({ dashboard_public_key: dashboard.publicKeyBase64Url, ...encrypted, verification_code: verificationCode }),
      signal: AbortSignal.any([signal, AbortSignal.timeout(REQUEST_TIMEOUT_MS)]), redirect: "error",
    });
    if (response.ok && !response.redirected && !signal.aborted) return { ok: true, verificationCode };
  } catch {
    if (sent) return { ok: false, message: "Approval could not be confirmed. Start a new sign-in from your terminal." };
  }
  return { ok: false, message: "Start a new sign-in from your terminal. No code was sent to it." };
}
