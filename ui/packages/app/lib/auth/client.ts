"use client";

import { useCallback, useEffect, useLayoutEffect, useRef } from "react";
import { useUser, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

// `useClientToken()` was retired in M64_006. Every dashboard mutation/read
// now flows through a Server Action that calls `auth().getToken()`
// server-side. The in-browser SDK keeps only the `__session` cookie fresh;
// application code never receives its token value or builds a Bearer header.

// Clerk session tokens live for roughly one minute. Refresh with enough
// headroom that a long dashboard journey cannot submit a Server Action POST
// after the cookie expires; POST cannot complete Clerk's redirect handshake.
const SESSION_REFRESH_INTERVAL_MS = 45_000;

/**
 * A fresh session token on demand, and whose session it is.
 *
 * `refresh` mints the next token through Clerk's `user.reload()`, which
 * refreshes the `__session` cookie without returning token bytes to this
 * module. A call while one is in flight joins nothing and returns at once.
 * `signedInUserId` is null until a signed-in session has loaded.
 */
export function useSessionRefresh(): {
  signedInUserId: string | null;
  refresh: () => Promise<void>;
} {
  const { isLoaded, isSignedIn, user } = useUser();
  const refreshInFlight = useRef(false);
  // Clerk re-instantiates the User resource after every reload(), so a
  // callback keyed on the object identity would change once per refresh, and
  // an effect depending on it would re-arm and fire again, doubling every FAPI
  // call. The ref tracks the live resource; `refresh` stays one function.
  const userRef = useRef(user);
  useLayoutEffect(() => {
    userRef.current = user;
  }, [user]);

  const refresh = useCallback(async (): Promise<void> => {
    if (refreshInFlight.current) return;
    refreshInFlight.current = true;
    try {
      await userRef.current?.reload();
    } catch {
      // Offline and transient Clerk failures retry on the caller's next tick;
      // never leak an unhandled promise rejection.
    } finally {
      refreshInFlight.current = false;
    }
  }, []);

  const signedInUserId = isLoaded && isSignedIn ? user.id : null;
  return { signedInUserId, refresh };
}

export function AuthSessionKeeper(): null {
  const { signedInUserId, refresh } = useSessionRefresh();

  useEffect(() => {
    if (signedInUserId === null) return;

    const refreshWhenVisible = (): void => {
      if (document.visibilityState === "visible") void refresh();
    };

    void refresh();
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
  }, [signedInUserId, refresh]);

  return null;
}

// Hook returning the current user's identity. Keyed on Clerk today;
// swapping to fleet-auth means replacing only this file + server.ts.
export function useCurrentUser(): {
  isLoaded: boolean;
  isSignedIn: boolean;
  userId: string | null;
  emailAddress: string | null;
  displayName: string | null;
} {
  const { isLoaded, isSignedIn, user } = useUser();
  return {
    isLoaded,
    isSignedIn: Boolean(isSignedIn),
    userId: user?.id ?? null,
    emailAddress: user?.primaryEmailAddress?.emailAddress ?? null,
    displayName: user?.fullName ?? user?.username ?? null,
  };
}

// UI components re-exported so app code never imports from @clerk/nextjs
// directly. Replacing the auth provider later = swap these named exports
// to the new library's equivalents; no consumer changes.
//
// Note: server-side dashboard pages call `auth()` directly from
// `@clerk/nextjs/server`. The lone surviving api-template mint is in
// `app/cli-auth/[session_id]/page.tsx` — the CLI handshake carve-out.
export const AuthProvider = ClerkProvider;
export { SignOutButton as AuthSignOutButton, UserProfile as AuthUserProfile } from "@clerk/nextjs";
export const AuthSignIn = SignIn;
export const AuthSignUp = SignUp;
