import readline from "node:readline";

export async function promptYesNo(stdin, stdout, message, { defaultYes = true } = {}) {
  if (!stdin || stdin.isTTY === false) return null;
  return new Promise((resolve) => {
    const rl = readline.createInterface({ input: stdin, output: stdout, terminal: false });
    let answered = false;
    rl.question(message, (answer) => {
      answered = true;
      rl.close();
      const trimmed = String(answer).trim().toLowerCase();
      if (trimmed === "") return resolve(defaultYes);
      if (trimmed === "y" || trimmed === "yes") return resolve(true);
      if (trimmed === "n" || trimmed === "no") return resolve(false);
      resolve(defaultYes);
    });
    rl.on("close", () => {
      if (!answered) resolve(null);
    });
  });
}
