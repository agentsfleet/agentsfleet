import type { RunStatus as RunStatusType } from "@/lib/types";

type Props = {
  status: RunStatusType | string;
  size?: "sm" | "md";
};

const STATUS_CONFIG: Record<string, { label: string; cls: string }> = {
  SPEC_QUEUED:               { label: "Queued",       cls: "status-queued" },
  RUN_PLANNED:               { label: "Planned",      cls: "status-queued" },
  PATCH_IN_PROGRESS:         { label: "Patching",     cls: "status-running" },
  VERIFICATION_IN_PROGRESS:  { label: "Verifying",    cls: "status-running" },
  PR_PREPARED:               { label: "PR Ready",     cls: "status-running" },
  PR_OPENED:                 { label: "PR Open",      cls: "status-running" },
  NOTIFIED:                  { label: "Notified",     cls: "status-done" },
  DONE:                      { label: "Done",         cls: "status-done" },
  FAILED:                    { label: "Failed",       cls: "status-failed" },
  RETRYING:                  { label: "Retrying",     cls: "status-running" },
};

export default function RunStatus({ status, size = "md" }: Props) {
  const cfg = STATUS_CONFIG[status] ?? { label: status, cls: "status-pending" };
  return (
    <span
      className={`status-badge ${cfg.cls}`}
      style={size === "sm" ? { fontSize: "0.65rem", padding: "0.15rem 0.5rem" } : undefined}
    >
      {cfg.label}
    </span>
  );
}
