"use client";

import {
  Button,
  DashboardPanel,
  DashboardPanelContent,
  DashboardPanelDescription,
  DashboardPanelHeader,
  DashboardPanelTitle,
  EmptyState,
  SectionLabel,
} from "@agentsfleet/design-system";
import type { FleetTemplateGalleryEntry } from "@/lib/types";
import AddTemplateDialog, { CREATE_TEMPLATE_DOC_URL } from "./AddTemplateDialog";
import { InstallFlowGuide } from "./InstallFlowGuide";
import { TemplateCard } from "./TemplateCard";

type Props = {
  workspaceId: string;
  templates: FleetTemplateGalleryEntry[];
  onUseTemplate: (template: FleetTemplateGalleryEntry) => void;
  canAddTemplate?: boolean;
};

// Template gallery picker: the workspace's templates (platform ∪ tenant) are the
// install surface. Picking one proceeds inline to the live install states —
// there is no review page. github-import and paste authoring were removed in
// M103; onboard a template first, then install it from here.
export function InstallSourceSelector({
  workspaceId,
  templates,
  onUseTemplate,
  canAddTemplate = false,
}: Props) {
  const showAddTemplate = canAddTemplate;
  return (
    <div className="grid grid-cols-1 gap-lg lg:grid-cols-3">
      <DashboardPanel padding="compact" className="lg:col-span-2">
        <DashboardPanelHeader>
          <div className="space-y-2">
            <SectionLabel>Next step</SectionLabel>
            <DashboardPanelTitle>Start your fleet</DashboardPanelTitle>
            <DashboardPanelDescription className="max-w-prose">
              Pick a template. Install runs inline.
            </DashboardPanelDescription>
          </div>
        </DashboardPanelHeader>

        <DashboardPanelContent>
          <div className="space-y-sm">
            <div className="flex flex-wrap items-center justify-between gap-md">
              <SectionLabel>Start from a template</SectionLabel>
              {showAddTemplate && templates.length > 0 ? (
                <AddTemplateDialog workspaceId={workspaceId} />
              ) : null}
            </div>
            {templates.length > 0 ? (
              <div className="grid grid-cols-1 gap-md sm:grid-cols-2">
                {templates.map((template) => (
                  <TemplateCard
                    key={template.id}
                    template={template}
                    action={
                      <Button type="button" onClick={() => onUseTemplate(template)}>
                        Use template
                      </Button>
                    }
                  />
                ))}
              </div>
            ) : (
              <div className="space-y-md">
                <EmptyState
                  title="No templates found"
                  description="Add a template, then install it here."
                />
                <div className="flex flex-wrap justify-center gap-md">
                  {showAddTemplate ? <AddTemplateDialog workspaceId={workspaceId} /> : null}
                  <Button asChild variant="ghost" size="sm">
                    <a href={CREATE_TEMPLATE_DOC_URL} target="_blank" rel="noopener noreferrer">
                      Create a template
                    </a>
                  </Button>
                </div>
              </div>
            )}
          </div>
        </DashboardPanelContent>
      </DashboardPanel>

      <InstallFlowGuide />
    </div>
  );
}
