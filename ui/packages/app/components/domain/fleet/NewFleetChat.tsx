"use client";

import type { ThreadMessage } from "@assistant-ui/react";
import type { WorkspaceModel } from "@/lib/api/workspace-models";
import { useMemo, useState } from "react";
import { AssistantRuntimeProvider, ThreadPrimitive, useExternalStoreRuntime } from "@assistant-ui/react";
import { GitPullRequestIcon, TerminalWindowIcon, UsersIcon } from "@phosphor-icons/react/ssr";
import { Alert, BrandMark, Button, PageTitle } from "@agentsfleet/design-system";
import starters from "@/public/fleet-starters.json" with { type: "json" };
import { runtimeModelLabel } from "@/lib/api/workspace-models";
import { ChatThread } from "../shared/chat/ChatThread";
import { SteerComposer } from "../conversation/composer/SteerComposer";
import { FleetModelContext, type FleetModelChoice } from "../models/FleetModelContext";
import { useFleetCreation } from "./useFleetCreation";

const STARTERS: readonly {title: string; prompt: string}[] = starters;
/** With no model chosen the daemon uses the workspace's selected model. */
const WORKSPACE_MODEL = "Workspace model";
const NO_MESSAGES: readonly ThreadMessage[] = [];
const ignore = (): void => undefined;

/** New fleet: the first message creates the fleet and its first turn, then the page becomes that fleet's chat. */
export function NewFleetChat({workspace, models}: {workspace: string; models: readonly WorkspaceModel[]}) {
  const [model, setModel] = useState<string | null>(null);
  const creation = useFleetCreation(workspace, model);
  const choice = useMemo<FleetModelChoice>(() => ({
    models, selected: model, label: model === null ? WORKSPACE_MODEL : runtimeModelLabel(models, model),
    changing: false, error: null, select: setModel,
  }), [models, model]);
  const adapter = useMemo(() => ({messages: NO_MESSAGES, isRunning: creation.creating, onNew: creation.onNew}),
    [creation.creating, creation.onNew]);
  const runtime = useExternalStoreRuntime(adapter);
  // The marker gives New fleet the fleet chat's edge-to-edge canvas (`app/globals.css`).
  return <AssistantRuntimeProvider runtime={runtime}><FleetModelContext value={choice}><div data-fleet-workspace className="flex min-h-0 flex-1 flex-col">
    <ChatThread label="New fleet" centered footer={<>
      {creation.error ? <Alert variant="destructive" className="mb-sm">{creation.error}</Alert> : null}
      <SteerComposer creating modelLabel={choice.label} pending={creation.pending} onResend={creation.resend}
        onDismiss={creation.dismiss} onRestored={ignore} onDraft={ignore} />
      {creation.creating ? <output className="mt-sm text-center text-label text-muted-foreground">Starting your fleet…</output> : null}
      <div className="mt-5 flex w-full flex-wrap justify-start gap-lg sm:justify-center">
        {STARTERS.map(({title, prompt}, index) => <ThreadPrimitive.Suggestion key={title} prompt={prompt} send={false} asChild>
          <Button variant="outline" size="sm" disabled={creation.creating} className="min-h-9.5 rounded-full border-border bg-transparent text-eyebrow font-semibold text-muted-foreground">{index === 0 ? <TerminalWindowIcon size={16} /> : index === 1 ? <GitPullRequestIcon size={16} /> : <UsersIcon size={16} />}{title}</Button>
        </ThreadPrimitive.Suggestion>)}
      </div>
      <p className="mb-xl mt-5 text-center text-eyebrow leading-label text-text-dim">No fleet selected. Start something new, or open your fleets from the rail.</p>
    </>}>
      <div role="log" aria-live="off" className="mx-auto flex w-full max-w-chat flex-1 flex-col">
        <div className="content-entrance flex w-full flex-col items-center text-center">
          <BrandMark viewBox="-4 -4 32 32" className="mb-2xl size-11 rounded-brand bg-brand-tile text-brand-ray" aria-hidden="true" />
          <PageTitle className="mb-lg text-new-fleet-title leading-new-fleet-title tracking-new-fleet-title">What should your fleet do?</PageTitle>
          <p className="text-new-fleet-description text-muted-foreground">Give it a goal. Follow the work. Step in when you’re needed.</p>
        </div>
      </div>
    </ChatThread>
  </div></FleetModelContext></AssistantRuntimeProvider>;
}
