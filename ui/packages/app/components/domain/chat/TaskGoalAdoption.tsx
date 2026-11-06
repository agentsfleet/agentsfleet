"use client";

import { useEffect, useRef, useState } from "react";
import { z } from "zod";
import { Button, SignalSpinner } from "@agentsfleet/design-system";
import { adoptTaskGoal, type TaskDetail } from "@/lib/api/tasks";
import { setupRequest } from "@/lib/api/draft-request";
import { readDecisionPolicy, type DecisionPolicy } from "@/lib/api/decision-policy";
import { fleetPath } from "@/lib/api/fleet-paths";
import { mintOperationId } from "@/lib/streaming/operation-id";
import { QuestionDetails } from "./SetupQuestions";

const Saved = z.object({setup_revision_id: z.uuidv7(), setup_revision: z.string().regex(/^[1-9][0-9]*$/u),
  definition: z.object({config: z.unknown()})});
type Preview = {revisionId: string; revision: string; policy: DecisionPolicy; operation: string};
type State = {kind: "idle"} | {kind: "reading"} | {kind: "preview"; value: Preview}
  | {kind: "applying"; value: Preview} | {kind: "notice"; text: string};
type Props = {workspaceId: string; fleetId: string; detail: TaskDetail; enabled: boolean; onChanged: () => void};

export function TaskGoalAdoption({workspaceId, fleetId, detail, enabled, onChanged}: Props) {
  const [state, setState] = useState<State>({kind: "idle"});
  const active = useRef<AbortController | null>(null);
  useEffect(() => () => active.current?.abort(), []);
  if (["done", "cancelled", "exhausted"].includes(detail.context.state)) return null;
  const review = async (): Promise<void> => {
    if (active.current || !enabled) return;
    const pending = new AbortController();
    active.current = pending;
    setState({kind: "reading"});
    try {
      const saved = await setupRequest({path: `/live${fleetPath(workspaceId, fleetId)}`}, Saved, pending.signal);
      if (saved.setup_revision_id === detail.context.accepted_revision_id) {
        setState({kind: "notice", text: "This work already uses the latest saved success criteria."});
      } else {
        const read = readDecisionPolicy(saved.definition.config);
        setState(read.kind === "ready" ? {kind: "preview", value: {revisionId: saved.setup_revision_id,
          revision: saved.setup_revision, policy: read.policy, operation: mintOperationId()}}
          : {kind: "notice", text: "The latest setup has no readable success criteria. Review its setup first."});
      }
    } catch {if (!pending.signal.aborted) setState({kind: "notice", text: "Saved criteria could not be read. Try again after refreshing progress."});}
    finally {active.current = null;}
  };
  const apply = async (preview: Preview): Promise<void> => {
    if (active.current || !enabled || !detail.etag) return;
    const pending = new AbortController();
    active.current = pending;
    setState({kind: "applying", value: preview});
    const result = await adoptTaskGoal(workspaceId, fleetId, detail, preview.revisionId, preview.operation, pending.signal);
    active.current = null;
    if (pending.signal.aborted) return;
    if (result.kind === "confirmed") {setState({kind: "idle"}); onChanged();}
    else {setState({kind: "notice", text: "The criteria change could not be confirmed. Refresh progress and review the saved criteria before trying again."});}
  };
  const preview = state.kind === "preview" || state.kind === "applying" ? state.value : null;
  return <div className="space-y-sm" aria-label="Update success criteria">
    <Button variant="ghost" size="sm" disabled={!enabled || state.kind === "reading" || state.kind === "applying"}
      onClick={() => {void review();}}>Review latest saved criteria</Button>
    {state.kind === "reading" ? <output className="inline-flex items-center gap-xs text-eyebrow text-muted-foreground"><SignalSpinner aria-hidden="true" />Reading saved criteria…</output> : null}
    {state.kind === "notice" ? <output className="block text-eyebrow text-muted-foreground">{state.text}</output> : null}
    {preview ? <div className="space-y-sm rounded-md border border-border p-sm">
      <p className="font-medium">Success criteria from setup v{preview.revision}</p>
      {preview.policy.completion.statements.map((statement, index) => <QuestionDetails key={statement.id} number={index + 1} question={statement} />)}
      <p className="text-eyebrow text-muted-foreground">Use these criteria for this work. Progress, limits and access stay the same.</p>
      <div className="flex gap-xs">
        <Button variant="ghost" size="sm" disabled={!enabled || !detail.etag || state.kind === "applying"} onClick={() => {void apply(preview);}}>
          {state.kind === "applying" ? "Applying criteria…" : "Use these criteria"}</Button>
        <Button variant="ghost" size="sm" disabled={state.kind === "applying"} onClick={() => setState({kind: "idle"})}>Keep current criteria</Button>
      </div>
    </div> : null}
  </div>;
}
