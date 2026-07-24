"use client";

import { useState } from "react";
import { PageHeader, PageLayout, PageTitle, SectionHeader } from "@agentsfleet/design-system";
import type { AdminModel, AdminModelList, PlatformKey } from "@/lib/api/admin_model_library";
import CatalogueList from "./CatalogueList";
import AddModelDialog from "./AddModelDialog";

// One trimmed line — the per-token pricing detail lives in the Create-model
// dialog, not repeated on the page.
const MODELS_DESCRIPTION =
  "Every model your team can run, priced per token — the platform default runs for users without their own key.";

// Single source of truth for the catalogue: the table renders it, the Create
// dialog appends to it, edits update a row in place, and deletes remove from it.
// The platform default is no longer a separate section — a row's ★ action makes
// it the default, and the active one carries a "Default" badge (resolved from
// `activeDefault`, read server-side in page.tsx).
export default function ModelsView({
  initial,
  activeDefault,
}: {
  initial: AdminModelList;
  activeDefault: PlatformKey | null;
}) {
  const [models, setModels] = useState<AdminModel[]>(initial.models);

  return (
    <PageLayout fullHeight className="h-full overflow-hidden">
      <PageHeader description={MODELS_DESCRIPTION}>
        <PageTitle>Model library</PageTitle>
      </PageHeader>

      <div aria-label="Model catalogue" className="flex min-h-0 flex-1 flex-col gap-xl">
        <SectionHeader actions={<AddModelDialog onCreated={(m) => setModels((prev) => [...prev, m])} />}>
          Manage model library
        </SectionHeader>
        <CatalogueList
          models={models}
          activeDefault={activeDefault}
          onDeleted={(uid) => setModels((prev) => prev.filter((m) => m.uid !== uid))}
          onUpdated={(m) => setModels((prev) => prev.map((x) => (x.uid === m.uid ? m : x)))}
        />
      </div>
    </PageLayout>
  );
}
