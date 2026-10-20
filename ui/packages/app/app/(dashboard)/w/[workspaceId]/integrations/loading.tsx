import RouteLoading from "@/components/layout/RouteLoading";
import { CONNECT_APPS_COPY } from "./copy";

// Paints the correct Integrations header instantly on navigation while the
// connector status + stored-secret reads resolve, so the header never flashes a
// wrong/empty title.
export default function IntegrationsLoading() {
  return (
    <RouteLoading
      title={CONNECT_APPS_COPY.title}
      description={CONNECT_APPS_COPY.description}
      layout="cards"
    />
  );
}
