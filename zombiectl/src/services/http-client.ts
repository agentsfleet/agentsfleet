// HttpClient service — wraps apiRequestWithRetry from lib/http.ts.
// Returns Effects whose error channel carries NetworkError or
// ServerError (no raw ApiError leaks). Retry behaviour and
// Retry-After honoring are unchanged from the existing transport
// (lib/http.ts is reused verbatim).
//
// Authorization is opt-in per request via the `token` option; the
// Credentials service is responsible for reading the on-disk token
// and passing it here as a Redacted value.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import {
  ApiError,
  apiRequestWithRetry,
  authHeaders,
  type FetchImpl,
  type RetryConfig,
} from "../lib/http.ts";
import { CliConfig } from "./config.ts";
import { NetworkError, ServerError } from "../errors/index.ts";

export interface HttpRequestInput {
  readonly path: string;
  readonly method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  readonly headers?: Record<string, string>;
  readonly body?: unknown;
  readonly token?: Redacted.Redacted<string> | undefined;
  readonly retry?: RetryConfig | null;
  readonly timeoutMs?: number;
}

export interface HttpClientShape {
  readonly request: <T = unknown>(
    input: HttpRequestInput,
  ) => Effect.Effect<T, NetworkError | ServerError>;
}

export class HttpClient extends Context.Service<HttpClient, HttpClientShape>()(
  "zombiectl/runtime/HttpClient",
) {}

const isFetchFailed = (cause: unknown): boolean =>
  cause instanceof TypeError &&
  typeof cause.message === "string" &&
  cause.message.toLowerCase().includes("fetch failed");

const toCliError = (
  url: string,
  cause: unknown,
): NetworkError | ServerError => {
  if (cause instanceof ApiError) {
    const status = cause.status ?? 0;
    if (status >= 500 || status === 0) {
      return new ServerError({
        detail: cause.message,
        suggestion: "retry; if the error persists, capture the request_id and contact support",
        code: cause.code ?? `HTTP_${status}`,
        status,
        requestId: cause.requestId ?? null,
      });
    }
    return new ServerError({
      detail: cause.message,
      suggestion: status === 401 || status === 403
        ? "re-authenticate with `zombiectl login`"
        : "verify the request payload and retry",
      code: cause.code ?? `HTTP_${status}`,
      status,
      requestId: cause.requestId ?? null,
    });
  }
  if (isFetchFailed(cause)) {
    return new NetworkError({
      detail: `cannot reach usezombie API at ${url}`,
      suggestion: "check network connectivity, ZOMBIE_API_URL, and any proxy/VPN settings",
      url,
    });
  }
  return new NetworkError({
    detail: cause instanceof Error ? cause.message : String(cause),
    suggestion: "retry; if the error persists, capture the output and contact support",
    url,
  });
};

const buildHeaders = (
  base: Record<string, string> | undefined,
  token: Redacted.Redacted<string> | undefined,
): Record<string, string> => {
  const auth = token !== undefined ? authHeaders({ token: Redacted.value(token) }) : { "Content-Type": "application/json" };
  return { ...auth, ...(base ?? {}) };
};

const makeLive = (
  apiUrl: string,
  fetchImpl: FetchImpl | undefined,
): HttpClientShape => ({
  request: <T = unknown>(input: HttpRequestInput): Effect.Effect<T, NetworkError | ServerError> => {
    const url = `${apiUrl.replace(/\/$/, "")}${input.path}`;
    const headers = buildHeaders(input.headers, input.token);
    return Effect.tryPromise({
      try: () =>
        apiRequestWithRetry(url, {
          method: input.method ?? "GET",
          headers,
          ...(input.body !== undefined
            ? { body: typeof input.body === "string" ? input.body : JSON.stringify(input.body) }
            : {}),
          ...(input.retry !== undefined ? { retry: input.retry } : {}),
          ...(input.timeoutMs !== undefined ? { timeoutMs: input.timeoutMs } : {}),
          ...(fetchImpl !== undefined ? { fetchImpl } : {}),
        }) as Promise<T>,
      catch: (cause) => toCliError(url, cause),
    });
  },
});

export const httpClientLayer: Layer.Layer<HttpClient, never, CliConfig> = Layer.effect(
  HttpClient,
  Effect.gen(function* () {
    const config = yield* CliConfig;
    return HttpClient.of(makeLive(config.apiUrl, config.fetchImpl));
  }),
);

// Token resolution helper for command handlers: prefer Credentials over env.
// Returns the Option-wrapped Redacted value the HttpClient request shape consumes.
export const resolveToken = (
  envToken: Option.Option<Redacted.Redacted<string>>,
  storedToken: Option.Option<Redacted.Redacted<string>>,
): Option.Option<Redacted.Redacted<string>> =>
  Option.isSome(storedToken) ? storedToken : envToken;
