"use client";

import { useCallback, useState } from "react";
import {
  Toast,
  useResettableTimeout,
  type ToastSeverity,
} from "@agentsfleet/design-system";

const WORKSPACE_NOTICE_MS = 2800;

export type WorkspaceNoticeValue = {
  message: string;
  severity: ToastSeverity;
};

type WorkspaceNoticeState = {
  value: WorkspaceNoticeValue;
  visible: boolean;
};

export function useWorkspaceNotice() {
  const [notice, setNotice] = useState<WorkspaceNoticeState | null>(null);
  const timer = useResettableTimeout();
  const showNotice = useCallback(
    (severity: ToastSeverity, message: string) => {
      const value = { severity, message };
      setNotice({ value, visible: true });
      timer.start(
        () => setNotice({ value, visible: false }),
        WORKSPACE_NOTICE_MS,
      );
    },
    [timer],
  );
  return { notice, showNotice };
}

export function WorkspaceNotice({ notice }: { notice: WorkspaceNoticeState | null }) {
  return (
    <Toast
      visible={notice?.visible ?? false}
      severity={notice?.value.severity ?? "info"}
      className="pointer-events-none fixed right-4 top-16 z-50 max-w-sm"
      data-testid="workspace-toast"
    >
      {notice?.value.message ?? ""}
    </Toast>
  );
}
