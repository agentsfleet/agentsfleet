"use client";

import { useCallback, useState } from "react";
import { useRouter } from "next/navigation";

// Shared action-runner for the Models & Keys client surfaces (hero, switch list,
// panels). Every model/credential mutation follows the same shape: clear error,
// flip pending, await a server action that returns an error string or null,
// surface the error or run an optional success step + router.refresh(). Centralised
// here so the hero and the switch list don't each re-roll the pending/error dance.
export function useProviderAction() {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (fn: () => Promise<string | null>, onSuccess?: () => void) => {
      setError(null);
      setPending(true);
      try {
        const err = await fn();
        if (err) {
          setError(err);
          return;
        }
        onSuccess?.();
        router.refresh();
      } finally {
        setPending(false);
      }
    },
    [router],
  );

  return { pending, error, setError, run };
}
