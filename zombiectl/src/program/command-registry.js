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
    // M1_001 §5: Zombie commands
    "zombie.install": handlers.zombieInstall,
    "zombie.up": handlers.zombieUp,
    "zombie.status": handlers.zombieStatus,
    "zombie.kill": handlers.zombieKill,
    "zombie.logs": handlers.zombieLogs,
    "zombie.credential": handlers.zombieCredential,
  };
}
