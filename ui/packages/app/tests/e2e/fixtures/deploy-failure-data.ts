import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";

export const DEPLOY_CHECK = "Check the latest deploy and tell me whether the service is healthy.";

/** Native event and tool shapes, with explicit sample observations for the connection failure. */
export function deployFailureEvent(started: number): FleetEvent {
  const created = started - 11_000;
  return {id: "deploy-health-check", actor: "steer:fixture-owner", role: "user", text: DEPLOY_CHECK,
    reply: "I checked the release record. I couldn’t finish the health check because the deployment connection expired.",
    createdAt: new Date(created), status: "fleet_error", outcome: "The response was interrupted",
    failureLabel: "transport_loss", failureDetail: "The deployment connection expired before the health check ran. Reconnect your deploy account, then retry the request.",
    wallMs: 10_000, tokens: 310, costNanos: "80000000", custom: {requestJson: JSON.stringify({message: DEPLOY_CHECK})}, tools: [
      {name: "http_request", callId: "read-release", args: {url: "https://deploy.example.test/releases/latest"},
        done: true, status: "succeeded", outputHead: '{"version":"v2.8.1"}', outputLineCount: 1, startedAtMs: created, ms: 180},
      {name: "http_request", callId: "health-check", args: {url: "https://deploy.example.test/healthz"},
        done: true, status: "failed", outputHead: "401 Unauthorized. The deployment connection expired.", outputLineCount: 1, startedAtMs: created + 180, ms: 200},
    ]};
}
