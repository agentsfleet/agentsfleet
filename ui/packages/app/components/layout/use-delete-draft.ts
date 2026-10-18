"use client";

import { useEffect, useRef, useState } from "react";
import { deleteDraft, setupErrorMessage, type DeleteDraft } from "@/lib/api/draft-request";

export function useDeleteDraft(workspace: string, action: DeleteDraft | null, onRemoved: (id: string) => void) {
  const owner = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => owner.current?.abort(), []);
  const remove = async (id: string): Promise<void> => {
    owner.current?.abort();
    const request = new AbortController();
    owner.current = request;
    setPending(true); setError(null);
    try {
      if (action) await action(id, request.signal);
      else await deleteDraft(`/live/v1/workspaces/${encodeURIComponent(workspace)}`, id, request.signal);
      request.signal.throwIfAborted();
      onRemoved(id);
    } catch (cause) {
      if (!request.signal.aborted) setError(setupErrorMessage(cause));
      throw cause;
    } finally {
      if (!request.signal.aborted) setPending(false);
      if (owner.current === request) owner.current = null;
    }
  };
  return {remove, pending, error, clearError: () => setError(null)};
}
