import { lazy } from "react";
import { Route, Routes, ScrollRestoration } from "react-router-dom";
import { SiteLayout } from "./components/SiteLayout";
import { PUBLIC_ACCESS_URL } from "./config";

const Examples = lazy(() => import("./pages/Examples"));
const Fleets = lazy(() => import("./pages/Fleets"));
const Privacy = lazy(() => import("./pages/Privacy"));
const Terms = lazy(() => import("./pages/Terms"));
const NotFound = lazy(() => import("./pages/NotFound"));
const DesignSystemGallery = import.meta.env.DEV
  ? lazy(() => import("./pages/DesignSystemGallery"))
  : null;

export default function App() {
  return (
    <SiteLayout waitlistUrl={PUBLIC_ACCESS_URL}>
      <ScrollRestoration />
      <Routes>
        <Route path="/examples" element={<Examples />} />
        <Route path="/agents" element={<Fleets />} />
        <Route path="/privacy" element={<Privacy />} />
        <Route path="/terms" element={<Terms />} />
        {DesignSystemGallery && (
          <Route path="/_design-system" element={<DesignSystemGallery />} />
        )}
        <Route path="*" element={<NotFound />} />
      </Routes>
    </SiteLayout>
  );
}
