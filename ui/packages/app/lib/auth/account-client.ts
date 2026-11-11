"use client";

import type { AccountAccess, AccountSettingsClient, ProfileNames } from "./account-types";
import { useCallback, useLayoutEffect, useMemo, useRef } from "react";
import { useClerk, useReverification, useSession, useUser } from "@clerk/nextjs";
import { PROFILE_APPEARANCE } from "@/lib/profile-appearance";
import { listAccountSessionsAction, revokeAccountSessionAction } from "./account-actions";
import { accountSessionPageSchema, profileNamesSchema } from "./account-types";

type Generation = {key: string};

/** Provider resources and challenge hints stop here; screens consume app-owned data. */
export function useAccountSettings(): AccountAccess {
  const identity = useUser();
  const provider = useClerk();
  const session = useSession();
  const subject = identity.user?.id ?? null;
  const contextKey = JSON.stringify([subject, session.session?.id ?? null]);
  const generation = useMemo<Generation>(() => ({key: contextKey}), [contextKey]);
  const live = useRef(identity.user);
  const owner = useRef(generation);
  useLayoutEffect(() => {live.current = identity.user; owner.current = generation; return () => {live.current = null;};}, [identity.user, generation]);
  const revoke = useReverification(useCallback(async (expected: Generation, subject: string, id: string) => {
    if (live.current?.id !== subject || owner.current !== expected) return {ok: false as const, status: 409, error: "Your signed-in account changed. Reload this page."};
    return revokeAccountSessionAction(subject, id);
  }, []));
  const update = useReverification(useCallback(async (expected: Generation, names: ProfileNames) => {
    if (!live.current || owner.current !== expected) throw new Error("The signed-in account changed.");
    await live.current.update(names);
  }, []));
  const client = useMemo<AccountSettingsClient>(() => {
    const current = () => subject !== null && live.current?.id === subject && owner.current === generation;
    return {
      async updateProfile(names: ProfileNames) {
        const parsed = profileNamesSchema.safeParse(names);
        if (!parsed.success) return {ok: false, error: "Names must be at most 256 characters each."};
        const user = live.current;
        if (!user || !current()) return {ok: false, error: "Sign in before editing your profile."};
        try {await update(generation, parsed.data); return current() ? {ok: true, data: null} : changed();}
        catch {return {ok: false, error: "Your profile change could not be confirmed. Reload before trying again."};}
      },
      async listSessions(offset = 0) {
        if (!subject || !current()) return changed();
        try {
          const result = await listAccountSessionsAction(subject, offset);
          if (!current()) return changed();
          return result.ok ? {ok: true, data: accountSessionPageSchema.parse(result.data)} : result;
        } catch {return {ok: false, error: "Your sessions could not be loaded. Try again."};}
      },
      async revokeSession(id: string) {
        if (!subject || !current()) return changed();
        try {const result = await revoke(generation, subject, id); return current() ? result : changed();}
        catch {return {ok: false, error: "Session sign-out could not be confirmed. Refresh before trying again."};}
      },
      manageSignIn() {if (current()) provider.openUserProfile({appearance: PROFILE_APPEARANCE});},
    };
  }, [subject, generation, provider, revoke, update]);
  if (!identity.isLoaded || !session.isLoaded) return {status: "loading"};
  if (!identity.user || !session.session) return {status: "signed-out"};
  return {status: "ready", contextKey, client, profile: {subject: identity.user.id, firstName: identity.user.firstName ?? "", lastName: identity.user.lastName ?? "",
    email: identity.user.primaryEmailAddress?.emailAddress ?? null, imageUrl: identity.user.hasImage ? identity.user.imageUrl : null}};
}

function changed() {return {ok: false as const, status: 409, error: "Your signed-in account changed. Reload this page before continuing."};}
