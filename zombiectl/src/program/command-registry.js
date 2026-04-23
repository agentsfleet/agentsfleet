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
    // Zombie commands
    "zombie.install": handlers.zombieInstall,
    "zombie.list": handlers.zombieList,
    "zombie.status": handlers.zombieStatus,
    "zombie.kill": handlers.zombieKill,
    "zombie.logs": handlers.zombieLogs,
    "zombie.credential": handlers.zombieCredential,
  };
}
