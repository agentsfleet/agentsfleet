import { cn } from "@usezombie/design-system";
import type { RunStatus } from "@/lib/types";

type State = "done" | "active" | "failed" | "pending";

type Props = {
  stage: RunStatus;
  state: State;
  showConnector: boolean;
};

const STAGE_LABELS: Record<string, string> = {
  SPEC_QUEUED: "Queue",
  RUN_PLANNED: "Plan",
  PATCH_IN_PROGRESS: "Patch",
  VERIFICATION_IN_PROGRESS: "Verify",
  PR_PREPARED: "PR Ready",
  PR_OPENED: "PR Open",
  DONE: "Done",
};

const STATE_ICONS: Record<State, string> = {
  done: "✓",
  active: "●",
  failed: "✗",
  pending: "○",
};

const DOT_STATE_CLASS: Record<State, string> = {
  done: "border-success text-success bg-success/10",
  active: "border-info text-info bg-info/10",
  failed: "border-destructive text-destructive bg-destructive/10",
  pending: "",
};

export default function PipelineStage({ stage, state, showConnector }: Props) {
  return (
    <>
      <div className="flex flex-col flex-shrink-0 items-center gap-1.5">
        <div
          className={cn(
            "relative z-10 flex h-7 w-7 items-center justify-center rounded-full border-2 border-border bg-card font-mono text-xs transition-colors",
            DOT_STATE_CLASS[state],
          )}
        >
          {STATE_ICONS[state]}
        </div>
        <span className="text-center font-mono text-xs uppercase tracking-wide leading-tight max-w-16 text-muted-foreground">
          {STAGE_LABELS[stage] ?? stage}
        </span>
      </div>
      {showConnector ? (
        <div
          className={cn(
            "h-0.5 w-12 shrink-0 mb-5 transition-colors",
            state === "done" ? "bg-success" : "bg-border",
          )}
        />
      ) : null}
    </>
  );
}
