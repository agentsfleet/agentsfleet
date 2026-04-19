import { cn } from "@usezombie/design-system";

type Props = {
  label: string;
  value: string;
};

/*
 * ArtifactRow — one row in the run-detail artifact list. Domain-specific:
 * a mono-font warning-tinted label on the left + a cyan-tinted code value
 * on the right, inside a bordered card panel.
 */
export default function ArtifactRow({ label, value }: Props) {
  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-sm border border-border bg-card px-3 py-2",
      )}
    >
      <span className="min-w-[120px] font-mono text-xs uppercase tracking-wide text-warning">
        {label}
      </span>
      <code className="text-xs text-info">{value}</code>
    </div>
  );
}
