import type { ThreadMessageLike } from "@assistant-ui/react";
import { AssistantRuntimeProvider, MessagePrimitive, ThreadPrimitive, useAuiState, useExternalStoreRuntime } from "@assistant-ui/react";
import { useMemo, useState } from "react";
import { Button } from "@agentsfleet/design-system";
import { FleetReply } from "@/components/domain/FleetReplyBody";

const STATES = ["running", "succeeded", "failed", "interrupted", "unconfirmed"] as const;
type State = (typeof STATES)[number];
const startedAt = Date.now();
const github = "https://api.github.com/repos/agentsfleet/linkwarden/pulls";

/** Real presentation/runtime, synthetic recorded parts; controls execute no actions. */
export function ActivityFixture() {
  const [state, setState] = useState<State>("running");
  const messages = useMemo(() => [message(state)], [state]);
  const runtime = useExternalStoreRuntime({messages, isRunning: state === "running",
    convertMessage: (message: ThreadMessageLike) => message, onNew: async () => {}});
  return <main className="flex min-h-dvh flex-col bg-background text-foreground">
    <p className="px-xl py-sm text-eyebrow text-muted-foreground">Activity fixture · deterministic recorded events</p>
    <div className="flex flex-wrap gap-sm px-xl py-sm">{STATES.map(value => <Button key={value} size="sm" variant="outline"
      onClick={() => setState(value)}>Show {value}</Button>)}</div>
    <AssistantRuntimeProvider runtime={runtime}>
      <ThreadPrimitive.Root className="mx-auto w-full max-w-chat px-xl py-3xl">
        <ThreadPrimitive.Messages components={{Message: ActivityMessage}} />
      </ThreadPrimitive.Root>
    </AssistantRuntimeProvider>
  </main>;
}

function ActivityMessage() {
  const message = useAuiState(state => state.message);
  return <MessagePrimitive.Root><FleetReply message={message} senderLabel="CI repairer"
    status={message.status?.type === "running" ? "received" : "processed"} /></MessagePrimitive.Root>;
}

function message(state: State): ThreadMessageLike {
  const running = state === "running";
  const call = (id: string, toolName: string, args: Record<string, string>, status: State): Extract<Exclude<ThreadMessageLike["content"], string>[number], {type: "tool-call"}> => ({
    type: "tool-call", toolCallId: id, toolName, args, argsText: JSON.stringify(args),
    timing: {startedAt, ...(status === "running" ? {} : {completedAt: startedAt + 400})},
    ...(status === "running" ? {} : {result: {status: status === "unconfirmed" ? undefined : status,
      outputHead: status === "failed" ? "[request_policy_not_allowed] The request is outside the reviewed scope." : "Recorded fixture response", outputLineCount: 1}}),
  });
  return {id: "activity-fixture", role: "assistant", createdAt: new Date(startedAt),
    status: running ? {type: "running"} : {type: "complete", reason: "stop"}, content: [
      call("ready", "http_request", {url: "https://api-dev.agentsfleet.net/readyz"}, "succeeded"),
      call("read", "file_read", {path: "/workspace/apps/web/lib/client/unescapeString.ts"}, "succeeded"),
      call("command", "exec_command", {cmd: "bun run typecheck"}, "succeeded"),
      call("pull", "http_request", {url: github, method: "POST"}, state),
      call("write", "file_write", {path: "/workspace/notes.md", content: "Recorded fixture"}, state),
      {type: "text", text: running ? "I’m checking the recorded evidence and preparing the small repair." : "The recorded action has settled. Its outcome appears in the activity above."},
    ]};
}
