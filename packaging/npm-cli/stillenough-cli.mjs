#!/usr/bin/env node
import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
const require = createRequire(import.meta.url);
if (process.platform !== "win32" || process.arch !== "x64") {
  console.error("StillEnough CLI 当前仅分发 Windows x64；其他平台请从源码构建并核查支持范围。");
  process.exit(5);
}
let binary;
try {
  binary = join(dirname(require.resolve("@stillenough/cli-win32-x64/package.json")), "stillenough-cli.exe");
} catch {
  console.error("缺少匹配的 Windows x64 二进制包，请重新安装 CLI。");
  process.exit(3);
}
const child = spawn(binary, process.argv.slice(2), { stdio: "inherit", shell: false, windowsHide: true });
child.on("error", () => {
  console.error("无法启动 StillEnough CLI 二进制文件。");
  process.exitCode = 10;
});
child.on("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 130 : 10);
});
for (const signal of ["SIGINT", "SIGTERM"])
  process.on(signal, () => {
    child.kill(signal);
  });
