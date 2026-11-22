import { StrictMode } from "react";
import { hydrateRoot } from "react-dom/client";
import { initAnalytics } from "../analytics/posthog";
import { PUBLIC_ACCESS_URL } from "../config";
import Home from "../pages/Home";

initAnalytics();

hydrateRoot(
  document.getElementById("root")!,
  <StrictMode>
    <Home waitlistUrl={PUBLIC_ACCESS_URL} />
  </StrictMode>,
);
