import path from "node:path";

const built = await Bun.build({
  entrypoints: ["tests/e2e/fixtures/runner-configuration-probe.tsx"],
  target: "browser", format: "esm", minify: true,
  define: {"process.env.NODE_ENV": '"production"', "process.env.NEXT_PUBLIC_API_URL": '"https://api-test.agentsfleet.net"'},
  plugins: [{name: "runner-action-http-boundary", setup(build) {
    build.onResolve({filter: /^\.\.\/actions$|^\.\.\/\.\.\/models\/actions$/},
      () => ({path: path.join(import.meta.dirname, "runner-configuration-action.ts")}));
  }}],
});
if (!built.success) throw new AggregateError(built.logs, "Runner fixture build failed");
const output = built.outputs[0];
if (!output) throw new Error("Runner fixture build produced no JavaScript");
process.stdout.write(await output.text());
