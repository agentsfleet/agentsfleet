"use client";

import { useEffect, useRef, useState, useTransition } from "react";
import type { Secret } from "@/lib/api/secrets";
import { listSecretsAction } from "../actions";
import { SECRETS_LOAD, type SecretsLoad } from "./secrets-load";

/**
 * The workspace's stored-secret list, fetched on demand — NOT preloaded. The
 * Add dialog fires `refreshSecrets` on open and after committing a secret;
 * an ordinary Models visit never pays for it.
 *
 * The confirmed list distinguishes saved-key selection from choosing a free
 * name. The API also refuses a name that another tab claimed first.
 *
 * `ready` is sticky: a failed REFRESH keeps the last good list live (the
 * form still has usable data), while a list that never loaded lands on
 * `error` so the dialog blocks and offers retry.
 */
export function useStoredSecrets(workspaceId: string, subject: string | null = null): {
  secrets: Secret[];
  secretsLoad: SecretsLoad;
  refreshSecrets: () => void;
} {
  const [, startTransition] = useTransition();
  const [secrets, setSecrets] = useState<Secret[]>([]);
  const [secretsLoad, setSecretsLoad] = useState<SecretsLoad>(SECRETS_LOAD.idle);
  const scope = `${workspaceId}:${subject ?? ""}`;
  const [loadedScope, setLoadedScope] = useState(scope);
  // Latest-wins: open/close/reopen and retry can overlap requests, and this
  // list guards held names — a slow stale response must not overwrite
  // a newer list and still call itself ready. Same pattern as
  // ModelCatalogueProvider.preload.
  const generation = useRef(0);
  useEffect(() => {
    setLoadedScope(scope);
    setSecrets([]);
    setSecretsLoad(SECRETS_LOAD.idle);
    return () => { generation.current += 1; };
  }, [scope]);

  function refreshSecrets() {
    const mine = ++generation.current;
    setSecretsLoad((prior) => (prior === SECRETS_LOAD.ready ? prior : SECRETS_LOAD.loading));
    startTransition(async () => {
      // try/catch because the action ROUND-TRIP itself can reject (network
      // failure, deploy skew): without it, a rejection would strand the
      // dialog at "Checking your stored secrets…" with Save disabled and no
      // Retry, since Retry only renders in the error state.
      try {
        const r = await listSecretsAction(workspaceId);
        if (mine !== generation.current) return;
        if (!r.ok) {
          if (r.status === 401 || r.status === 403) {
            setSecrets([]);
            setSecretsLoad(SECRETS_LOAD.error);
            return;
          }
          setSecretsLoad((prior) => (prior === SECRETS_LOAD.ready ? prior : SECRETS_LOAD.error));
          return;
        }
        setSecrets(r.data.secrets);
        setSecretsLoad(SECRETS_LOAD.ready);
      } catch {
        if (mine !== generation.current) return;
        setSecretsLoad((prior) => (prior === SECRETS_LOAD.ready ? prior : SECRETS_LOAD.error));
      }
    });
  }

  return loadedScope === scope
    ? { secrets, secretsLoad, refreshSecrets }
    : { secrets: [], secretsLoad: SECRETS_LOAD.idle, refreshSecrets };
}
