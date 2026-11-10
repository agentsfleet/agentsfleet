"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Button, EmptyState, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { LoaderCircleIcon } from "lucide-react";
import { useSessionRefresh } from "@/lib/auth/client";

// A new account's first minutes (M245 §10). Signup opens the account from
// Clerk's `user.created` webhook and then writes its tenant and scopes back to
// Clerk, so a browser that finished signing up can reach the dashboard with a
// token minted before either landed. Only a fresh token carries them, and the
// session keeper mints one every 45 s; this mints one every tick and re-runs
// the entry page, which redirects as soon as the token carries the tenant.

/** How often a fresh token is minted while the account opens. */
export const ACCOUNT_SETUP_POLL_MS = 2_000;

/**
 * How long the screen keeps minting before it says so. Svix redelivers a
 * signup whose writeback failed at 5 s and then 5 min, so a minute and a half
 * covers the first redelivery and a slow provider with room to spare.
 */
export const ACCOUNT_SETUP_GIVE_UP_MS = 90_000;

const TITLE = "Setting up your workspace";
const DESCRIPTION_WAITING = "Your account is being created. This usually takes a few seconds.";
const DESCRIPTION_STALLED =
  "This is taking longer than usual. Try again, or come back in a few minutes.";
const RETRY_LABEL = "Try again";
const SUPPORT_NOTE = "If this continues, contact support.";

export default function AccountSetup() {
  const router = useRouter();
  // A ref, so the loop below depends only on what restarts it: a router whose
  // identity changed between renders must not reset the deadline.
  const routerRef = useRef(router);
  routerRef.current = router;
  const { refresh } = useSessionRefresh();
  // Bumped by "Try again": a new run of the loop, with a new deadline.
  const [run, setRun] = useState(0);
  const [stalled, setStalled] = useState(false);

  useEffect(() => {
    if (stalled) return;
    let stopped = false;
    const tick = async (): Promise<void> => {
      await refresh();
      if (!stopped) routerRef.current.refresh();
    };
    const interval = window.setInterval(() => void tick(), ACCOUNT_SETUP_POLL_MS);
    const deadline = window.setTimeout(() => {
      window.clearInterval(interval);
      setStalled(true);
    }, ACCOUNT_SETUP_GIVE_UP_MS);
    void tick();
    return () => {
      stopped = true;
      window.clearInterval(interval);
      window.clearTimeout(deadline);
    };
  }, [refresh, run, stalled]);

  const tryAgain = useCallback(() => {
    setStalled(false);
    setRun((current) => current + 1);
  }, []);

  return (
    <PageLayout>
      <PageHeader className="flex-col sm:flex-col">
        <PageTitle>{TITLE}</PageTitle>
      </PageHeader>
      <EmptyState
        icon={
          stalled ? undefined : <LoaderCircleIcon size={32} className="animate-spin" aria-hidden />
        }
        title={TITLE}
        description={stalled ? DESCRIPTION_STALLED : DESCRIPTION_WAITING}
        action={
          stalled ? (
            <div className="flex flex-col items-center gap-md">
              <Button type="button" onClick={tryAgain} data-testid="account-setup-retry">
                {RETRY_LABEL}
              </Button>
              <p className="font-sans text-label text-muted-foreground">{SUPPORT_NOTE}</p>
            </div>
          ) : undefined
        }
      />
    </PageLayout>
  );
}
