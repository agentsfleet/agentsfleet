const buildId = Bun.env.AGENTSFLEET_BUILD_ID ?? null;
if (buildId !== null && !/^[0-9a-f]{64}$/.test(buildId)) throw new Error("Invalid build source identity");
await Bun.write(new URL("../dist/build-info.json", import.meta.url), `${JSON.stringify({ build_id: buildId })}\n`);
