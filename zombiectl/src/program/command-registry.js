export function registerProgramCommands(handlers) {
  return {
    login: handlers.login,
    logout: handlers.logout,
    workspace: handlers.workspace,
    "specs.sync": handlers.specsSync,
    doctor: handlers.doctor,
    admin: handlers.admin,
    agent: handlers.agent,
    grant: handlers.grant,
    // M1_001 §5: Zombie commands
    "zombie.install": handlers.zombieInstall,
    "zombie.up": handlers.zombieUp,
    "zombie.status": handlers.zombieStatus,
    "zombie.kill": handlers.zombieKill,
    "zombie.logs": handlers.zombieLogs,
    "zombie.credential": handlers.zombieCredential,
  };
}
