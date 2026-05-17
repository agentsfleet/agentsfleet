import { AlertTriangleIcon } from "lucide-react";
import { Badge, formatTimeAbsolute } from "@usezombie/design-system";

type Props = { exhaustedAt: number | null };

export default function ExhaustionBadge({ exhaustedAt }: Props) {
  const when = exhaustedAt ? formatTimeAbsolute(new Date(exhaustedAt)) : null;
  return (
    <Badge
      variant="destructive"
      role="status" // oxlint-disable-line jsx-a11y/prefer-tag-over-role -- Badge is the design-system primitive; <output> drops icon children in happy-dom@20.
      aria-label="Balance exhausted"
      title={when ? `Exhausted since ${when}` : "Balance exhausted"}
    >
      <AlertTriangleIcon size={12} aria-hidden="true" />
      Balance exhausted
    </Badge>
  );
}
