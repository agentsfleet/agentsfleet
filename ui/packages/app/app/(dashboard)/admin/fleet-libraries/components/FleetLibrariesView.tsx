"use client";

import { useState } from "react";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  EmptyState,
  MetaGrid,
  PageHeader,
  PageTitle,
  Section,
  SectionLabel,
} from "@agentsfleet/design-system";
import { LibraryIcon } from "lucide-react";
import type { OnboardedPlatformLibraryEntry } from "@/lib/types";
import { FLEET_LIBRARIES_DESCRIPTION, FLEET_LIBRARIES_TITLE } from "../library-copy";
import OnboardPlatformLibraryDialog from "./OnboardPlatformLibraryDialog";

const EMPTY_TITLE = "Nothing onboarded yet in this session";
const EMPTY_DESCRIPTION =
  "Onboard a repository and its entry appears here. Installed fleets live in each workspace's gallery.";

// The platform catalog has no list route, so this surface deliberately shows no
// table: it renders the onboard affordance, and the entry the server actually
// returned. Anything else would be a claim the dashboard cannot substantiate —
// the workspace gallery is where an onboarded fleet is confirmed.
export default function FleetLibrariesView() {
  const [onboarded, setOnboarded] = useState<OnboardedPlatformLibraryEntry | null>(null);

  return (
    <div className="space-y-8">
      <PageHeader description={FLEET_LIBRARIES_DESCRIPTION}>
        <PageTitle>{FLEET_LIBRARIES_TITLE}</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Platform fleet library">
          <div className="flex flex-wrap items-baseline justify-between gap-md">
            <SectionLabel>Onboard a fleet</SectionLabel>
            <OnboardPlatformLibraryDialog onOnboarded={setOnboarded} />
          </div>

          {onboarded ? (
            <Card data-testid={`onboarded-entry-${onboarded.id}`}>
              <CardHeader>
                <CardTitle>{onboarded.name}</CardTitle>
              </CardHeader>
              <CardContent>
                <MetaGrid
                  items={[
                    { label: "Catalog id", value: onboarded.id },
                    { label: "Tier", value: onboarded.visibility },
                    { label: "Content hash", value: onboarded.content_hash },
                    {
                      label: "Credentials",
                      value: onboarded.requirements.credentials.join(", ") || "none",
                    },
                    { label: "Tools", value: onboarded.requirements.tools.join(", ") || "none" },
                    { label: "Support files", value: onboarded.support_files.length },
                  ]}
                />
              </CardContent>
            </Card>
          ) : (
            <EmptyState
              icon={<LibraryIcon size={20} aria-hidden="true" />}
              title={EMPTY_TITLE}
              description={EMPTY_DESCRIPTION}
            />
          )}
        </section>
      </Section>
    </div>
  );
}
