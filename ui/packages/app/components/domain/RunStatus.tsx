import { Badge, cn, type BadgeVariant } from "@usezombie/design-system";
import type { RunStatus as RunStatusType } from "@/lib/types";

type Props = {
  status: RunStatusType | string;
  size?: "sm" | "md";
};

type StatusKind = "queued" | "running" | "done" | "failed" | "pending";

const STATUS_CONFIG: Record<string, { label: string; kind: StatusKind }> = {
  SPEC_QUEUED: { label: "Queued", kind: "queued" },
  RUN_PLANNED: { label: "Planned", kind: "queued" },
  PATCH_IN_PROGRESS: { label: "Patching", kind: "running" },
  VERIFICATION_IN_PROGRESS: { label: "Verifying", kind: "running" },
  PR_PREPARED: { label: "PR Ready", kind: "running" },
  PR_OPENED: { label: "PR Open", kind: "running" },
  NOTIFIED: { label: "Notified", kind: "done" },
  DONE: { label: "Done", kind: "done" },
  FAILED: { label: "Failed", kind: "failed" },
  RETRYING: { label: "Retrying", kind: "running" },
};

const KIND_VARIANT: Record<StatusKind, BadgeVariant> = {
  queued: "amber",
  running: "cyan",
  done: "green",
  failed: "destructive",
  pending: "default",
};

export default function RunStatus({ status, size = "md" }: Props) {
  const cfg = STATUS_CONFIG[status] ?? { label: status, kind: "pending" as const };
  const isPulsing = cfg.kind === "running";
  return (
    <Badge
      variant={KIND_VARIANT[cfg.kind]}
      className={cn(
        size === "sm" && "text-[0.65rem] px-2 py-0.5",
        isPulsing && "animate-pulse",
      )}
    >
      <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden="true" />
      {cfg.label}
    </Badge>
  );
}
