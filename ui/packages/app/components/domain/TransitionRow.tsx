import { cn } from "@usezombie/design-system";
import { formatDate } from "@/lib/utils";

type Props = {
  fromStatus: string | null;
  toStatus: string;
  reason: string;
  actor: string;
  createdAt: string;
};

/*
 * TransitionRow — one entry in the run audit trail. Domain-specific:
 * mono status transition (from → to) on the left, reason + actor + time
 * on the right. Bottom-border divider with the last row excluded.
 */
export default function TransitionRow({
  fromStatus,
  toStatus,
  reason,
  actor,
  createdAt,
}: Props) {
  return (
    <div
      className={cn(
        "flex items-start justify-between gap-4 border-b border-border/50 py-3 last:border-b-0",
      )}
    >
      <div className="flex items-center gap-2 whitespace-nowrap font-mono text-xs">
        {fromStatus ? (
          <>
            <span className="text-muted-foreground">{fromStatus}</span>
            <span className="text-muted-foreground/60">→</span>
          </>
        ) : null}
        <span className="font-medium text-info">{toStatus}</span>
      </div>
      <div className="flex flex-col items-end gap-0.5 text-sm text-muted-foreground">
        <span>{reason}</span>
        <span className="font-mono text-xs text-muted-foreground/60">
          {actor} · {formatDate(createdAt)}
        </span>
      </div>
    </div>
  );
}
