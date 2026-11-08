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

// Lifecycle writes are deliberately unavailable in this visual fixture.
export async function setFleetStatusAction() {return {ok: false, error: "Visual fixture does not change fleet state"};}
export async function deleteFleetAction() {return {ok: false, error: "Visual fixture does not delete fleets"};}
export async function listFleetMemoriesAction() {return {ok: false, error: "Visual fixture does not load memories"};}
export async function setMemoryAccessAction() {return {ok: false, error: "Visual fixture does not change memory access"};}
export async function forgetMemoryAction() {return {ok: false, error: "Visual fixture does not remove memories"};}
