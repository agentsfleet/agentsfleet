"use client";

import { useEffect, useRef } from "react";
import { useUser, UserButton, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

// `useClientToken()` was retired in M64_006. Every dashboard mutation/read
// now flows through a Server Action that calls `auth().getToken()`
// server-side. The in-browser SDK keeps only the `__session` cookie fresh;
// application code never receives its token value or builds a Bearer header.

// Clerk session tokens live for roughly one minute. Refresh with enough
// headroom that a long dashboard journey cannot submit a Server Action POST
// after the cookie expires; POST cannot complete Clerk's redirect handshake.
const SESSION_REFRESH_INTERVAL_MS = 45_000;

export function AuthSessionKeeper(): null {
  const { isLoaded, isSignedIn, user } = useUser();
  const refreshInFlight = useRef(false);

  useEffect(() => {
    if (!isLoaded || !isSignedIn) return;

    const refreshSession = async (): Promise<void> => {
      if (refreshInFlight.current) return;
      refreshInFlight.current = true;
      try {
        // Clerk documents user.reload() as refreshing both the User resource
        // and the session token without returning token bytes to this module.
        await user.reload();
      } catch {
        // Offline and transient Clerk failures retry on the next interval or
        // browser-resume signal; never leak an unhandled promise rejection.
      } finally {
        refreshInFlight.current = false;
      }
    };
    const refreshWhenVisible = (): void => {
      if (document.visibilityState === "visible") void refreshSession();
    };

    void refreshSession();
    const interval = window.setInterval(refreshWhenVisible, SESSION_REFRESH_INTERVAL_MS);
    window.addEventListener("focus", refreshWhenVisible);
    window.addEventListener("online", refreshWhenVisible);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      window.clearInterval(interval);
      window.removeEventListener("focus", refreshWhenVisible);
      window.removeEventListener("online", refreshWhenVisible);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [isLoaded, isSignedIn, user]);

  return null;
}

// Hook returning the current user's identity. Keyed on Clerk today;
// swapping to fleet-auth means replacing only this file + server.ts.
export function useCurrentUser(): {
  isLoaded: boolean;
  isSignedIn: boolean;
  userId: string | null;
  emailAddress: string | null;
} {
  const { isLoaded, isSignedIn, user } = useUser();
  return {
    isLoaded,
    isSignedIn: Boolean(isSignedIn),
    userId: user?.id ?? null,
    emailAddress: user?.primaryEmailAddress?.emailAddress ?? null,
  };
}

// UI components re-exported so app code never imports from @clerk/nextjs
// directly. Replacing the auth provider later = swap these named exports
// to the new library's equivalents; no consumer changes.
//
// Note: post-Stage-1, server-side dashboard pages call `auth()` directly
// from `@clerk/nextjs/server`. The lone surviving api-template mint is in
// `app/cli-auth/[session_id]/page.tsx` — the CLI handshake carve-out.
export const AuthProvider = ClerkProvider;
export const AuthUserButton = UserButton;
export const AuthSignIn = SignIn;
export const AuthSignUp = SignUp;
