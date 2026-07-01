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
import { InstallFlowGuide } from "./InstallFlowGuide";
import { TemplateCard } from "./TemplateCard";

type Props = {
  templates: FleetTemplateGalleryEntry[];
  onUseTemplate: (template: FleetTemplateGalleryEntry) => void;
};

// Template gallery picker: the workspace's templates (platform ∪ tenant) are the
// install surface. Picking one proceeds inline to the live install states —
// there is no review page. github-import and paste authoring were removed in
// M103; onboard a template first, then install it from here.
export function InstallSourceSelector({ templates, onUseTemplate }: Props) {
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
            <SectionLabel>Start from a template</SectionLabel>
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
              <EmptyState
                title="No templates available yet"
                description="Onboard a template into your workspace, then install it here."
              />
            )}
          </div>
        </DashboardPanelContent>
      </DashboardPanel>

      <InstallFlowGuide />
    </div>
  );
}
