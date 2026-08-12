"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useTransition,
  type ReactNode,
} from "react";
import { usePathname, useRouter } from "next/navigation";
import type { ToastSeverity } from "@agentsfleet/design-system";
import type { CreateWorkspaceResponse } from "@/lib/api/workspaces";
import {
  DASHBOARD_ROOT_PATH,
  workspaceIdFromPath,
} from "@/lib/workspace-routes";
import {
  useWorkspaceCreationController,
  type CreatedWorkspace,
  type WorkspaceCreationCallbacks,
} from "@/components/layout/useWorkspaceCreation";
import {
  useWorkspaceNotice,
  WorkspaceNotice,
} from "@/components/layout/WorkspaceNotice";

type SharedWorkspaceCreation = ReturnType<typeof useWorkspaceCreationController> & {
  locked: boolean;
  settlingWorkspace: CreateWorkspaceResponse | null;
  showNotice: (severity: ToastSeverity, message: string) => void;
};

const WorkspaceCreationContext = createContext<SharedWorkspaceCreation | null>(null);

const BACKGROUND_CREATION_NOTICE = "Workspace creation continues in the background.";

export function WorkspaceCreationProvider({
  children,
  knownWorkspaceIds = [],
}: {
  children: ReactNode;
  knownWorkspaceIds?: readonly string[];
}) {
  const pathname = usePathname();
  const router = useRouter();
  const { notice, showNotice } = useWorkspaceNotice();
  const [routeSettlement, setRouteSettlement] = useState<CreateWorkspaceResponse | null>(null);
  const [reconciliationPending, startReconciliation] = useTransition();

  const beginRouteSettlement = useCallback((workspace: CreateWorkspaceResponse) => {
    setRouteSettlement(workspace);
  }, []);

  const lifecycle = useMemo(
    () => ({
      onSuccess(workspace: CreateWorkspaceResponse, attached: boolean) {
        showNotice("success", `Workspace created: ${workspace.name}.`);
        beginRouteSettlement(workspace);
        if (!attached && pathname !== DASHBOARD_ROOT_PATH) {
          router.refresh();
        }
      },
      onFailure() {
        startReconciliation(() => router.refresh());
      },
      onDetachedFailure(message: string) {
        showNotice("destructive", message);
      },
    }),
    [beginRouteSettlement, pathname, router, showNotice],
  );
  const controller = useWorkspaceCreationController(lifecycle);
  const controllerCreate = controller.create;
  const create = useCallback(
    (
      name: string,
      owner: symbol,
      callbacks: WorkspaceCreationCallbacks,
    ) => {
      if (reconciliationPending) return Promise.resolve();
      return controllerCreate(name, owner, callbacks);
    },
    [controllerCreate, reconciliationPending],
  );

  useEffect(() => {
    if (!routeSettlement) return;
    const workspaceId = routeSettlement.workspace_id;
    const routeSettled = workspaceIdFromPath(pathname) === workspaceId;
    const dataSettled = knownWorkspaceIds.includes(workspaceId);
    if (routeSettled || dataSettled) setRouteSettlement(null);
  }, [knownWorkspaceIds, pathname, routeSettlement]);

  const controllerDismiss = controller.dismiss;
  const dismiss = useCallback(
    (owner: symbol) => {
      const detached = controllerDismiss(owner);
      if (detached) showNotice("info", BACKGROUND_CREATION_NOTICE);
      return detached;
    },
    [controllerDismiss, showNotice],
  );

  const value = useMemo(
    () => ({
      ...controller,
      create,
      dismiss,
      locked:
        controller.pending ||
        reconciliationPending ||
        routeSettlement !== null,
      pending: controller.pending || reconciliationPending,
      settlingWorkspace: routeSettlement,
      showNotice,
    }),
    [
      controller,
      create,
      dismiss,
      reconciliationPending,
      routeSettlement,
      showNotice,
    ],
  );

  return (
    <WorkspaceCreationContext.Provider value={value}>
      {children}
      <WorkspaceNotice notice={notice} />
    </WorkspaceCreationContext.Provider>
  );
}

/// Read-only view of optimistically created workspaces for surfaces that only
/// resolve a label (the always-mounted switcher trigger). The full
/// `useWorkspaceCreation` hook registers an owner and callbacks; a label read
/// must not. Outside a provider this yields the empty list — the trigger then
/// falls back to the server-provided workspace list, never crashes the header.
export function useCreatedWorkspaces(): readonly CreatedWorkspace[] {
  const shared = useContext(WorkspaceCreationContext);
  return shared?.createdWorkspaces ?? EMPTY_CREATED_WORKSPACES;
}

const EMPTY_CREATED_WORKSPACES: readonly CreatedWorkspace[] = [];

export function useWorkspaceCreation(callbacks: WorkspaceCreationCallbacks) {
  const shared = useContext(WorkspaceCreationContext);
  if (!shared) {
    throw new Error("useWorkspaceCreation requires WorkspaceCreationProvider");
  }

  const ownerRef = useRef(Symbol("workspace-creation-owner"));
  const callbacksRef = useRef(callbacks);
  useLayoutEffect(() => {
    callbacksRef.current = callbacks;
  }, [callbacks]);

  const sharedCreate = shared.create;
  const sharedDetach = shared.detach;
  const sharedDismiss = shared.dismiss;
  const notifySuccess = useCallback(
    (workspace: CreateWorkspaceResponse) => callbacksRef.current.onSuccess(workspace),
    [],
  );
  const create = useCallback(
    (name: string) => sharedCreate(
      name,
      ownerRef.current,
      { onSuccess: notifySuccess },
    ),
    [notifySuccess, sharedCreate],
  );
  const dismiss = useCallback(
    () => sharedDismiss(ownerRef.current),
    [sharedDismiss],
  );

  useLayoutEffect(
    () => () => {
      sharedDetach(ownerRef.current);
    },
    [sharedDetach],
  );

  return useMemo(
    () => ({
      create,
      createdWorkspaces: shared.createdWorkspaces,
      dismiss,
      error: shared.error,
      locked: shared.locked,
      pending: shared.pending,
      reset: shared.reset,
      settlingWorkspace: shared.settlingWorkspace,
      showNotice: shared.showNotice,
    }),
    [create, dismiss, shared],
  );
}
