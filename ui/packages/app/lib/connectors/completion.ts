// A return hint chooses navigation only. The backend still authorizes the
// workspace from signed state; neither this cookie nor the query grants access.
export const CONNECTOR_RETURN_COOKIE = "connector_return_";
export const CONNECTOR_RETURN_MAX_AGE_SECONDS = 600;
export const CONNECTOR_CALLBACK_TIMEOUT_MS = 25_000;
export const CONNECTOR_COMPLETION_PATH = "/connectors/complete";
export const CONNECTOR_FAILURE_COPY = {
  denied: "Authorization was cancelled. You can start a new connection when you are ready.",
  session: "Your session expired. Sign in, then start a new connection from Connect apps.",
  expired: "This connection attempt expired or was already used. Start a new connection from Connect apps.",
  timeout: "The connection took too long to finish. Check its status in Connect apps before starting again.",
  unavailable: "We could not finish the connection. Check its status in Connect apps, then try again if needed.",
} as const;

const RETURN_PATH_PATTERN = /^\/w\/[a-zA-Z0-9_-]{1,128}\/integrations$/;

export function connectorReturnPath(value: string | undefined): string | null {
  return value && RETURN_PATH_PATTERN.test(value) ? value : null;
}

export function connectorFailureCopy(reason: string | undefined): string {
  return reason && Object.hasOwn(CONNECTOR_FAILURE_COPY, reason)
    ? CONNECTOR_FAILURE_COPY[reason as keyof typeof CONNECTOR_FAILURE_COPY]
    : CONNECTOR_FAILURE_COPY.unavailable;
}
