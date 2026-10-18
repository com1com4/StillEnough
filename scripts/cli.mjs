import { spawn } from "node:child_process";
const args = process.argv.slice(2);
if (args[0] === "--") args.shift();
const child = spawn("cargo", ["run", "-p", "stillenough-cli", "--locked", "--", ...args], {
  stdio: "inherit",
  shell: false,
  windowsHide: true,
});
child.on("error", () => {
  console.error("无法启动 Cargo；请安装 Rust MSVC 工具链");
  process.exitCode = 10;
});
child.on("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 130 : 10);
});
for (const signal of ["SIGINT", "SIGTERM"])
  process.on(signal, () => {
    child.kill(signal);
  });
