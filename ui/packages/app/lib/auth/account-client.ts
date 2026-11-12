"use client";

import type { AccountAccess, AccountSettingsClient } from "./account-types";
import { useCallback, useLayoutEffect, useMemo, useRef } from "react";
import { useReverification, useSession, useUser } from "@clerk/nextjs";
import { listAccountSessionsAction, revokeAccountSessionAction } from "./account-actions";
import { accountSessionPageSchema } from "./account-types";

type Generation = {key: string};

/** Provider resources and challenge hints stop here; screens consume app-owned data. */
export function useAccountSettings(): AccountAccess {
  const identity = useUser();
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
  const client = useMemo<AccountSettingsClient>(() => {
    const current = () => subject !== null && live.current?.id === subject && owner.current === generation;
    return {
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
    };
  }, [subject, generation, revoke]);
  if (!identity.isLoaded || !session.isLoaded) return {status: "loading"};
  if (!identity.user || !session.session) return {status: "signed-out"};
  return {status: "ready", contextKey, client};
}

function changed() {return {ok: false as const, status: 409, error: "Your signed-in account changed. Reload this page before continuing."};}
