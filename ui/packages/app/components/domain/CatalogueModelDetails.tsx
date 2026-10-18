"use client";

import { Badge, DescriptionDetails, DescriptionList, DescriptionTerm } from "@agentsfleet/design-system";
import type { LibraryModel } from "@/lib/api/model-library-types";
import { formatContextTokens, formatRatesPerMtok } from "@/lib/models/display";
import { CatalogueEvidence, ModelPriceSchedule } from "./ModelPriceSchedule";

export function CatalogueModelDetails({ model }: { model: LibraryModel }) {
  return <div className="space-y-sm">
    {model.advanced ? <Badge>Advanced model</Badge> : null}
    <DescriptionList>
      <div><DescriptionTerm>Context cap (tokens)</DescriptionTerm>
        <DescriptionDetails mono>{formatContextTokens(model.context_cap_tokens)}</DescriptionDetails></div>
      <div><DescriptionTerm>Input / cached input / output ($/1M)</DescriptionTerm>
        <DescriptionDetails mono>{formatRatesPerMtok(model)}</DescriptionDetails></div>
    </DescriptionList>
    <CatalogueEvidence source={model.context_source_url} verifiedAt={model.context_verified_at} />
    <ModelPriceSchedule prices={model.prices} />
  </div>;
}
