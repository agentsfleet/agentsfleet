"use client";

import type { ComponentProps, RefObject } from "react";
import { createContext, createElement, useCallback, useContext, useEffect, useLayoutEffect, useRef } from "react";
import { useClerk, useSession, useUser, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

// The in-browser SDK keeps only the `__session` cookie fresh. Every dashboard
// read and mutation resolves its bearer server-side (`credential.ts`), so
// application code never receives a token value or builds a Bearer header.

// Clerk session tokens live for roughly one minute. Refresh with enough
// headroom that a long dashboard journey cannot submit a Server Action POST
// after the cookie expires; POST cannot complete Clerk's redirect handshake.
const SESSION_REFRESH_INTERVAL_MS = 45_000;

// Retry a failed proactive refresh within the remaining minute of token lifetime.
const EXPIRY_HEADROOM_MS = 15_000;

type RefreshState = {
  sessionId: string;
  userId: string;
  dueAt: number;
  expiresAt?: number;
  pending?: Promise<number>;
};

// The mounted provider shares one refresh across the keeper and onboarding.
const RefreshContext = createContext<RefObject<RefreshState | undefined> | null>(null);

function refreshCookie(
  active: RefObject<RefreshState | undefined>,
  session: NonNullable<ReturnType<typeof useSession>["session"]>,
  force: boolean,
): Promise<number> {
  const user = session.user;
  const now = Date.now();
  if (active.current?.sessionId !== session.id || active.current.userId !== user.id) {
    active.current = { sessionId: session.id, userId: user.id, dueAt: 0 };
  }
  const state = active.current;
  // The SDK may have renewed its cookie independently. Only the public expiry
  // claim matters here. Missing metadata leaves the existing 45-second cadence.
  const exp = session.lastActiveToken?.jwt?.claims.exp;
  const expiresAt = typeof exp === "number" && Number.isFinite(exp) ? exp : undefined;
  if (expiresAt !== state.expiresAt) {
    state.expiresAt = expiresAt;
    state.dueAt = expiresAt === undefined ? 0 : expiresAt * 1000 - EXPIRY_HEADROOM_MS;
  }
  if (state.pending) return state.pending;
  if (!force && now < state.dueAt) return Promise.resolve(state.dueAt);
  const attempt = async () => {
    try {
      await user.reload();
      // A token observed during this reload can have its own, earlier deadline.
      if (state.expiresAt === expiresAt || state.expiresAt === undefined) {
        state.dueAt = now + SESSION_REFRESH_INTERVAL_MS;
      }
      return state.dueAt;
    } catch {
      // An online, visibility or focus signal can retry sooner than the timer.
      state.dueAt = 0;
      return Date.now() + EXPIRY_HEADROOM_MS;
    }
  };
  state.pending = attempt().finally(() => { state.pending = undefined; });
  return state.pending;
}

function useBrowserSession() {
  const { isSignedIn, session } = useSession();
  const provider = useContext(RefreshContext);
  const local = useRef<RefreshState>(undefined);
  const active = provider ?? local;
  const current = isSignedIn ? session : null;
  const live = useRef(current);
  // Clerk can replace the resource on every reload. Track its identity and
  // expiry separately so a replacement alone never restarts the keeper.
  useLayoutEffect(() => {
    live.current = current;
    return () => { live.current = null; };
  }, [current]);
  const reload = useCallback((force: boolean) => {
    const resource = live.current;
    return resource
      ? refreshCookie(active, resource, force)
      : Promise.resolve(Date.now() + EXPIRY_HEADROOM_MS);
  }, [active]);
  return { sessionId: current?.id, signedInUserId: current?.user.id ?? null,
    expiresAt: current?.lastActiveToken?.jwt?.claims.exp, reload };
}

/** Refresh even a recently minted cookie so onboarding can observe a new tenant claim. */
export function useSessionRefresh(): { signedInUserId: string | null; refresh: () => Promise<void> } {
  const { signedInUserId, reload } = useBrowserSession();
  const refresh = useCallback(async () => { await reload(true); }, [reload]);
  return { signedInUserId, refresh };
}

export function AuthSessionKeeper(): null {
  const { signedInUserId, sessionId, expiresAt, reload } = useBrowserSession();

  useEffect(() => {
    if (!signedInUserId) return;
    let stopped = false;
    let timer: number | undefined;
    const refreshWhenVisible = () => {
      window.clearTimeout(timer);
      if (document.visibilityState !== "visible") return;
      void reload(false).then(dueAt => {
        if (stopped) return;
        window.clearTimeout(timer);
        timer = window.setTimeout(refreshWhenVisible, Math.max(0, dueAt - Date.now()));
      });
    };
    refreshWhenVisible();
    window.addEventListener("focus", refreshWhenVisible);
    window.addEventListener("online", refreshWhenVisible);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
      window.removeEventListener("focus", refreshWhenVisible);
      window.removeEventListener("online", refreshWhenVisible);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [signedInUserId, sessionId, expiresAt, reload]);

  return null;
}

// Hook returning the current user's identity. Keyed on Clerk today;
// swapping to fleet-auth means replacing only this file and `credential.ts`.
export function useCurrentUser(): {
  isLoaded: boolean;
  isSignedIn: boolean;
  userId: string | null;
  emailAddress: string | null;
  displayName: string | null;
  imageUrl: string | null;
} {
  const { isLoaded, isSignedIn, user } = useUser();
  return {
    isLoaded,
    isSignedIn: Boolean(isSignedIn),
    userId: user?.id ?? null,
    emailAddress: user?.primaryEmailAddress?.emailAddress ?? null,
    displayName: user?.fullName ?? user?.username ?? null,
    imageUrl: user?.imageUrl ?? null,
  };
}

// UI components re-exported so app code never imports from @clerk/nextjs
// directly. Replacing the auth provider later = swap these named exports
// to the new library's equivalents; no consumer changes.
//
// Personal settings use account-client.ts for app-owned profile/session shapes;
// server-side credential and personal-session authority stay in credential.ts.
export function AuthProvider(props: ComponentProps<typeof ClerkProvider>) {
  const refresh = useRef<RefreshState>(undefined);
  return createElement(ClerkProvider, props,
    createElement(RefreshContext.Provider, { value: refresh }, props.children));
}
export { SignOutButton as AuthSignOutButton } from "@clerk/nextjs";
export const AuthSignIn = SignIn;
export const AuthSignUp = SignUp;

export function useAuthLoadFailed(): boolean {
  return useClerk().status === "error";
}
