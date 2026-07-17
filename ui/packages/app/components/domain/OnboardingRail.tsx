import Link from "next/link";
import { cn, EYEBROW_CLASS } from "@agentsfleet/design-system";
import { CheckIcon } from "lucide-react";
import type { OnboardingStep } from "@/lib/onboarding";
import { workspacePath } from "@/lib/workspace-routes";

type Props = {
  workspaceId: string;
  steps: OnboardingStep[];
  // The widget renders the same rail at a tighter density; the page renders it
  // roomy with hints. Both read the same steps, so they cannot disagree.
  compact?: boolean;
};

// The tick rail — the ONE renderer the page checklist and the sidebar widget
// both use (design pick: variant B). A 1px rail threads every marker; a done
// step is a checked mint marker with a struck-through label, the next incomplete
// step carries a small centre dot (never the wake-pulse animation — that stays
// exclusive to live entities), and future steps are hollow and muted. The
// optional step sits after a rail gap behind an OPTIONAL eyebrow.
export default function OnboardingRail({ workspaceId, steps, compact }: Props) {
  return (
    <ol className="relative">
      <span
        aria-hidden="true"
        className="absolute left-[5px] top-1 bottom-1 w-px bg-border"
      />
      {steps.map((step, i) => {
        const prev = steps[i - 1];
        const firstOptional = !step.required && (prev === undefined || prev.required);
        return (
          <li
            key={step.id}
            className={cn("relative", firstOptional && "mt-3 pt-3")}
          >
            <RailRow workspaceId={workspaceId} step={step} compact={compact} />
          </li>
        );
      })}
    </ol>
  );
}

function RailRow({
  workspaceId,
  step,
  compact,
}: {
  workspaceId: string;
  step: OnboardingStep;
  compact?: boolean;
}) {
  const marker = <RailMarker step={step} />;
  const label = (
    <span
      className={cn(
        "font-mono",
        compact ? "text-label" : "text-body-sm",
        step.done && "line-through text-text-subtle",
        !step.done && step.isNext && "text-text",
        !step.done && !step.isNext && "text-muted-foreground",
      )}
    >
      {step.label}
    </span>
  );

  const body = (
    <span className="flex flex-col gap-1">
      <span className="flex items-center gap-2">
        {step.required ? null : (
          <span className={cn(EYEBROW_CLASS, "text-text-subtle")}>optional</span>
        )}
        {label}
      </span>
      {!compact && !step.done ? (
        <span className="text-body-sm text-muted-foreground">{step.hint}</span>
      ) : null}
    </span>
  );

  const inner = (
    <span className={cn("grid grid-cols-[16px_1fr] items-start", compact ? "gap-2 py-1" : "gap-3 py-2")}>
      <span className="relative z-10 flex justify-center pt-1">{marker}</span>
      {body}
    </span>
  );

  // A step with a destination links there; one without (it completes by activity
  // elsewhere) is inert text. The next-step ring already draws the eye, so the
  // link is an affordance, not the only signal.
  if (step.href) {
    return (
      <Link
        href={workspacePath(workspaceId, step.href)}
        className="block rounded-sm hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        {inner}
      </Link>
    );
  }
  return inner;
}

function RailMarker({ step }: { step: OnboardingStep }) {
  if (step.done) {
    return (
      <span
        aria-label="done"
        className="inline-flex w-3 h-3 items-center justify-center rounded-full bg-pulse text-on-pulse"
      >
        <CheckIcon size={10} aria-hidden="true" />
      </span>
    );
  }
  if (step.isNext) {
    return (
      <span
        aria-label="next step"
        className="inline-flex w-3 h-3 items-center justify-center rounded-full border border-pulse bg-background"
      >
        <span data-current-step="true" className="w-1 h-1 rounded-full bg-pulse" />
      </span>
    );
  }
  return (
    <span
      aria-label="pending"
      className="inline-block w-3 h-3 rounded-full border border-text-subtle bg-background"
    />
  );
}
