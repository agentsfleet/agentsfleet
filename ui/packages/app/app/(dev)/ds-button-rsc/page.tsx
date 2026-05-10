import { Button } from "@usezombie/design-system";

/*
 * RSC fixture for M26.5 / spec dim 2.5 — proves the shared Button
 * renders cleanly from a React Server Component (no "use client", no
 * hooks, no event-handler props crossing the RSC boundary). If this
 * route ever grows a "use client" hoist during build, the shared
 * Button has regressed its RSC-safe contract.
 */
export default function DsButtonRscPage() {
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
