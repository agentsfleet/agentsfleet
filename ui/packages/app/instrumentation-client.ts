import { initAnalytics, trackAppEvent } from "./lib/analytics/posthog";

try {
  void initAnalytics();

  window.addEventListener("error", (event) => {
    trackAppEvent(UI_RUNTIME_ERROR, {
      source: "window_error",
      surface: BROWSER_RUNTIME,
      error_message: event.message || event.error?.message || "Unknown runtime error",
    });
  });

  window.addEventListener("unhandledrejection", (event) => {
    const reason = event.reason;
    const errorMessage =
      typeof reason === "string"
        ? reason
        : reason instanceof Error
          ? reason.message
          : JSON.stringify(reason);

    trackAppEvent(UI_RUNTIME_ERROR, {
      source: "window_unhandled_rejection",
      surface: BROWSER_RUNTIME,
      error_message: errorMessage,
    });
  });
} catch {
  // Instrumentation failures must never impact app startup.
}

export function onRouterTransitionStart(url: string, navigationType: "push" | "replace" | "traverse") {
  trackAppEvent("page_navigation_started", {
    source: "app_router",
    surface: "navigation",
    target: url,
    reason: navigationType,
  });
}
const BROWSER_RUNTIME = "browser_runtime" as const;
const UI_RUNTIME_ERROR = "ui_runtime_error" as const;
