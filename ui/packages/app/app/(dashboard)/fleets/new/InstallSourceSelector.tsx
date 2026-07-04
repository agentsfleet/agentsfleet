"use client";

import { Button, EmptyState, SectionLabel } from "@agentsfleet/design-system";
import { LayoutTemplateIcon } from "lucide-react";
import type { FleetLibraryGalleryEntry } from "@/lib/types";
import AddTemplateDialog from "./AddTemplateDialog";
import {
  TemplateDocsLink,
  FLEET_LIBRARY_EMPTY_DESCRIPTION,
  FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY,
  FLEET_LIBRARY_EMPTY_TITLE,
} from "./template-docs";
import { TemplateCard } from "./TemplateCard";

type Props = {
  workspaceId: string;
  templates: FleetLibraryGalleryEntry[];
  onUseTemplate: (template: FleetLibraryGalleryEntry) => void;
  canAddTemplate?: boolean;
  /** Open the create-template dialog on first render (?create=1 deep link). */
  initialCreateOpen?: boolean;
};

// Template gallery picker: the workspace's templates (platform ∪ tenant) are the
// install surface. Picking one proceeds inline to the live install states —
// there is no review page. Rendered plainly under the page header (same shape
// as the dashboard's first-run gallery) — the page title/description already
// frame it, so no wrapping panel and no side guide.
export function InstallSourceSelector({
  workspaceId,
  templates,
  onUseTemplate,
  canAddTemplate = false,
  initialCreateOpen = false,
}: Props) {
  const showAddTemplate = canAddTemplate;
  return (
    <div className="space-y-sm">
      <div className="flex flex-wrap items-baseline justify-between gap-md">
        <SectionLabel>Fleet library</SectionLabel>
        {showAddTemplate && templates.length > 0 ? (
          <AddTemplateDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
        ) : null}
      </div>
      {templates.length > 0 ? (
        <div className="grid grid-cols-1 gap-md sm:grid-cols-2 lg:grid-cols-3">
          {templates.map((template) => (
            <TemplateCard
              key={template.id}
              template={template}
              action={
                <Button type="button" onClick={() => onUseTemplate(template)}>
                  Use entry
                </Button>
              }
            />
          ))}
        </div>
      ) : (
        <EmptyState
          icon={<LayoutTemplateIcon size={28} />}
          title={FLEET_LIBRARY_EMPTY_TITLE}
          description={showAddTemplate ? FLEET_LIBRARY_EMPTY_DESCRIPTION : FLEET_LIBRARY_EMPTY_DESCRIPTION_READONLY}
          action={
            <div className="flex flex-wrap items-center justify-center gap-md">
              <TemplateDocsLink />
              {showAddTemplate ? (
                <AddTemplateDialog workspaceId={workspaceId} defaultOpen={initialCreateOpen} />
              ) : null}
            </div>
          }
        />
      )}
    </div>
  );
}
