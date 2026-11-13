import { requireCredential } from "@/lib/auth/credential";
import { LibraryContent } from "./components/LibraryContent";

export const dynamic = "force-dynamic";

export default async function LibraryPage({params}: {params: Promise<{workspaceId: string}>}) {
  const {workspaceId} = await params;
  await requireCredential();
  return <LibraryContent workspaceId={workspaceId} />;
}
