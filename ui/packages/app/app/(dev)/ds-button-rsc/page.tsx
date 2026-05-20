import { notFound } from "next/navigation";
import { Button } from "@usezombie/design-system";

/*
 * RSC fixture — proves the shared Button renders cleanly from a React
 * Server Component (no "use client", no hooks, no event-handler props
 * crossing the RSC boundary). If this route ever grows a "use client"
 * hoist during build, the shared Button has regressed its RSC-safe
 * contract. That assertion lives at `next build` time: this module is
 * always compiled, so the contract is checked even though the route is
 * unreachable in production (the guard below 404s it). The fixture must
 * not be a public production surface — `notFound()` removes it from prod
 * while keeping the build-time guarantee. `notFound()` is a server-safe
 * call and does not introduce client-ness, so the RSC contract holds.
 */
export default function DsButtonRscPage() {
  if (process.env.NODE_ENV === "production") notFound();
  return (
    <main>
      <h1>DS Button — RSC fixture</h1>
      <Button variant="default">Hello</Button>
      {/* Dev fixture — a raw <a> is the load-bearing thing being tested
       * (Button asChild + RSC). Next's no-html-link-for-pages would force
       * <Link>, but that defeats the test (which checks the asChild
       * contract still holds with a non-Link child). */}
      <Button asChild variant="outline">
        {/* oxlint-disable-next-line nextjs/no-html-link-for-pages */}
        <a href="/">Home</a>
      </Button>
    </main>
  );
}
