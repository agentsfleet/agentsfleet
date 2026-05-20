"use client";

import { use, useCallback, useEffect, useState } from "react";
import { useAuth } from "@clerk/nextjs";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@usezombie/design-system";
import {
  deriveSharedKey,
  encryptJwt,
  generateEphemeralKeypair,
  generateVerificationCode,
} from "@/lib/auth/cli-flow";

// Discriminant + status tags are named per RULE UFS (no bare repeated string
// literals); the `as const` objects keep them usable as discriminated-union
// literal types so narrowing still works.
const SESSION_STATUS = {
  pending: "pending",
  verification_pending: "verification_pending",
} as const;
type SessionStatus = (typeof SESSION_STATUS)[keyof typeof SESSION_STATUS];

interface ActiveSession {
  status: SessionStatus;
  cli_public_key: string;
  token_name: string;
  expires_at_ms: number;
}

const LOAD = {
  loading: "loading",
  active: "active",
  terminal: "terminal",
  error: "error",
} as const;

type LoadState =
  | { kind: typeof LOAD.loading }
  | { kind: typeof LOAD.active; session: ActiveSession }
  | { kind: typeof LOAD.terminal; message: string }
  | { kind: typeof LOAD.error; message: string };

const APPROVE = {
  idle: "idle",
  working: "working",
  approved: "approved",
  uncertain: "uncertain",
  failed: "failed",
} as const;

type ApproveState =
  | { kind: typeof APPROVE.idle }
  | { kind: typeof APPROVE.working }
  | { kind: typeof APPROVE.approved; verificationCode: string }
  // PATCH threw before a response arrived: the approve may have landed
  // server-side, so we surface the code with a caveat rather than stranding
  // a user whose session is now waiting on exactly that code.
  | { kind: typeof APPROVE.uncertain; verificationCode: string }
  | { kind: typeof APPROVE.failed; message: string };

const TOKEN_NAME_MAX_LEN = 64;
const JSON_CONTENT_TYPE = "application/json";

export default function CliAuthPage({
  params,
}: {
  params: Promise<{ session_id: string }>;
}) {
  const { session_id } = use(params);
  const { isLoaded, isSignedIn, getToken } = useAuth();

  const [load, setLoad] = useState<LoadState>({ kind: LOAD.loading });
  const [approve, setApprove] = useState<ApproveState>({ kind: APPROVE.idle });

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`/backend/v1/auth/sessions/${encodeURIComponent(session_id)}`, {
          method: "GET",
          headers: { Accept: JSON_CONTENT_TYPE },
        });
        if (cancelled) return;
        if (res.ok) {
          const body = (await res.json()) as Partial<ActiveSession>;
          if (
            (body.status === SESSION_STATUS.pending || body.status === SESSION_STATUS.verification_pending) &&
            typeof body.cli_public_key === "string" &&
            typeof body.token_name === "string" &&
            typeof body.expires_at_ms === "number"
          ) {
            setLoad({
              kind: LOAD.active,
              session: {
                status: body.status,
                cli_public_key: body.cli_public_key,
                token_name: body.token_name,
                expires_at_ms: body.expires_at_ms,
              },
            });
            if (body.status === SESSION_STATUS.verification_pending) {
              setApprove({ kind: APPROVE.failed, message: "This session has already been approved on another tab." });
            }
            return;
          }
          setLoad({ kind: LOAD.error, message: "Unexpected session payload." });
          return;
        }
        if (res.status === 404) {
          setLoad({ kind: LOAD.terminal, message: "This login session is not recognized — start over from your terminal." });
          return;
        }
        if (res.status === 410 || res.status === 409 || res.status === 400) {
          setLoad({ kind: LOAD.terminal, message: "This login session is no longer accepting approval." });
          return;
        }
        setLoad({ kind: LOAD.error, message: `Could not load the login session (HTTP ${res.status}).` });
      } catch {
        if (!cancelled) setLoad({ kind: LOAD.error, message: "Network error loading the login session." });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [session_id]);

  const onApprove = useCallback(async () => {
    if (load.kind !== LOAD.active) return;
    setApprove({ kind: APPROVE.working });
    // Hoisted out of the try so the catch can still read it: once the PATCH is
    // sent, the server may approve the session against this code even if the
    // response never makes it back to us.
    let pendingCode: string | null = null;
    try {
      // ───────── CLI carve-out (I9.1) ─────────
      // This is the ONE surviving `getToken({ template: "api" })` call in
      // the dashboard post-Stage-1. The rest of the dashboard now uses the
      // customized default session token (`auth().getToken()` with no
      // template arg). WHY this site keeps the api-template mint:
      //   • The minted JWT is encrypted with the CLI's ephemeral ECDH
      //     pubkey and persisted in `~/.usezombie/credentials.json` for
      //     ~15 minutes. The CLI has no Clerk SDK and cannot refresh.
      //   • Default session tokens are ~60s lived and refresh-coupled to
      //     the browser session via Clerk's cookie. That refresh
      //     mechanism doesn't exist on the CLI side.
      //   • The api template lets us mint a longer-lived (currently 60s
      //     but template-configurable independently of session tokens)
      //     token that the CLI can actually use.
      // Invariant I9.1 (grep-gate test) verifies this is the ONLY site
      // outside `/cli-auth/[session_id]/page.tsx` calling the api template.
      const jwt = await getToken({ template: "api" });
      if (!jwt) {
        setApprove({ kind: APPROVE.failed, message: "Your dashboard session expired. Refresh and try again." });
        return;
      }
      const dash = await generateEphemeralKeypair();
      const key = await deriveSharedKey(dash.privateKey, load.session.cli_public_key);
      const { ciphertext, nonce } = await encryptJwt(jwt, key);
      pendingCode = generateVerificationCode();

      const res = await fetch(
        `/backend/v1/auth/sessions/${encodeURIComponent(session_id)}/approve`,
        {
          method: "PATCH",
          headers: {
            "Content-Type": JSON_CONTENT_TYPE,
            Authorization: `Bearer ${jwt}`,
          },
          body: JSON.stringify({
            dashboard_public_key: dash.publicKeyBase64Url,
            ciphertext,
            nonce,
            verification_code: pendingCode,
          }),
        },
      );

      if (res.ok) {
        setApprove({ kind: APPROVE.approved, verificationCode: pendingCode });
        return;
      }
      if (res.status === 409) {
        setApprove({ kind: APPROVE.failed, message: "This session is already approved — check your terminal." });
        return;
      }
      if (res.status === 410) {
        setApprove({ kind: APPROVE.failed, message: "This session is no longer accepting approval." });
        return;
      }
      if (res.status === 401) {
        setApprove({ kind: APPROVE.failed, message: "Your dashboard session is not authorized for this action." });
        return;
      }
      setApprove({ kind: APPROVE.failed, message: `Approval failed (HTTP ${res.status}).` });
    } catch {
      // A thrown fetch means no response arrived — but the PATCH may have
      // reached the server and approved against pendingCode. If we got far
      // enough to generate it, show the code (with a caveat) instead of a
      // dead-end error; otherwise the failure was before any approve attempt.
      setApprove(
        pendingCode
          ? { kind: APPROVE.uncertain, verificationCode: pendingCode }
          : { kind: APPROVE.failed, message: "Something went wrong while approving the CLI login." },
      );
    }
  }, [getToken, load, session_id]);

  if (!isLoaded || load.kind === LOAD.loading) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle>Approve CLI login</CardTitle>
            <CardDescription>Checking your terminal&apos;s login session…</CardDescription>
          </CardHeader>
          <CardContent>
            <Skeleton className="h-6 w-full" />
          </CardContent>
        </Card>
      </PageShell>
    );
  }

  if (!isSignedIn) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle>Sign in to continue</CardTitle>
            <CardDescription>You need to be signed in to approve a CLI login.</CardDescription>
          </CardHeader>
        </Card>
      </PageShell>
    );
  }

  if (load.kind === LOAD.terminal || load.kind === LOAD.error) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle>Login session unavailable</CardTitle>
            <CardDescription>{load.message}</CardDescription>
          </CardHeader>
        </Card>
      </PageShell>
    );
  }

  const tokenLabel = sanitizeTokenName(load.session.token_name);

  if (approve.kind === APPROVE.approved || approve.kind === APPROVE.uncertain) {
    return (
      <PageShell>
        <Card>
          <CardHeader>
            <CardTitle>Type this code into your CLI</CardTitle>
            <CardDescription>
              {approve.kind === APPROVE.uncertain
                ? "We couldn't confirm the approval over the network. If your terminal is asking for a code, enter this one; otherwise refresh and try again."
                : "The terminal that started this login is waiting for a 6-digit verification code."}
            </CardDescription>
          </CardHeader>
          <CardContent>
            <VerificationCodeDisplay code={approve.verificationCode} />
          </CardContent>
          <CardFooter>
            <CopyButton value={approve.verificationCode} />
          </CardFooter>
        </Card>
      </PageShell>
    );
  }

  return (
    <PageShell>
      <Card>
        <CardHeader>
          <CardTitle>Approve CLI login for {tokenLabel}</CardTitle>
          <CardDescription>
            Approving will issue a short-lived API token to your terminal. Only continue if you
            started this login.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {approve.kind === APPROVE.failed ? (
            <Alert variant="destructive">
              <AlertTitle>Could not approve</AlertTitle>
              <AlertDescription>{approve.message}</AlertDescription>
            </Alert>
          ) : null}
        </CardContent>
        <CardFooter className="gap-2">
          <Button
            onClick={() => void onApprove()}
            disabled={
              approve.kind === APPROVE.working ||
              approve.kind === APPROVE.failed ||
              load.session.status === SESSION_STATUS.verification_pending
            }
          >
            {approve.kind === APPROVE.working ? "Approving…" : "Approve"}
          </Button>
        </CardFooter>
      </Card>
    </PageShell>
  );
}

function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <main className="min-h-screen flex items-center justify-center bg-background p-6">
      <div className="w-full max-w-md">{children}</div>
    </main>
  );
}

function VerificationCodeDisplay({ code }: { code: string }) {
  return (
    <output
      aria-label="Verification code"
      className="block font-mono text-3xl tracking-widest text-center py-4"
    >
      {code}
    </output>
  );
}

function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const onCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  }, [value]);
  return (
    <Button variant="secondary" onClick={() => void onCopy()}>
      {copied ? "Copied" : "Copy code"}
    </Button>
  );
}

function sanitizeTokenName(raw: string): string {
  const trimmed = raw.slice(0, TOKEN_NAME_MAX_LEN);
  const printable = trimmed.replace(/[\x00-\x1f\x7f]/g, "");
  return printable.length > 0 ? printable : "your terminal";
}
