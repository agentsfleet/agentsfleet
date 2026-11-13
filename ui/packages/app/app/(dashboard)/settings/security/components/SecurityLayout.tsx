import {AccountSettingsAction} from "@/components/account/AccountSettingsAction";
import type { ReactNode } from "react";
import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";

export function SecurityLayout({children}: {children: ReactNode}) {
  return <PageLayout><PageHeader description="Manage API keys, signed-in devices and your account’s sign-in settings." actions={<AccountSettingsAction />}><PageTitle>API Keys</PageTitle></PageHeader>{children}</PageLayout>;
}
