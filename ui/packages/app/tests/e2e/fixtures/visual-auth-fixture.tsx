import type { PropsWithChildren } from "react";
import type { AccountAccess } from "@/lib/auth/account-types";

// A component fixture has sample identity, never a live session or credentials.
export const useCurrentUser = () => ({ isLoaded: true, isSignedIn: true, userId: "fixture-owner", displayName: "Kishore", emailAddress: "you@example.test" });
export const useAccountSettings = (): AccountAccess => ({status: "ready", contextKey: "visual-account", client: {
  listSessions: async () => ({ok: false, error: "Use authenticated review for account operations."}),
  revokeSession: async () => ({ok: false, error: "A visual fixture cannot change sessions."}),
}});
/** Native credential forms are verified in the separate authenticated captures. */
export function AccountControls() {return <p className="text-label text-muted-foreground">Profile and sign-in forms require an authenticated account.</p>;}
export const AuthSignOutButton = ({ children }: PropsWithChildren) => children;
export const AuthProvider = ({ children }: PropsWithChildren) => children;
export const AuthSessionKeeper = () => null;
export const credential = async () => null;
export const requireCredential = async () => { throw new Error("A visual fixture cannot perform authenticated writes"); };
export const claims = async () => null;
