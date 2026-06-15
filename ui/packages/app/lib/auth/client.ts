import { useUser, UserButton, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

// `useClientToken()` was retired in M64_006. Every dashboard mutation/read
// now flows through a Server Action that calls `auth().getToken()`
// server-side, so the Clerk in-browser SDK's session state is no longer
// load-bearing for any API call. This unblocks Playwright's cookie-mount
// fixture path — `clerkMiddleware` accepts the mounted `__session` cookie
// SSR, which is all Server Actions need. See `docs/AUTH.md` "Test
// infrastructure" for the full picture.

// Hook returning the current user's identity. Keyed on Clerk today;
// swapping to agent-auth means replacing only this file + server.ts.
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
