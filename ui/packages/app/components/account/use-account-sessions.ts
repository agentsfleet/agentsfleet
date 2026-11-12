"use client";

import type { AccountSessionPage, AccountSettingsClient } from "@/lib/auth/account-types";
import { useCallback, useEffect, useRef, useState } from "react";

type ReadState = {kind: "loading"} | {kind: "failed"; error: string}
  | {kind: "ready"; page: AccountSessionPage; error: string | null; notice: string | null};
type Operation = null | "read" | "revoke-others" | {revoke: string};

/** One mounted account owns reads, confirmed changes and late-response exclusion. */
export function useAccountSessions(client: AccountSettingsClient) {
  const [state, setState] = useState<ReadState>({kind: "loading"});
  const [operation, setOperation] = useState<Operation>(null);
  const epoch = useRef(0);
  const busy = useRef(false);
  const fail = (error: string, notice: string | null = null) => setState(previous => previous.kind === "ready" ? {...previous, error, notice} : {kind: "failed", error});
  const clearFeedback = () => setState(previous => previous.kind === "ready" ? {...previous, error: null, notice: null} : previous);
  const removeConfirmed = (id: string) => setState(previous => previous.kind === "ready"
    ? {...previous, page: {...previous.page, sessions: previous.page.sessions.filter(session => session.id !== id)}} : previous);
  const readPage = useCallback(async (offset: number, append: boolean, mine: number) => {
    try {
      const result = await client.listSessions(offset);
      if (epoch.current !== mine) return false;
      if (!result.ok) {fail(result.error); return false;}
      setState(previous => ({kind: "ready", page: {...result.data,
        sessions: append && previous.kind === "ready"
          ? [...new Map([...previous.page.sessions, ...result.data.sessions].map(session => [session.id, session])).values()]
          : result.data.sessions}, error: null, notice: previous.kind === "ready" ? previous.notice : null}));
      return true;
    } catch {if (epoch.current === mine) fail("Your sessions could not be loaded. Try again."); return false;}
  }, [client]);
  const refresh = useCallback(async (offset = 0, append = false) => {
    if (busy.current) return;
    busy.current = true;
    setOperation("read");
    const mine = epoch.current;
    try {await readPage(offset, append, mine);}
    finally {if (epoch.current === mine) {busy.current = false; setOperation(null);}}
  }, [readPage]);
  useEffect(() => {
    epoch.current += 1;
    busy.current = false;
    setState({kind: "loading"});
    void refresh();
    return () => {epoch.current += 1;};
  }, [refresh]);
  async function revoke(id: string): Promise<boolean> {
    if (busy.current || state.kind !== "ready") return false;
    busy.current = true;
    clearFeedback();
    setOperation({revoke: id});
    const mine = epoch.current;
    try {
      const result = await client.revokeSession(id);
      if (epoch.current !== mine) return false;
      if (!result.ok) {fail(result.error); return false;}
      setState({...state, page: {sessions: state.page.sessions.filter(session => session.id !== id), nextOffset: null},
        error: null, notice: "Session signed out"});
      if (!await readPage(0, false, mine) && epoch.current === mine)
        fail("The session list could not be refreshed. Try refreshing it again.", "Session signed out");
      return true;
    } catch {
      if (epoch.current === mine) fail("Session sign-out could not be confirmed. Refresh the list before trying again.");
      return false;
    } finally {if (epoch.current === mine) {busy.current = false; setOperation(null);}}
  }
  async function revokeOthers(): Promise<boolean> {
    if (busy.current || state.kind !== "ready") return false;
    busy.current = true;
    clearFeedback();
    setOperation("revoke-others");
    const mine = epoch.current;
    let failure: string | null = null;
    try {
      const targets = await otherSessionIds(client, () => epoch.current === mine);
      for (const id of targets) {
        if (epoch.current !== mine) return false;
        const result = await client.revokeSession(id);
        if (epoch.current !== mine) return false;
        if (!result.ok) { failure = result.error; break; }
        removeConfirmed(id);
      }
      if (epoch.current !== mine) return false;
      const refreshed = await readPage(0, false, mine);
      if (epoch.current !== mine) return false;
      if (failure) { fail(`Some sessions may remain signed in. ${failure}`); return false; }
      if (!refreshed) {fail("The session list could not be refreshed. Try refreshing it again.", "Other sessions signed out"); return true;}
      setState(previous => previous.kind === "ready" ? {...previous, notice: "Other sessions signed out"} : previous);
      return true;
    } catch {
      if (epoch.current === mine) fail("Other session sign-out could not be confirmed. Refresh before trying again.");
      return false;
    } finally { if (epoch.current === mine) { busy.current = false; setOperation(null); } }
  }
  return {state, pending: operation !== null, refresh, revoke, revokeOthers};
}


const SESSION_PAGE_LIMIT = 100;

/** Read the complete target set before revoking; a failed page must not imply success. */
async function otherSessionIds(client: AccountSettingsClient, current: () => boolean): Promise<string[]> {
  const ids = new Set<string>();
  const ownIds = new Set<string>();
  const offsets = new Set<number>();
  let offset = 0;
  for (let page = 0; page < SESSION_PAGE_LIMIT; page += 1) {
    if (!current()) throw new Error("Account changed");
    offsets.add(offset);
    const result = await client.listSessions(offset);
    if (!result.ok) throw new Error(result.error);
    for (const session of result.data.sessions) {
      if (session.current) ownIds.add(session.id);
      else ids.add(session.id);
    }
    const next = result.data.nextOffset;
    if (next === null) return [...ids].filter(id => !ownIds.has(id));
    if (offsets.has(next)) throw new Error("Session paging repeated");
    offset = next;
  }
  throw new Error("Session paging exceeded its bound");
}
