import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import App from "./App";
import { initAnalytics } from "./analytics/posthog";
import "./styles.css";

const router = createBrowserRouter([
  {
    path: "*",
    element: <App />,
  },
]);

initAnalytics();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
