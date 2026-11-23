"use client";

import { useEffect, useRef, useState } from "react";
import { useAuth, useClerk } from "@clerk/nextjs";
import {
  Alert, AlertDescription, AlertTitle, Button, Card, CardContent,
  CardDescription, CardFooter, CardHeader, PageTitle, Spinner,
} from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { matchingCode, type LoopbackHandoff } from "@/lib/auth/cli-loopback";
import { clearBrowserHandoff } from "@/lib/auth/cli-loopback-store";
import { CLI_SIGN_IN_TITLE, PageShell, VerificationCodeDisplay } from "./cli-auth-ui";

const JSON_CONTENT_TYPE = "application/json";
const FORM_CONTENT_TYPE = "application/x-www-form-urlencoded";
const RESTART_MESSAGE = "Start a new sign-in from your terminal. No code was sent to it.";
const VIEW = { loading: "loading", ready: "ready", working: "working", failed: "failed" } as const;
const DECISION = { approved: "approved", denied: "denied" } as const;
type Decision = (typeof DECISION)[keyof typeof DECISION];
type View = { kind: typeof VIEW.loading }
  | { kind: typeof VIEW.working; decision: Decision; code: string }
  | { kind: typeof VIEW.ready; code: string }
  | { kind: typeof VIEW.failed; message: string };

async function readSession(response: Response): Promise<unknown> {
  if (!response.ok || response.redirected || !response.body) throw new Error(RESTART_MESSAGE);
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      text += decoder.decode(part.value, { stream: true });
    }
    return JSON.parse(text + decoder.decode()) as unknown;
  } finally {
    try { await reader.cancel(); } finally { reader.releaseLock(); }
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

async function loadComparison(handoff: LoopbackHandoff, signal: AbortSignal): Promise<string> {
  const response = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(handoff.sessionId)}`, {
    headers: { Accept: JSON_CONTENT_TYPE }, signal,
  });
  const body = await readSession(response);
  if (!isRecord(body) || body.status !== "pending" || body.cli_public_key !== handoff.publicKey ||
    typeof body.expires_at !== "string" || !Number.isFinite(Date.parse(body.expires_at)) ||
    Date.parse(body.expires_at) <= Date.now()) throw new Error(RESTART_MESSAGE);
  return matchingCode(handoff);
}

function submitDecision(handoff: LoopbackHandoff, status: Decision, code?: string): void {
  const form = document.createElement("form");
  form.method = "POST";
  form.action = handoff.callback;
  form.enctype = FORM_CONTENT_TYPE;
  form.target = "_self";
  form.className = "ph-no-capture ph-no-record";
  const fields: Record<string, string> = { state: handoff.state, session_id: handoff.sessionId, status };
  if (code !== undefined) fields.verification_code = code;
  for (const [name, value] of Object.entries(fields)) {
    const input = document.createElement("input");
    input.type = "hidden";
    input.name = name;
    input.value = value;
    form.append(input);
  }
  clearBrowserHandoff(handoff.sessionId);
  document.body.append(form);
  try { form.submit(); } finally { form.remove(); }
}

interface Identity { userId: string; sessionId: string }
export interface ApprovalRequest extends Identity { current: () => boolean; signal: AbortSignal }
export type ApprovalResult = { ok: true; verificationCode: string } | { ok: false; message: string };
type ApproveLogin = (request: ApprovalRequest) => Promise<ApprovalResult>;
interface ApprovalPort {
  approve: ApproveLogin;
  displayed: Identity;
  current(): Identity | null;
  signal: AbortSignal;
  update(view: View): void;
}

class ApprovalDecision {
  readonly #handoff: LoopbackHandoff;
  readonly #port: ApprovalPort;
  #chosen = false;
  constructor(handoff: LoopbackHandoff, port: ApprovalPort) { this.#handoff = handoff; this.#port = port; }

  decide(decision: Decision, code: string): void {
    if (this.#chosen || this.#port.signal.aborted) return;
    this.#chosen = true;
    this.#port.update({ kind: VIEW.working, decision, code });
    if (decision === DECISION.approved) { void this.#authorize(); return; }
    try { submitDecision(this.#handoff, DECISION.denied); } catch { this.#fail(RESTART_MESSAGE); }
  }

  #fail(message: string): void {
    clearBrowserHandoff(this.#handoff.sessionId);
    if (!this.#port.signal.aborted) this.#port.update({ kind: VIEW.failed, message });
  }

  #sameAccount(): boolean {
    const current = this.#port.current();
    return current?.userId === this.#port.displayed.userId &&
      current.sessionId === this.#port.displayed.sessionId && !this.#port.signal.aborted;
  }

  async #authorize(): Promise<void> {
    try {
      const result = await this.#port.approve({
        ...this.#port.displayed, current: () => this.#sameAccount(), signal: this.#port.signal,
      });
      if (!result.ok) { this.#fail(result.message); return; }
      if (!this.#port.signal.aborted) submitDecision(this.#handoff, DECISION.approved, result.verificationCode);
    } catch { this.#fail(RESTART_MESSAGE); }
  }
}

function useComparison(handoff: LoopbackHandoff) {
  const [view, setView] = useState<View>({ kind: VIEW.loading });
  const lifetime = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController();
    lifetime.current = controller;
    void loadComparison(handoff, controller.signal).then(
      code => { if (!controller.signal.aborted) setView({ kind: VIEW.ready, code }); },
      () => {
        if (!controller.signal.aborted) {
          clearBrowserHandoff(handoff.sessionId);
          setView({ kind: VIEW.failed, message: RESTART_MESSAGE });
        }
      },
    );
    return () => controller.abort();
  }, [handoff]);
  return { view, setView, lifetime };
}

export function CliLoopbackApproval({ handoff, approve }: { handoff: LoopbackHandoff; approve: ApproveLogin }) {
  const { isLoaded, isSignedIn } = useAuth();
  const clerk = useClerk();
  const { view, setView, lifetime } = useComparison(handoff);
  const actions = useRef<ApprovalDecision | null>(null);
  const displayed = clerk.user && clerk.session ? { userId: clerk.user.id, sessionId: clerk.session.id } : null;
  const identity = clerk.user?.primaryEmailAddress?.emailAddress ?? clerk.user?.fullName;
  const ready = Boolean(isSignedIn && view.kind === VIEW.ready && identity);
  const decide = (decision: Decision): void => {
    if (!ready || view.kind !== VIEW.ready || !displayed || !lifetime.current) return;
    actions.current ??= new ApprovalDecision(handoff, {
      approve, displayed, signal: lifetime.current.signal, update: setView,
      current: () => clerk.user && clerk.session ? { userId: clerk.user.id, sessionId: clerk.session.id } : null,
    });
    actions.current.decide(decision, view.code);
  };
  if (!isLoaded || view.kind === VIEW.loading) return <RouteLoading fullScreen />;
  return <ApprovalCard view={view} identity={identity} ready={ready} decide={decision => decide(decision)} />;
}

function ApprovalCard({ view, identity, ready, decide }: {
  view: View; identity: string | null | undefined; ready: boolean; decide: (decision: Decision) => void;
}) {
  return <PageShell><Card>
    <CardHeader>
      <PageTitle>{CLI_SIGN_IN_TITLE}</PageTitle>
      <CardDescription>Continue only if you started this sign-in on this computer.</CardDescription>
    </CardHeader>
    <CardContent className="space-y-md">
      {identity ? <p className="text-label text-foreground">Signing in as {identity}</p> : null}
      {view.kind === VIEW.ready || view.kind === VIEW.working ? <>
        <p className="text-label text-muted-foreground">Confirm this code matches the one in your terminal.</p>
        <VerificationCodeDisplay code={view.code} />
      </> : null}
      {view.kind === VIEW.failed || !identity ? <Alert variant="destructive">
        <AlertTitle>Sign-in unavailable</AlertTitle>
        <AlertDescription>{view.kind === VIEW.failed ? view.message : RESTART_MESSAGE}</AlertDescription>
      </Alert> : null}
    </CardContent>
    <CardFooter className="gap-sm">
      <Button disabled={!ready} onClick={() => decide(DECISION.approved)}>{view.kind === VIEW.working && view.decision === DECISION.approved ? <Spinner size="sm" label="Authorizing…" /> : "Authorize"}</Button>
      <Button variant="outline" disabled={!ready} onClick={() => decide(DECISION.denied)}>{view.kind === VIEW.working && view.decision === DECISION.denied ? <Spinner size="sm" label="Denying…" /> : "Deny"}</Button>
    </CardFooter>
  </Card></PageShell>;
}
