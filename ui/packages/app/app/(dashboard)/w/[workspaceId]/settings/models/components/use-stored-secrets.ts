"use client";

import { useState, useTransition } from "react";
import type { Secret } from "@/lib/api/secrets";
import { listSecretsAction } from "../actions";
import { SECRETS_LOAD, type SecretsLoad } from "./secrets-load";

/**
 * The workspace's stored-secret list, fetched on demand — NOT preloaded. The
 * Add dialog fires `refreshSecrets` on open and after committing a secret;
 * an ordinary Models visit never pays for it.
 *
 * `secretsLoad` is what lets the dialog tell "not loaded yet" apart from
 * "loaded and empty" and fail closed on anything but `ready`: its
 * rotate-vs-create decision and name-ownership guard read this list, and the
 * secrets POST upserts server-side, so submitting against an unknown list
 * would silently overwrite whatever already holds the typed name.
 *
 * `ready` is sticky: a failed REFRESH keeps the last good list live (the
 * form still has usable data), while a list that never loaded lands on
 * `error` so the dialog blocks and offers retry.
 */
export function useStoredSecrets(workspaceId: string): {
  secrets: Secret[];
  secretsLoad: SecretsLoad;
  refreshSecrets: () => void;
} {
  const [, startTransition] = useTransition();
  const [secrets, setSecrets] = useState<Secret[]>([]);
  const [secretsLoad, setSecretsLoad] = useState<SecretsLoad>(SECRETS_LOAD.idle);

  function refreshSecrets() {
    setSecretsLoad((prior) => (prior === SECRETS_LOAD.ready ? prior : SECRETS_LOAD.loading));
    startTransition(async () => {
      const r = await listSecretsAction(workspaceId);
      if (!r.ok) {
        setSecretsLoad((prior) => (prior === SECRETS_LOAD.ready ? prior : SECRETS_LOAD.error));
        return;
      }
      setSecrets(r.data.secrets);
      setSecretsLoad(SECRETS_LOAD.ready);
    });
  }

  return { secrets, secretsLoad, refreshSecrets };
}
