import { useAuth, useUser, UserButton, ClerkProvider, SignIn, SignUp } from "@clerk/nextjs";

export function useClientToken(): { getToken: () => Promise<string | null> } {
  const { getToken } = useAuth();
  return { getToken };
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
