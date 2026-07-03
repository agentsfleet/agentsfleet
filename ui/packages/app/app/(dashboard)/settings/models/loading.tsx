import RouteLoading from "@/components/layout/RouteLoading";

// Without this, /settings/models borrowed the parent settings loader, which
// painted the wrong "Workspace" title on navigation. Match the real Models
// header exactly while the provider + catalogue reads resolve.
export default function ModelsLoading() {
  return (
    <RouteLoading
      title="Models"
      description="The model your fleets run on, and the key behind it."
    />
  );
}
