import RouteLoading from "@/components/layout/RouteLoading";
import { LIBRARY_PAGE_DESCRIPTION, LIBRARY_PAGE_TITLE } from "./copy";

// The real header while the entries load, from the same constants the page
// uses — so the title does not change once the data arrives.
export default function LibraryLoading() {
  return <RouteLoading title={LIBRARY_PAGE_TITLE} description={LIBRARY_PAGE_DESCRIPTION} />;
}
