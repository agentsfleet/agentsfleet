"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import {
  getOnboardingProgressAction,
  type OnboardingProgress,
} from "@/lib/actions/preferences";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";

const PROGRESS_REFRESH_INTERVAL_MS = 30_000;
const DESKTOP_SIDEBAR_MEDIA_QUERY = "(min-width: 768px)";

export type OnboardingPollingMode = "desktop" | "mounted";

const FAIL_OPEN_PROGRESS: OnboardingProgress = {
  inputs: {
    modelConfigured: false,
    fleetTotal: 0,
    secretCount: 0,
    hasProcessedEvent: false,
    hasSteerEvent: false,
    cliTicked: false,
  },
  dismissed: false,
  collapsed: false,
};

type RefreshTriggers = {
  dismissed: boolean;
  pollingMode: OnboardingPollingMode;
  refresh: () => Promise<void>;
  workspaceId: string;
};

type RefreshState = {
  activeWorkspaceIds: Set<string>;
  latestGenerations: Map<string, number>;
  queuedWorkspaceIds: Set<string>;
  mounted: boolean;
  currentWorkspaceId: string;
};

type ProgressSetter = Dispatch<SetStateAction<Map<string, OnboardingProgress>>>;

export function useOnboardingProgress(
  workspaceId: string,
  locallyDismissed: boolean,
  pollingMode: OnboardingPollingMode,
) {
  const [progressByWorkspace, setProgressByWorkspace] = useState(
    new Map<string, OnboardingProgress>(),
  );
  const refreshState = useRef<RefreshState>({
    activeWorkspaceIds: new Set(),
    latestGenerations: new Map(),
    queuedWorkspaceIds: new Set(),
    mounted: false,
    currentWorkspaceId: workspaceId,
  });
  const refresh = useCallback(
    () => refreshProgress(workspaceId, refreshState.current, setProgressByWorkspace),
    [workspaceId],
  );

  useEffect(() => {
    const state = refreshState.current;
    state.mounted = true;
    return () => {
      state.mounted = false;
    };
  }, []);

  useEffect(() => {
    refreshState.current.currentWorkspaceId = workspaceId;
  }, [workspaceId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const progress = progressByWorkspace.get(workspaceId) ?? null;
  const dismissed = locallyDismissed || progress?.dismissed === true;

  useRefreshTriggers({ dismissed, pollingMode, refresh, workspaceId });

  return progress;
}

async function refreshProgress(
  workspaceId: string,
  state: RefreshState,
  setProgress: ProgressSetter,
) {
  const generation = (state.latestGenerations.get(workspaceId) ?? 0) + 1;
  state.latestGenerations.set(workspaceId, generation);
  if (state.activeWorkspaceIds.has(workspaceId)) {
    state.queuedWorkspaceIds.add(workspaceId);
    return;
  }

  state.activeWorkspaceIds.add(workspaceId);
  try {
    const result = await getOnboardingProgressAction(workspaceId);
    if (result.ok && canCommitProgress(state, workspaceId, generation)) {
      setProgress((current) => withProgress(current, workspaceId, result.data));
    } else if (!result.ok) {
      commitFailOpenProgress(state, workspaceId, generation, setProgress);
    }
  } catch {
    // Keep the last good progress, or use the fail-open checklist before the
    // first success. Focus, invalidation, or the fallback timer will retry.
    commitFailOpenProgress(state, workspaceId, generation, setProgress);
  } finally {
    state.activeWorkspaceIds.delete(workspaceId);
    const rerun = state.queuedWorkspaceIds.delete(workspaceId);
    if (rerun && state.mounted && state.currentWorkspaceId === workspaceId) {
      void refreshProgress(workspaceId, state, setProgress);
    }
  }
}

function canCommitProgress(state: RefreshState, workspaceId: string, generation: number) {
  return (
    state.latestGenerations.get(workspaceId) === generation &&
    state.mounted &&
    state.currentWorkspaceId === workspaceId
  );
}

function commitFailOpenProgress(
  state: RefreshState,
  workspaceId: string,
  generation: number,
  setProgress: ProgressSetter,
) {
  if (!canCommitProgress(state, workspaceId, generation)) return;
  setProgress((current) =>
    current.has(workspaceId) ? current : withProgress(current, workspaceId, FAIL_OPEN_PROGRESS),
  );
}

function withProgress(
  current: Map<string, OnboardingProgress>,
  workspaceId: string,
  progress: OnboardingProgress,
) {
  const next = new Map(current);
  next.set(workspaceId, progress);
  return next;
}

function useRefreshTriggers({
  dismissed,
  pollingMode,
  refresh,
  workspaceId,
}: RefreshTriggers) {
  useEffect(() => {
    if (dismissed) return;

    const unsubscribe = subscribeOnboardingRefresh(workspaceId, () => void refresh());
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);

    const timer = window.setInterval(() => {
      const pollingSurfaceVisible =
        pollingMode === "mounted" || window.matchMedia(DESKTOP_SIDEBAR_MEDIA_QUERY).matches;
      if (pollingSurfaceVisible && document.visibilityState === "visible") void refresh();
    }, PROGRESS_REFRESH_INTERVAL_MS);

    return () => {
      unsubscribe();
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, [dismissed, pollingMode, refresh, workspaceId]);
}
