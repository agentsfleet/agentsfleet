"use client";

import type { AccountSessionPage, AccountSettingsClient } from "@/lib/auth/account-types";
import { useCallback, useEffect, useRef, useState } from "react";

type ReadState = {kind: "loading"} | {kind: "failed"; error: string}
  | {kind: "ready"; page: AccountSessionPage; error: string | null; notice: string | null};
type Operation = null | "read" | {revoke: string};

/** One mounted account owns reads, confirmed changes and late-response exclusion. */
export function useAccountSessions(client: AccountSettingsClient) {
  const [state, setState] = useState<ReadState>({kind: "loading"});
  const [operation, setOperation] = useState<Operation>(null);
  const epoch = useRef(0);
  const busy = useRef(false);
  const fail = (error: string) => setState(previous => previous.kind === "ready" ? {...previous, error} : {kind: "failed", error});
  const readPage = useCallback(async (offset: number, append: boolean, mine: number) => {
    try {
      const result = await client.listSessions(offset);
      if (epoch.current !== mine) return;
      if (!result.ok) {fail(result.error); return;}
      setState(previous => ({kind: "ready", page: {...result.data,
        sessions: append && previous.kind === "ready"
          ? [...new Map([...previous.page.sessions, ...result.data.sessions].map(session => [session.id, session])).values()]
          : result.data.sessions}, error: null, notice: previous.kind === "ready" ? previous.notice : null}));
    } catch {if (epoch.current === mine) fail("Your sessions could not be loaded. Try again.");}
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
    setOperation({revoke: id});
    const mine = epoch.current;
    try {
      const result = await client.revokeSession(id);
      if (epoch.current !== mine) return false;
      if (!result.ok) {fail(result.error); return false;}
      setState({...state, page: {sessions: state.page.sessions.filter(session => session.id !== id), nextOffset: null},
        error: null, notice: "Session signed out"});
      await readPage(0, false, mine);
      return true;
    } catch {
      if (epoch.current === mine) fail("Session sign-out could not be confirmed. Refresh the list before trying again.");
      return false;
    } finally {if (epoch.current === mine) {busy.current = false; setOperation(null);}}
  }
  return {state, pending: operation !== null, refresh, revoke};
}
