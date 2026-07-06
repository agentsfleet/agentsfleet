import RouteLoading from "@/components/layout/RouteLoading";

// Paints the correct Integrations header instantly on navigation while the
// connector status + stored-secret reads resolve, so the header never flashes a
// wrong/empty title.
export default function IntegrationsLoading() {
  return (
    <RouteLoading
      title="Integrations"
      description="Connect the tools your fleets act through."
    />
  );
}
