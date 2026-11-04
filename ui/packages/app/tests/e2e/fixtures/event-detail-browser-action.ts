import type { EventDetail } from "@/lib/api/events";

const READ_TIMEOUT_MS = 5_000;

// A component fixture cannot execute a Next Server Action. The browser test
// supplies its HTTP boundary; production continues to use the server-held token.
export async function getFleetEventAction(workspace: string, fleet: string, event: string) {
  const response = await fetch(`/event-details?${new URLSearchParams({workspace, fleet, event})}`, {signal: AbortSignal.timeout(READ_TIMEOUT_MS)});
  if (!response.ok) return {ok: false as const, error: "Record unavailable"};
  const data: EventDetail = await response.json();
  return {ok: true as const, data};
}
