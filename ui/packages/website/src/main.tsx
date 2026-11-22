import { lazy, StrictMode, Suspense } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";
import { BrowserRouter, Route, Routes } from "react-router-dom";
import Home from "./pages/Home";
import { PUBLIC_ACCESS_URL } from "./config";
import { initAnalytics } from "./analytics/posthog";

const App = lazy(() => import("./App"));

initAnalytics();

const root = document.getElementById("root")!;
const application = (
  <StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Home waitlistUrl={PUBLIC_ACCESS_URL} />} />
        <Route path="*" element={<Suspense fallback={null}><App /></Suspense>} />
      </Routes>
    </BrowserRouter>
  </StrictMode>
);

if (root.hasChildNodes() && window.location.pathname === "/") {
  hydrateRoot(root, application);
} else {
  createRoot(root).render(application);
}
