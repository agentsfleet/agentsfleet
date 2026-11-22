import {
  trackNavigationClicked,
  trackSignupStarted,
} from "./posthog";

export function linkTracking(source: string, surface: string, target: string) {
  return {
    "data-source": source,
    onClick() {
      if (target === "signup")
        trackSignupStarted({ source, surface, mode: "humans" });
      else trackNavigationClicked({ source, surface, target });
    },
  };
}
