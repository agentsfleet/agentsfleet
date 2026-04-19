import {
  Button,
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
  CardFooter,
  Terminal,
  Grid,
  Section,
  InstallBlock,
  AnimatedIcon,
  ZombieHandIcon,
  Badge,
  Input,
  Separator,
  Skeleton,
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
  EmptyState,
  StatusCard,
  Pagination,
} from "@usezombie/design-system";

/*
 * DesignSystemGallery — hidden smoke route at /_design-system.
 *
 * Purpose: contract test surface for the shared design-system package.
 * Playwright's tests/e2e/design-system-smoke.spec.ts visits this page
 * and asserts computed styles on every component + variant — catches
 * Tailwind compilation gaps that JSDOM className-string tests can't see.
 *
 * Not linked from any navigation. Safe to leave in production.
 */

export default function DesignSystemGallery() {
  return (
    <Section gap>
      <h1>Design System Gallery</h1>
      <p>Smoke route for Playwright computed-style verification.</p>

      <Section>
        <h2>Button — variants</h2>
        <div className="flex flex-wrap gap-md">
          <Button data-testid="btn-default">Default</Button>
          <Button variant="destructive" data-testid="btn-destructive">Destructive</Button>
          <Button variant="outline" data-testid="btn-outline">Outline</Button>
          <Button variant="secondary" data-testid="btn-secondary">Secondary</Button>
          <Button variant="ghost" data-testid="btn-ghost">Ghost</Button>
          <Button variant="link" data-testid="btn-link">Link</Button>
          <Button variant="double-border" data-testid="btn-double-border">Double border</Button>
        </div>
      </Section>

      <Section>
        <h2>Button — sizes</h2>
        <div className="flex flex-wrap items-center gap-md">
          <Button size="sm" data-testid="btn-sm">Small</Button>
          <Button size="default" data-testid="btn-default-size">Default</Button>
          <Button size="lg" data-testid="btn-lg">Large</Button>
          <Button size="icon" aria-label="settings" data-testid="btn-icon">⚙</Button>
        </div>
      </Section>

      <Section>
        <h2>Button — asChild</h2>
        <Button asChild data-testid="btn-aschild">
          <a href="#top">Anchor child</a>
        </Button>
      </Section>

      <Section>
        <h2>Card</h2>
        <Grid columns="two">
          <Card data-testid="card-default">
            <CardHeader>
              <CardTitle>Standard card</CardTitle>
              <CardDescription>Default variant, no featured badge</CardDescription>
            </CardHeader>
            <CardContent>Body content goes here.</CardContent>
            <CardFooter>Footer</CardFooter>
          </Card>
          <Card featured data-testid="card-featured">
            <CardHeader>
              <CardTitle>Featured card</CardTitle>
              <CardDescription>Renders a &ldquo;Popular&rdquo; badge</CardDescription>
            </CardHeader>
            <CardContent>Featured content.</CardContent>
          </Card>
        </Grid>
      </Section>

      <Section>
        <h2>Terminal</h2>
        <Terminal label="default terminal" data-testid="terminal-default">
          {"echo default"}
        </Terminal>
        <Terminal green copyable label="green terminal" data-testid="terminal-green">
          {"echo green"}
        </Terminal>
      </Section>

      <Section>
        <h2>Grid</h2>
        <Grid columns="two" data-testid="grid-two">
          <div>A</div>
          <div>B</div>
        </Grid>
        <Grid columns="three" data-testid="grid-three">
          <div>A</div>
          <div>B</div>
          <div>C</div>
        </Grid>
        <Grid columns="four" data-testid="grid-four">
          <div>A</div>
          <div>B</div>
          <div>C</div>
          <div>D</div>
        </Grid>
      </Section>

      <Section>
        <h2>InstallBlock</h2>
        <InstallBlock
          title="Install zombiectl"
          command="curl -sSL https://usezombie.sh/install | bash"
          actions={[
            { label: "Docs", to: "https://docs.usezombie.com", external: true },
            { label: "Pricing", to: "/pricing", variant: "ghost" },
          ]}
        />
      </Section>

      <Section>
        <h2>Badge — variants</h2>
        <div className="flex flex-wrap items-center gap-md">
          <Badge data-testid="badge-default">Default</Badge>
          <Badge variant="orange" data-testid="badge-orange">Active</Badge>
          <Badge variant="amber" data-testid="badge-amber">Pending</Badge>
          <Badge variant="green" data-testid="badge-green">Healthy</Badge>
          <Badge variant="cyan" data-testid="badge-cyan">Info</Badge>
          <Badge variant="destructive" data-testid="badge-destructive">Error</Badge>
        </div>
      </Section>

      <Section>
        <h2>Input</h2>
        <div className="flex flex-col gap-md max-w-sm">
          <Input placeholder="you@example.com" data-testid="input-default" />
          <Input placeholder="Disabled" disabled data-testid="input-disabled" />
        </div>
      </Section>

      <Section>
        <h2>Separator</h2>
        <div className="flex flex-col gap-md">
          <div>Above</div>
          <Separator data-testid="separator-horizontal" />
          <div>Below</div>
          <div className="flex h-8 items-center gap-md">
            <span>Left</span>
            <Separator orientation="vertical" data-testid="separator-vertical" />
            <span>Right</span>
          </div>
        </div>
      </Section>

      <Section>
        <h2>Skeleton</h2>
        <div className="flex flex-col gap-md max-w-sm">
          <Skeleton className="h-4 w-3/4" data-testid="skeleton-line" />
          <Skeleton className="h-4 w-1/2" data-testid="skeleton-line-sm" />
          <Skeleton className="h-24 w-full" data-testid="skeleton-block" />
        </div>
      </Section>

      <Section>
        <h2>Dialog</h2>
        <Dialog>
          <DialogTrigger data-testid="dialog-trigger" className="border border-border rounded-full px-4 py-2 text-sm">
            Open dialog
          </DialogTrigger>
          <DialogContent data-testid="dialog-content">
            <DialogHeader>
              <DialogTitle>Confirm action</DialogTitle>
              <DialogDescription data-testid="dialog-description">
                This is a smoke-route dialog for Playwright verification.
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="ghost">Cancel</Button>
              <Button>Confirm</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </Section>

      <Section>
        <h2>DropdownMenu</h2>
        <DropdownMenu>
          <DropdownMenuTrigger data-testid="dropdown-trigger" className="border border-border rounded-full px-4 py-2 text-sm">
            Open menu
          </DropdownMenuTrigger>
          <DropdownMenuContent data-testid="dropdown-content">
            <DropdownMenuLabel data-testid="dropdown-label">Actions</DropdownMenuLabel>
            <DropdownMenuItem data-testid="dropdown-item">
              Edit<DropdownMenuShortcut>⌘E</DropdownMenuShortcut>
            </DropdownMenuItem>
            <DropdownMenuSeparator data-testid="dropdown-separator" />
            <DropdownMenuItem>Delete</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </Section>

      <Section>
        <h2>Tooltip</h2>
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger data-testid="tooltip-trigger" className="border border-border rounded-full px-4 py-2 text-sm">
              Hover me
            </TooltipTrigger>
            <TooltipContent data-testid="tooltip-content">Ship it</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </Section>

      <Section>
        <h2>EmptyState</h2>
        <EmptyState
          title="Nothing to show yet"
          description="Try adjusting the filter or creating a new record."
          action={<Button size="sm">Create</Button>}
        />
      </Section>

      <Section>
        <h2>StatusCard — variants</h2>
        <Grid columns="four">
          <StatusCard label="Active" count={12} variant="success" trend="up" sublabel="last 24h" />
          <StatusCard label="Pending" count={3} variant="warning" trend="flat" />
          <StatusCard label="Stopped" count={1} variant="danger" trend="down" />
          <StatusCard label="Idle" count={7} variant="muted" />
        </Grid>
      </Section>

      <Section>
        <h2>Pagination — cursor + page</h2>
        <Pagination kind="cursor" nextCursor="abc" onNext={() => {}} />
        <Pagination kind="page" page={2} pageSize={20} total={87} onPageChange={() => {}} />
      </Section>

      <Section>
        <h2>AnimatedIcon</h2>
        <div className="flex flex-wrap items-center gap-lg">
          <AnimatedIcon animation="wave" trigger="always" label="always-wave">
            <ZombieHandIcon size={24} />
          </AnimatedIcon>
          <AnimatedIcon animation="wiggle" trigger="self-hover" label="self-hover-wiggle">
            <ZombieHandIcon size={24} />
          </AnimatedIcon>
          <span className="group">
            parent-hover <AnimatedIcon animation="wave" trigger="parent-hover" label="parent-hover-wave">
              <ZombieHandIcon size={24} />
            </AnimatedIcon>
          </span>
        </div>
      </Section>
    </Section>
  );
}
