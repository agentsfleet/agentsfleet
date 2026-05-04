export function registerProgramCommands(handlers) {
  return {
    login: handlers.login,
    logout: handlers.logout,
    workspace: handlers.workspace,
    doctor: handlers.doctor,
    agent: handlers.agent,
    grant: handlers.grant,
    tenant: handlers.tenant,
    billing: handlers.billing,
    // Zombie commands
    "zombie.install": handlers.zombieInstall,
    "zombie.list": handlers.zombieList,
    "zombie.status": handlers.zombieStatus,
    "zombie.kill": handlers.zombieKill,
    "zombie.stop": handlers.zombieStop,
    "zombie.resume": handlers.zombieResume,
    "zombie.delete": handlers.zombieDelete,
    "zombie.logs": handlers.zombieLogs,
    "zombie.steer": handlers.zombieSteer,
    "zombie.events": handlers.zombieEvents,
    "zombie.credential": handlers.zombieCredential,
  };
}
