import type { ThreadMessageLike } from "@assistant-ui/react";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { RENDER_KIND, RENDER_KIND_KEY } from "@/components/domain/conversation/thread/useFleetThreadEntries";
import { activityTracePart, recordedActivityTrace } from "@/components/domain/activity/replies/fleet-activity-trace";
import { LAUNCH_REPLIES, LAUNCH_REQUEST } from "./launch-review-activity";

/** Durable events have one reply string. Only this example supplies prose boundaries and a named responsive-check result. */
export function launchReviewConversation(event: FleetEvent, answer: ThreadMessageLike, actor: string): ThreadMessageLike[] {
  const trace = recordedActivityTrace(event);
  const integration = trace.findIndex(row => row.kind === "integration");
  const checks = event.tools?.find(tool => tool.name === "exec_command" && tool.args?.cmd === "bun run test:responsive");
  const layout = checks?.done && checks.status === "succeeded" && checks.exitCode === 0 && checks.outputHead
    ? [{kind: "check" as const, label: "Checked desktop and mobile layouts", detail: checks.outputHead}] : [];
  const raw = typeof answer.content === "string" ? [] : answer.content.filter(part => part.type === "tool-call");
  return [
    {id: "launch-request", role: "user", createdAt: event.createdAt, content: LAUNCH_REQUEST,
      attachments: [
        {id: "launch-brief", type: "file", name: "launch-brief.md", contentType: "text/markdown", status: {type: "complete"}, content: []},
        {id: "mobile-reference", type: "image", name: "mobile-reference.png", contentType: "image/png", status: {type: "complete"}, content: []},
      ], metadata: {custom: {[RENDER_KIND_KEY]: RENDER_KIND.TRIGGER, actor}}},
    {...answer, content: [{type: "text", text: LAUNCH_REPLIES[0]}, activityTracePart(trace.slice(0, integration)),
      {type: "text", text: LAUNCH_REPLIES[1]}, activityTracePart([...trace.slice(integration), ...layout]),
      {type: "text", text: LAUNCH_REPLIES[2]}, ...raw]},
  ];
}
