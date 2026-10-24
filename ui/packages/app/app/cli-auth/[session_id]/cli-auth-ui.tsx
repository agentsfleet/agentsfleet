"use client";

import {
  Alert, AlertDescription, AlertTitle, Button, Card, CardContent, CardDescription,
  CardFooter, CardHeader, CopyButton, PageTitle,
} from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";

export const LOAD = { loading: "loading", active: "active", terminal: "terminal", error: "error" } as const;
export const APPROVE = { idle: "idle", working: "working", approved: "approved", uncertain: "uncertain", failed: "failed" } as const;
export const SESSION_STATUS = { pending: "pending", verification_pending: "verification_pending" } as const;
export interface ActiveSession {
  status: (typeof SESSION_STATUS)[keyof typeof SESSION_STATUS];
  cli_public_key: string;
  token_name: string;
  expires_at: string;
}
export type LoadState = { kind: typeof LOAD.loading }
  | { kind: typeof LOAD.active; session: ActiveSession }
  | { kind: typeof LOAD.terminal; message: string }
  | { kind: typeof LOAD.error; message: string };
export type ApproveState = { kind: typeof APPROVE.idle | typeof APPROVE.working }
  | { kind: typeof APPROVE.approved | typeof APPROVE.uncertain; verificationCode: string }
  | { kind: typeof APPROVE.failed; message: string };

// Presentational chrome for the CLI-auth approve page. Extracted from
// page.tsx to keep that file under the length cap; these are internal to
// the route (the approve-flow logic stays in page.tsx).
//
// The copy affordance used to live here, as a component ALSO named CopyButton —
// which shadowed the design-system export of the same name and is very likely why
// that primitive went unused across the whole app. It is gone; the route imports
// the real one.

export function PageShell({ children }: { children: React.ReactNode }) {
  return (
    <main className="ph-no-capture ph-no-record min-h-screen flex items-center justify-center bg-background p-6">
      <div className="w-full max-w-md">{children}</div>
    </main>
  );
}

export function ManualApprovalView({ isLoaded, isSignedIn, load, approve, onApprove }: {
  isLoaded: boolean; isSignedIn: boolean | undefined; load: LoadState; approve: ApproveState;
  onApprove: (session: ActiveSession) => void;
}) {
  if (!isLoaded || load.kind === LOAD.loading) return <RouteLoading fullScreen />;
  if (!isSignedIn) return <AuthNotice title="Sign in to continue" message="You need to be signed in to approve a CLI login." />;
  if (load.kind === LOAD.terminal || load.kind === LOAD.error) return <AuthNotice title="Login session unavailable" message={load.message} />;
  if (approve.kind === APPROVE.approved || approve.kind === APPROVE.uncertain) return <PageShell><Card>
    <CardHeader><PageTitle>Finish signing in</PageTitle><CardDescription>
      {approve.kind === APPROVE.uncertain
        ? "We couldn't confirm the approval over the network. If your terminal is asking for a code, enter this one; otherwise refresh and try again."
        : "Go back to the terminal where you started signing in and enter this code."}
    </CardDescription></CardHeader>
    <CardContent><VerificationCodeDisplay code={approve.verificationCode} /></CardContent>
    <CardFooter><CopyButton value={approve.verificationCode} label="Copy code" showLabel /></CardFooter>
  </Card></PageShell>;
  return <PageShell><Card>
    <CardHeader><PageTitle>Connect your terminal</PageTitle><CardDescription>
      Allow your terminal to use your agentsfleet account. Continue only if you started this sign-in.
    </CardDescription></CardHeader>
    <CardContent>{approve.kind === APPROVE.failed ? <Alert variant="destructive">
      <AlertTitle>Could not approve</AlertTitle><AlertDescription>{approve.message}</AlertDescription>
    </Alert> : null}</CardContent>
    <CardFooter className="gap-2"><Button data-testid="cli-auth-approve" onClick={() => onApprove(load.session)}
      disabled={approve.kind === APPROVE.working || approve.kind === APPROVE.failed || load.session.status === SESSION_STATUS.verification_pending}>
      {approve.kind === APPROVE.working ? "Authorizing…" : "Authorize"}
    </Button></CardFooter>
  </Card></PageShell>;
}

export function AuthNotice({ title, message }: { title: string; message: string }) {
  return <PageShell><Card><CardHeader><PageTitle>{title}</PageTitle>
    <CardDescription>{message}</CardDescription></CardHeader></Card></PageShell>;
}

export function VerificationCodeDisplay({ code }: { code: string }) {
  return (
    <output
      aria-label="Verification code"
      className="block font-mono text-3xl tracking-widest text-center py-4"
    >
      {code}
    </output>
  );
}
