import RouteLoading from "@/components/layout/RouteLoading";

// Paints the correct Credentials header instantly on navigation while the
// provider + stored-secret reads resolve.
export default function CredentialsLoading() {
  return (
    <RouteLoading
      title="Credentials"
      description="Write-only keys for models, tools, and secrets."
    />
  );
}
