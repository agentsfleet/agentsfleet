export function registerProgramCommands(handlers) {
  return {
    login: handlers.login,
    logout: handlers.logout,
    workspace: handlers.workspace,
    "specs.sync": handlers.specsSync,
    run: handlers.run,
    "runs.list": handlers.runsList,
    doctor: handlers.doctor,
    harness: handlers.harness,
    "skill-secret": handlers.skillSecret,
  };
}
