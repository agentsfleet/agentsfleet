import { ApiError, apiRequest, authHeaders } from "../lib/http.js";

function apiHeaders(ctx) {
  return authHeaders({ token: ctx.token, apiKey: ctx.apiKey });
}

async function request(ctx, reqPath, options = {}) {
  const url = `${ctx.apiUrl}${reqPath}`;
  return apiRequest(url, {
    ...options,
    fetchImpl: ctx.fetchImpl,
  });
}

function printApiError(stderr, err, jsonMode, printJson, writeLine) {
  if (!(err instanceof ApiError)) throw err;
  const payload = {
    error: {
      code: err.code || "API_ERROR",
      message: err.message,
      status: err.status || null,
      request_id: err.requestId || null,
    },
  };
  if (jsonMode) {
    printJson(stderr, payload);
  } else {
    writeLine(stderr, `error: ${payload.error.code} ${payload.error.message}`);
    if (payload.error.request_id) writeLine(stderr, `request_id: ${payload.error.request_id}`);
  }
}

export {
  ApiError,
  apiHeaders,
  printApiError,
  request,
};
