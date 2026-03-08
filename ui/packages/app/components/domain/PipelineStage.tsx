import type { RunStatus } from "@/lib/types";

type State = "done" | "active" | "failed" | "pending";

type Props = {
  stage: RunStatus;
  state: State;
  showConnector: boolean;
};

const STAGE_LABELS: Record<string, string> = {
  SPEC_QUEUED:               "Queue",
  RUN_PLANNED:               "Plan",
  PATCH_IN_PROGRESS:         "Patch",
  VERIFICATION_IN_PROGRESS:  "Verify",
  PR_PREPARED:               "PR Ready",
  PR_OPENED:                 "PR Open",
  DONE:                      "Done",
};

const STATE_ICONS: Record<State, string> = {
  done:    "✓",
  active:  "●",
  failed:  "✗",
  pending: "○",
};

export default function PipelineStage({ stage, state, showConnector }: Props) {
  return (
    <>
      <div className={`pipeline-stage ${state}`}>
        <div className="pipeline-stage-dot">{STATE_ICONS[state]}</div>
        <span className="pipeline-stage-label">{STAGE_LABELS[stage] ?? stage}</span>
      </div>
      {showConnector && (
        <div className={`pipeline-connector${state === "done" ? " done" : ""}`} />
      )}
    </>
  );
}
