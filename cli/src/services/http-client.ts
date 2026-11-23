// HttpClient service — wraps apiRequest from lib/http.ts.
// Returns Effects whose error channel carries NetworkError or
// ServerError (no raw ApiError leaks). Each request is sent once.
//
// Authorization is opt-in per request via the `token` option; the
// Credentials service is responsible for reading the on-disk token
// and passing it here as a Redacted value.

import { Effect, Layer, Option, Redacted, Context } from "effect";
import {
  ApiError,
  apiRequest,
  authHeaders,
  readProblemDetails,
  type FetchImpl,
} from "../lib/http.ts";
import { DROP_CODES, PRE_SEND_CODES, isTransientResponse, socketCode } from "../lib/http-failure.ts";
import { CliConfig } from "./config.ts";
import { NetworkError, ServerError } from "../errors/index.ts";
import { isString } from "../lib/guards.ts";
import { HTTP_METHOD, type HttpMethod } from "../constants/http-method.ts";


export interface HttpRequestInput {
  readonly path: string;
  readonly method?: HttpMethod;
  readonly headers?: Record<string, string>;
  readonly body?: unknown;
  readonly token?: Redacted.Redacted<string> | undefined;
  readonly timeoutMs?: number;
  readonly signal?: AbortSignal | undefined;
}

export interface HttpClientShape {
  readonly request: <T = unknown>(
    input: HttpRequestInput,
  ) => Effect.Effect<T, NetworkError | ServerError>;
}

export type HttpClient = HttpClientShape;
export const HttpClient = Context.Service<HttpClient>(
  "agentsfleet/runtime/HttpClient",
);

// Node's fetch says "fetch failed"; Bun says what happened and puts the code
// on the error. Both mean the connection never carried the request, and the
// operator is owed the same suggestion. A code that is not about the
// connection — an invalid URL, a certificate the client rejected — is not
// this, and keeps its own message: telling someone to check their proxy when
// the real answer is "that certificate is not the one I expected" hides the
// one error worth reading closely.
const reachabilityCode = (cause: TypeError): boolean => {
  const code = socketCode(cause);
  return code !== undefined && (DROP_CODES.has(code) || PRE_SEND_CODES.has(code));
};

const isFetchFailed = (cause: unknown): boolean => {
  if (!(cause instanceof TypeError)) return false;
  return socketCode(cause) === undefined
    ? cause.message.toLowerCase().includes("fetch failed")
    : reachabilityCode(cause);
};

export const ERR_WORKSPACE_NAME_EXISTS = "UZ-WORKSPACE-001" as const;
const ERR_INSUFFICIENT_SCOPE = "UZ-AUTH-022";
const WORKSPACE_NAME_EXISTS_SUGGESTION =
  "run `agentsfleet workspace list`, then `agentsfleet workspace use <workspace_id>`, or choose another name";

const apiErrorSuggestion = (cause: ApiError, status: number): string => {
  if (cause.code === ERR_INSUFFICIENT_SCOPE) {
    return "ask your administrator to grant the required permissions";
  }
  if (cause.code === ERR_WORKSPACE_NAME_EXISTS) {
    return WORKSPACE_NAME_EXISTS_SUGGESTION;
  }
  return status === 401 || status === 403
    ? "re-authenticate with `agentsfleet login`; if you logged into a different server, check the target API URL (--api / AGENTSFLEET_API_URL)"
    : "verify the request payload and retry";
};

// The daemon writes a `user_message` for a person and a `detail` for a log.
// The CLI rendered the log one under the ✕ glyph, so an operator read
// a storage diagnostic where the daemon had already written the sentence
// naming what to fix. Prefer the human one wherever it was sent.
const renderedDetail = (cause: ApiError): string =>
  readProblemDetails(cause.body).userMessage ?? cause.message;

const toCliError = (
  url: string,
  cause: unknown,
): NetworkError | ServerError => {
  if (cause instanceof ApiError) {
    const status = cause.status ?? 0;
    if (status >= 500 || status === 0) {
      return new ServerError({
        transient: isTransientResponse(cause),
        detail: renderedDetail(cause),
        suggestion:
          "retry; if the error persists, capture the request_id and contact support",
        code: cause.code ?? `HTTP_${status}`,
        status,
        requestId: cause.requestId ?? null,
      });
    }
    return new ServerError({
      transient: isTransientResponse(cause),
      detail: renderedDetail(cause),
      suggestion: apiErrorSuggestion(cause, status),
      code: cause.code ?? `HTTP_${status}`,
      status,
      requestId: cause.requestId ?? null,
    });
  }
  if (isFetchFailed(cause)) {
    return new NetworkError({
      transient: true,
      detail: `cannot reach agentsfleet API at ${url}`,
      suggestion:
        "check network connectivity, AGENTSFLEET_API_URL, and any proxy/VPN settings",
      url,
    });
  }
  return new NetworkError({
    detail: cause instanceof Error ? cause.message : String(cause),
    suggestion:
      "retry; if the error persists, capture the output and contact support",
    url,
  });
};

const buildHeaders = (
  base: Record<string, string> | undefined,
  token: Redacted.Redacted<string> | undefined,
): Record<string, string> => {
  const auth =
    token !== undefined
      ? authHeaders({ token: Redacted.value(token) })
      : { "Content-Type": "application/json" };
  return { ...auth, ...(base ?? {}) };
};

// A request that is slow or flaky is the thing an operator needs to see, and
// until now there was nothing to see: `--log-level` validated a level and then
// governed no records, because this package emitted none.
//
// The path is reported with its identifiers replaced, so a record names the
// endpoint rather than which fleet someone was looking at. The token never
// appears: it lives in a header this function is not handed.
// The boundary accepts what can FOLLOW an identifier in a request path: the
// next segment, the end of the path, or the query string. Without the last of
// those a path ending in an identifier keeps it whenever a caller appends a
// query — the redaction would hold everywhere except the one shape a reader
// would never think to check.
const ID_SEGMENT = /\/[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}(?=[/?]|$)/g;
const ID_PLACEHOLDER = "/{id}";
// A query VALUE is a row identifier as often as a path segment is, and it does
// not have to look like a UUID to be one: `starting_after` carries an opaque
// cursor, and a memory cursor carries the memory key. Names are what a reader
// needs — which parameters a request sent — so the names stay and the values
// go. A bare flag with no `=` carries no value and is left alone.
const QUERY_SEPARATOR = "?";
const QUERY_PAIR_SEPARATOR = "&";
const QUERY_ASSIGNMENT = "=";
const QUERY_VALUE_PLACEHOLDER = "{value}";
const TRACE_ATTEMPT = "http.attempt";
const STATUS_NONE = "none";

/**
 * The endpoint a record names, with every identifier in it replaced.
 *
 * Exported because it is the redaction boundary: a record must name the route
 * somebody called, never the row they were looking at, and that promise is
 * worth asserting directly rather than only through whichever paths today's
 * commands happen to build.
 */
export const endpointOf = (path: string): string => {
  const split = path.indexOf(QUERY_SEPARATOR);
  const route = (split === -1 ? path : path.slice(0, split)).replace(ID_SEGMENT, ID_PLACEHOLDER);
  if (split === -1) return route;
  const named = path
    .slice(split + 1)
    .split(QUERY_PAIR_SEPARATOR)
    .map((pair) => {
      const assigned = pair.indexOf(QUERY_ASSIGNMENT);
      return assigned === -1
        ? pair
        : `${pair.slice(0, assigned)}${QUERY_ASSIGNMENT}${QUERY_VALUE_PLACEHOLDER}`;
    })
    .join(QUERY_PAIR_SEPARATOR);
  return `${route}${QUERY_SEPARATOR}${named}`;
};

const attemptRecord = (method: string, path: string, status: number | undefined, durationMs: number): string =>
  `${TRACE_ATTEMPT} method=${method} endpoint=${endpointOf(path)} ` +
  `status=${status ?? STATUS_NONE} duration_ms=${durationMs}`;

const makeLive = (
  apiUrl: string,
  fetchImpl: FetchImpl | undefined,
): HttpClientShape => ({
  request: <T = unknown>(
    input: HttpRequestInput,
  ): Effect.Effect<T, NetworkError | ServerError> => {
    const url = `${apiUrl.replace(/\/$/, "")}${input.path}`;
    const headers = buildHeaders(input.headers, input.token);
    const body =
      input.body === undefined
        ? undefined
        : isString(input.body)
          ? input.body
          : JSON.stringify(input.body);
    const method = input.method ?? HTTP_METHOD.get;
    // Collected during the request, emitted after it settles: the hooks are
    // plain callbacks inside a promise, so logging from them would escape the
    // fiber whose level `--log-level` set.
    const trace: string[] = [];
    return Effect.tryPromise({
      try: async (signal) => {
        const startedAt = Date.now();
        try {
          const result = await apiRequest(url, {
            method,
            signal: input.signal ? AbortSignal.any([input.signal, signal]) : signal,
            headers,
            ...(body !== undefined ? { body } : {}),
            ...(input.timeoutMs !== undefined
              ? { timeoutMs: input.timeoutMs }
              : {}),
            ...(fetchImpl !== undefined ? { fetchImpl } : {}),
          });
          trace.push(attemptRecord(method, input.path, 200, Date.now() - startedAt));
          return result as T;
        } catch (err) {
          const status = err instanceof ApiError ? err.status : undefined;
          trace.push(attemptRecord(method, input.path, status, Date.now() - startedAt));
          throw err;
        }
      },
      catch: (cause) => toCliError(url, cause),
    }).pipe(
      // ensuring, not tap: a request that failed is the one worth reading.
      Effect.ensuring(
        Effect.forEach(trace, (line) => Effect.logDebug(line), { discard: true }),
      ),
    );
  },
});

export const httpClientLayer: Layer.Layer<HttpClient, never, CliConfig> =
  Layer.effect(
    HttpClient,
    Effect.gen(function* () {
      const config = yield* CliConfig;
      return HttpClient.of(makeLive(config.apiUrl, config.fetchImpl));
    }),
  );

// Token resolution helper for command handlers. Precedence: the env-sourced
// service API key (AGENTSFLEET_API_KEY) wins over the stored login JWT — a
// machine that explicitly exports a key means to act as that principal,
// overriding whatever `agentsfleet login` left on disk.
// Returns the Option-wrapped Redacted value the HttpClient request shape consumes.
export const resolveToken = (
  envToken: Option.Option<Redacted.Redacted<string>>,
  storedToken: Option.Option<Redacted.Redacted<string>>,
): Option.Option<Redacted.Redacted<string>> =>
  Option.isSome(envToken) ? envToken : storedToken;
