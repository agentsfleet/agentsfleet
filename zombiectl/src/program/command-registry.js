export function registerProgramCommands(handlers) {
  return {
    login: handlers.login,
    logout: handlers.logout,
    workspace: handlers.workspace,
    "specs.sync": handlers.specsSync,
    "spec.init": handlers.specInit,
    run: handlers.run,
    "runs.list": handlers.runsList,
    "runs.cancel": handlers.runsCancel,
    "runs.replay": handlers.runsReplay,
    "runs.interrupt": handlers.runsInterrupt,
    doctor: handlers.doctor,
    harness: handlers.harness,
    "skill-secret": handlers.skillSecret,
    agent: handlers.agent,
    admin: handlers.admin,
  };
}
