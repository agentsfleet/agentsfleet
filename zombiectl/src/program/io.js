function writeLine(stream, line = "") {
  stream.write(`${line}\n`);
}

function printJson(stream, value) {
  writeLine(stream, JSON.stringify(value, null, 2));
}

function printHelp(stdout, ui) {
  writeLine(stdout, ui.head("zombiectl - UseZombie operator CLI"));
  writeLine(stdout);
  writeLine(stdout, "USAGE:");
  writeLine(stdout, "  zombiectl [--api URL] [--json] <command> [subcommand] [flags]");
  writeLine(stdout);
  writeLine(stdout, "COMMANDS:");
  writeLine(stdout, "  login [--timeout-sec N] [--no-open]");
  writeLine(stdout, "  logout");
  writeLine(stdout, "  workspace add <repo_url> [--default-branch BRANCH]");
  writeLine(stdout, "  workspace list");
  writeLine(stdout, "  workspace remove <workspace_id>");
  writeLine(stdout, "  specs sync [--workspace-id ID]");
  writeLine(stdout, "  run [--workspace-id ID] [--spec-id ID] [--mode MODE] [--requested-by USER]");
  writeLine(stdout, "  run status <run_id>");
  writeLine(stdout, "  runs list [--workspace-id ID]");
  writeLine(stdout, "  doctor");
  writeLine(stdout, "  harness source put --workspace-id ID --file PATH [--profile-id ID] [--name NAME]");
  writeLine(stdout, "  harness compile --workspace-id ID [--profile-id ID] [--profile-version-id ID]");
  writeLine(stdout, "  harness activate --workspace-id ID --profile-version-id ID [--activated-by USER]");
  writeLine(stdout, "  harness active --workspace-id ID");
  writeLine(stdout, "  skill-secret put --workspace-id ID --skill-ref REF --key KEY --value VALUE [--scope host|sandbox]");
  writeLine(stdout, "  skill-secret delete --workspace-id ID --skill-ref REF --key KEY");
  writeLine(stdout);
  writeLine(stdout, ui.dim("workspace add opens UseZombie GitHub App install and binds via callback."));
}

export {
  printHelp,
  printJson,
  writeLine,
};
