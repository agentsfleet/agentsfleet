import { clerkMiddleware, createRouteMatcher } from "@clerk/nextjs/server";
import { buildSignInUrl } from "@/lib/auth/sign-in-redirect";

const isPublicRoute = createRouteMatcher([
  "/sign-in(.*)",
  "/sign-up(.*)",
]);

export default clerkMiddleware(async (auth, request) => {
  if (!isPublicRoute(request)) {
    // Route unauthenticated hits to our embedded `/sign-in` page (Clerk would
    // otherwise use the hosted Account Portal, since NEXT_PUBLIC_CLERK_SIGN_IN_URL
    // isn't set outside tests). Carry the intended destination on `redirect_url`
    // so a completed sign-in returns to the deep-linked page instead of the
    // fleet-wall fallback. A relative path keeps the target same-origin.
    const destination = request.nextUrl.pathname + request.nextUrl.search;
    await auth.protect({ unauthenticatedUrl: buildSignInUrl(request.url, destination) });
  }
});

export const config = {
  matcher: [
    // Skip Next.js internals and static files
    "/((?!_next|[^?]*\\.(?:html?|css|js(?!on)|jpe?g|webp|png|gif|svg|ttf|woff2?|ico|csv|docx?|xlsx?|zip|webmanifest)).*)",
    "/(api|trpc)(.*)",
  ],
};
