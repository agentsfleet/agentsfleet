import RouteLoading from "@/components/layout/RouteLoading";
import { MODELS_PAGE_DESCRIPTION, MODELS_PAGE_TITLE } from "./copy";

// Without this, /settings/models borrowed the parent settings loader, which
// painted the wrong "Workspace" title on navigation. Match the real Models
// header exactly while the provider + catalogue reads resolve.
export default function ModelsLoading() {
  return (
    <RouteLoading
      title={MODELS_PAGE_TITLE}
      description={MODELS_PAGE_DESCRIPTION}
    />
  );
}
