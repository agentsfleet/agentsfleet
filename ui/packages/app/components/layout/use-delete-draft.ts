"use client";

import { useEffect, useRef, useState } from "react";
import type { DeleteDraft } from "@/lib/api/draft-request";
import { setupErrorMessage } from "@/lib/api/draft-errors";

export function useDeleteDraft(workspace: string, action: DeleteDraft | null, onRemoved: (id: string) => void) {
  const owner = useRef<AbortController | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => owner.current?.abort(), []);
  const remove = async (id: string): Promise<void> => {
    owner.current?.abort();
    const request = new AbortController();
    owner.current = request;
    setError(null);
    try {
      if (action) await action(id, request.signal);
      else {
        const {deleteDraft} = await import("@/lib/api/draft-request");
        await deleteDraft(`/live/v1/workspaces/${encodeURIComponent(workspace)}`, id, request.signal);
      }
      request.signal.throwIfAborted();
      onRemoved(id);
    } catch (cause) {
      if (!request.signal.aborted) setError(setupErrorMessage(cause));
      throw cause;
    } finally {
      if (owner.current === request) owner.current = null;
    }
  };
  return {remove, error};
}
