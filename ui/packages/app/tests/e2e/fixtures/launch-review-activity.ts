import type { FleetEvent, FleetToolCall } from "@/lib/streaming/fleet-stream-row";
import type { ToolArgs } from "@/lib/streaming/fleet-stream-tool-trace";
import { launchEditArguments } from "./launch-review-changes";

export const LAUNCH_REVIEW_ID = "launch-review";
export const LAUNCH_REQUEST = "Review the launch page, check mobile, and prepare the changes. Include a UI review.";
export const LAUNCH_REPLIES = [
  "I’ll check the layout and mobile behavior, then bring back the changes for your review.",
  "The mobile layout had cramped labels. I widened the reading area and made the action buttons easier to find.",
  "**The changes are ready.** Review the files below, or open the [Launch preview](https://preview.example.test/launch) in Summary.",
] as const;

/** Modeled tool records for the design example. No network, file or process operation runs here. */
export function launchReviewEvent(started: number): FleetEvent {
  const tools: FleetToolCall[] = [];
  function record(name: string, args: ToolArgs, outputHead: string, exitCode?: number) {
    const index = tools.length;
    tools.push({callId: `launch-tool-${index + 1}`, name, args, outputHead, outputLineCount: outputHead.split("\n").length,
      status: "succeeded", done: true, ms: 1200, startedAtMs: started - 138_000 + index * 2800, exitCode});
  }
  record("file_read", {path: "LaunchPage.tsx"}, "<main className=\"launch\">…</main>");
  record("file_read", {path: "theme.css"}, ".launch { gap: 4px; }");
  record("file_read", {path: "launch-brief.md"}, "Review text, keyboard focus and spacing at desktop and mobile widths.");
  record("image", {path: "desktop-reference.png"}, "Attached desktop-reference.png (7320 bytes, image/png) for your next turn");
  record("image", {path: "mobile-reference.png"}, "Attached mobile-reference.png (4182 bytes, image/png) for your next turn");
  record("spawn", {task: "UI reviewer"}, '{"child_id":7}');
  record("file_edit", launchEditArguments(0), "Edited src/components/LaunchPage.tsx");
  record("file_edit", launchEditArguments(1), "Edited src/styles/theme.css");
  record("file_edit", launchEditArguments(2), "Edited src/components/MobileNav.tsx");
  record("file_edit", launchEditArguments(3), "Edited src/components/UsageTable.tsx");
  record("file_edit", launchEditArguments(4), "Edited design-review.md");
  record("file_edit", launchEditArguments(5), "Edited launch-checklist.md");
  record("exec_command", {cmd: "bun run build"}, "Build succeeded", 0);
  record("exec_command", {cmd: "bun run lint"}, "Lint succeeded", 0);
  record("exec_command", {cmd: "bun run test:layout"}, "Layout tests passed", 0);
  record("wait_agent", {child_id: 7}, '{"status":"done","answer":"Reviewed text clarity, spacing, and responsive behavior. Two spacing issues were corrected."}');
  record("http_request", {method: "GET", url: "https://api.github.com/repos/agentsfleet/launch-page/commits/HEAD/check-runs"}, '{"total_count":2}');
  record("exec_command", {cmd: "bun run preview -- --host 127.0.0.1"}, "https://preview.example.test/launch\nProcess running with session ID 12");
  record("exec_command", {cmd: "bun run test:responsive"}, "Desktop (1440px): text, focus, overflow passed.\nMobile (390px): text, focus, overflow passed.", 0);
  return {id: LAUNCH_REVIEW_ID, actor: "steer:fixture-owner", role: "user", text: LAUNCH_REQUEST, reply: LAUNCH_REPLIES.join("\n\n"),
    createdAt: new Date(started - 138_000), status: "processed", outcome: "", failureLabel: null, failureDetail: null,
    tokens: null, wallMs: 138_000, costNanos: "340000000", tools, custom: {requestJson: JSON.stringify({message: LAUNCH_REQUEST})}};
}
