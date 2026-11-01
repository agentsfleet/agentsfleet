import { runEvidenceCommand } from "./ci-evidence/command.ts";

export const result = runEvidenceCommand(process.argv.slice(2)).then(status => {process.exitCode = status;});
