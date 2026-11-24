import { Alert, Button, DashboardPanel } from "@agentsfleet/design-system";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { AuthFormReload, AuthFormStatus } from "./AuthFormStatus";

type Props = {kind: "sign-in" | "sign-up"};

export function AuthFormLoading({kind}: Props) {
  return <AuthFormStatus failure={
    <DashboardPanel className="w-full max-w-auth-card space-y-2xl rounded-lg p-2xl">
      <Alert variant="destructive">Couldn’t load {kind}. Reload the page to try again.</Alert>
      <Button asChild variant="outline" size="sm"><AuthFormReload /></Button>
    </DashboardPanel>
  }>
    <RouteLoading label={`Loading ${kind}…`} />
  </AuthFormStatus>;
}
