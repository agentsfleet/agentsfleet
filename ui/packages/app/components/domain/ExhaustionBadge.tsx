import { AlertTriangleIcon } from "lucide-react";
import { formatTimeAbsolute } from "@usezombie/design-system";

type Props = { exhaustedAt: number | null };

export default function ExhaustionBadge({ exhaustedAt }: Props) {
  const when = exhaustedAt ? formatTimeAbsolute(new Date(exhaustedAt)) : null;
  return (
    <span
      role="status"
      aria-label="Balance exhausted"
      title={when ? `Exhausted since ${when}` : "Balance exhausted"}
      className="inline-flex items-center gap-1 rounded-full border border-destructive/40 bg-destructive/10 px-2 py-0.5 text-xs font-medium text-destructive"
    >
      <AlertTriangleIcon size={12} aria-hidden="true" />
      Balance exhausted
    </span>
  );
}
