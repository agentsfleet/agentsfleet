import { clerkMiddleware, createRouteMatcher } from "@clerk/nextjs/server";
import { buildSignInUrl } from "@/lib/auth/sign-in-redirect";
import { CONNECTOR_COMPLETION_PATH } from "@/lib/connectors/completion";

const CONNECTOR_CALLBACK_ROUTE = /^\/api\/connectors\/[a-z][a-z0-9_-]{0,63}\/callback$/;

const isPublicRoute = createRouteMatcher([
  "/sign-in(.*)",
  "/sign-up(.*)",
  CONNECTOR_COMPLETION_PATH,
  // The relay requires a credential before any exchange; anonymous visits only recover.
  CONNECTOR_CALLBACK_ROUTE,
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
