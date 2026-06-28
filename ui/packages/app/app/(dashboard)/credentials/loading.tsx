import RouteLoading from "@/components/layout/RouteLoading";

// /credentials is a pure redirect to /settings/models (see page.tsx). Paint the
// DESTINATION header — "Models & Keys" — not "Credentials", so the redirect hop
// doesn't flash a title the user never lands on (matches settings/models/loading).
export default function CredentialsLoading() {
  return (
    <RouteLoading
      title="Models & Keys"
      description="The model your fleets run on, and the keys behind it."
    />
  );
}
