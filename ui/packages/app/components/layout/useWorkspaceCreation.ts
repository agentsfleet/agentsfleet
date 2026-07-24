"use client";

import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createWorkspaceAction } from "@/app/(dashboard)/actions";
import type { CreateWorkspaceResponse } from "@/lib/api/workspaces";
import { presentErrorString } from "@/lib/errors";

export type WorkspaceCreationCallbacks = {
  onSuccess: (workspace: CreateWorkspaceResponse) => void;
};

export type WorkspaceCreationLifecycle = {
  onSuccess: (workspace: CreateWorkspaceResponse, attached: boolean) => void;
  onDetachedFailure: (message: string) => void;
};

type Attempt = {
  attached: boolean;
  callbacks: WorkspaceCreationCallbacks;
  owner: symbol;
};

const CREATE_WORKSPACE_ACTION = "create workspace";

export type CreatedWorkspace = {
  id: string;
  name: string | null;
};

export function useWorkspaceCreationController(lifecycle: WorkspaceCreationLifecycle) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [createdWorkspaces, setCreatedWorkspaces] = useState<CreatedWorkspace[]>([]);
  const mountedRef = useRef(true);
  const attemptRef = useRef<Attempt | null>(null);
  const lifecycleRef = useRef(lifecycle);

  useLayoutEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useLayoutEffect(() => {
    lifecycleRef.current = lifecycle;
  }, [lifecycle]);

  const reset = useCallback(() => {
    setError(null);
  }, []);

  const detach = useCallback((owner: symbol) => {
    const attempt = attemptRef.current;
    if (!attempt || attempt.owner !== owner || !attempt.attached) {
      return false;
    }
    attempt.attached = false;
    return true;
  }, []);

  const dismiss = useCallback((owner: symbol) => {
    const detached = detach(owner);
    if (!detached) {
      setError(null);
      return false;
    }
    setError(null);
    return true;
  }, [detach]);

  const recordCreatedWorkspace = useCallback((workspace: CreateWorkspaceResponse) => {
    setCreatedWorkspaces((current) => {
      const next = { id: workspace.workspace_id, name: workspace.name };
      const found = current.some((candidate) => candidate.id === next.id);
      return found
        ? current.map((candidate) => (candidate.id === next.id ? next : candidate))
        : [...current, next];
    });
  }, []);

  const completeFailure = useCallback((attempt: Attempt, message: string) => {
    if (attempt.attached) {
      setError(message);
    } else {
      lifecycleRef.current.onDetachedFailure(message);
    }
  }, []);

  const create = useCallback(
    async (
      name: string | undefined,
      owner: symbol,
      callbacks: WorkspaceCreationCallbacks,
    ) => {
      if (attemptRef.current) return;

      const attempt: Attempt = { attached: true, callbacks, owner };
      attemptRef.current = attempt;
      setError(null);
      setPending(true);

      try {
        const result = await createWorkspaceAction({ name });
        if (!mountedRef.current || attemptRef.current !== attempt) return;

        attemptRef.current = null;
        setPending(false);
        if (result.ok) {
          recordCreatedWorkspace(result.data);
          lifecycleRef.current.onSuccess(result.data, attempt.attached);
          if (attempt.attached) attempt.callbacks.onSuccess(result.data);
          return;
        }

        completeFailure(
          attempt,
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: CREATE_WORKSPACE_ACTION,
          }),
        );
      } catch {
        if (!mountedRef.current || attemptRef.current !== attempt) return;

        attemptRef.current = null;
        setPending(false);
        completeFailure(
          attempt,
          presentErrorString({ action: CREATE_WORKSPACE_ACTION }),
        );
      }
    },
    [completeFailure, recordCreatedWorkspace],
  );

  return useMemo(
    () => ({ create, createdWorkspaces, detach, dismiss, error, pending, reset }),
    [create, createdWorkspaces, detach, dismiss, error, pending, reset],
  );
}
