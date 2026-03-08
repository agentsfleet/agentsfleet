import { redirect } from "next/navigation";

/** Root → dashboard redirect */
export default function RootPage() {
  redirect("/workspaces");
}
