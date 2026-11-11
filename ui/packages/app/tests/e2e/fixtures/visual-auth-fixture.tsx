import type { PropsWithChildren } from "react";

// A component fixture has sample identity, never a live session or credentials.
export const useCurrentUser = () => ({ isLoaded: true, isSignedIn: true, userId: "fixture-owner", displayName: "Kishore", emailAddress: "you@example.test" });
export const AuthSignOutButton = ({ children }: PropsWithChildren) => children;
export const AuthProvider = ({ children }: PropsWithChildren) => children;
export const AuthSessionKeeper = () => null;
export const credential = async () => null;
export const requireCredential = async () => { throw new Error("A visual fixture cannot perform authenticated writes"); };
export const claims = async () => null;
