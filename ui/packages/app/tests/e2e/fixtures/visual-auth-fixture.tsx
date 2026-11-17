import type { PropsWithChildren } from "react";
import type { AccountAccess } from "@/lib/auth/account-types";

// A component fixture has sample identity, never a live session or credentials.
const avatar = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32" fill="#6a4e8b"/><circle cx="16" cy="12" r="6" fill="#eadff8"/><path d="M4 32a12 12 0 0 1 24 0" fill="#eadff8"/></svg>')}`;
export const useCurrentUser = () => ({ isLoaded: true, isSignedIn: true, userId: "fixture-owner", displayName: "Kishore", emailAddress: "you@example.test", imageUrl: new URLSearchParams(window.location.search).has("avatar") ? avatar : null });
export const useAccountSettings = (): AccountAccess => ({status: "ready", contextKey: "visual-account", client: {
  listSessions: async () => ({ok: false, error: "Use authenticated review for account operations."}),
  revokeSession: async () => ({ok: false, error: "A visual fixture cannot change sessions."}),
}});
export const AuthSignOutButton = ({ children }: PropsWithChildren) => children;
export const AuthProvider = ({ children }: PropsWithChildren) => children;
export const AuthSessionKeeper = () => null;
export const credential = async () => null;
export const requireCredential = async () => { throw new Error("A visual fixture cannot perform authenticated writes"); };
export const claims = async () => null;
