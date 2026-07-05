"use client";

import { useState } from "react";
import { PageHeader, PageTitle, Section, SectionLabel } from "@agentsfleet/design-system";
import type { AdminModel, AdminModelList } from "@/lib/api/admin_models";
import CatalogueList from "./CatalogueList";
import AddModelDialog from "./AddModelDialog";
import PlatformDefaultCard from "./PlatformDefaultCard";

// One trimmed line — the per-token pricing detail lives in the Create-model
// dialog, not repeated on the page.
const MODELS_DESCRIPTION =
  "Every model your team can run, priced per token — the platform default runs for users without their own key.";

// Single source of truth for the catalogue: the table renders it, the Create
// dialog appends to it, deletes remove from it, and the Platform Default card
// reads it for its model picker — so adding a model rate immediately makes it
// selectable as the default without a round-trip.
export default function ModelsView({ initial }: { initial: AdminModelList }) {
  const [models, setModels] = useState<AdminModel[]>(initial.models);

  return (
    <div className="space-y-8">
      <PageHeader description={MODELS_DESCRIPTION}>
        <PageTitle>Model library</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Model catalogue">
          <div className="flex flex-wrap items-baseline justify-between gap-md">
            <SectionLabel>Manage model library</SectionLabel>
            <AddModelDialog onCreated={(m) => setModels((prev) => [...prev, m])} />
          </div>
          <CatalogueList
            models={models}
            onDeleted={(uid) => setModels((prev) => prev.filter((m) => m.uid !== uid))}
          />
        </section>
      </Section>

      <Section asChild>
        <section aria-label="Platform default" className="mt-10">
          <PlatformDefaultCard models={models} />
        </section>
      </Section>
    </div>
  );
}
