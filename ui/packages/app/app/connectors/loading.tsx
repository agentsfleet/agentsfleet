import { RouteLoading } from "@/components/layout/RouteLoading";

/** This route waits on server reads; its own boundary keeps the full-screen spinner. */
export default function Loading() {
  return <RouteLoading fullScreen />;
}
