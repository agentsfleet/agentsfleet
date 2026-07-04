"use client";

import { useCallback, useState } from "react";
import { useRouter } from "next/navigation";
import { presentErrorString } from "@/lib/errors";

/** A failed action's raw error, still carrying its `errorCode` for CODE_MAP lookup. */
export type ProviderActionError = { message: string; errorCode?: string };

// Shared action-runner for the Models client surfaces (hero, switch list,
// panels). Every model/credential mutation follows the same shape: clear error,
// flip pending, await a server action that returns a ProviderActionError or null,
// surface the friendly copy or run an optional success step + router.refresh().
// Centralised here so every call site's error routes through presentErrorString
// instead of rendering the raw backend string.
export function useProviderAction() {
  const router = useRouter();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    async (action: string, fn: () => Promise<ProviderActionError | null>, onSuccess?: () => void) => {
      setError(null);
      setPending(true);
      try {
        const err = await fn();
        if (err) {
          setError(presentErrorString({ errorCode: err.errorCode, message: err.message, action }));
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
