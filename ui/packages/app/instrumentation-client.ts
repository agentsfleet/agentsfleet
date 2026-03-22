import { initAnalytics, trackAppEvent } from "./lib/analytics/posthog";

try {
  void initAnalytics();

  window.addEventListener("error", (event) => {
    trackAppEvent("ui_runtime_error", {
      source: "window_error",
      surface: "browser_runtime",
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

    trackAppEvent("ui_runtime_error", {
      source: "window_unhandled_rejection",
      surface: "browser_runtime",
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
