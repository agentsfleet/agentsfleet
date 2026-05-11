import { useAuth, useUser, UserButton, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

// Mirror of `lib/auth/server.ts`: client-side `getToken()` returns Token B
// (the `api`-template JWT zombied accepts as Bearer). Bare `getToken()`
// returns the default session token (Token A), which lacks
// `metadata.tenant_id` and the api `aud` claim — see docs/AUTH.md
// "The two tokens at a glance".
const API_TEMPLATE = "api" as const;

export function useClientToken(): { getToken: () => Promise<string | null> } {
  const { getToken } = useAuth();
  return { getToken: () => getToken({ template: API_TEMPLATE }) };
}

// Hook returning the current user's identity. Keyed on Clerk today;
// swapping to zombie-auth means replacing only this file + server.ts.
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
export const AuthProvider = ClerkProvider;
export const AuthUserButton = UserButton;
export const AuthSignIn = SignIn;
export const AuthSignUp = SignUp;
