import { mkdirSync, readFileSync, writeFileSync, copyFileSync, realpathSync } from "node:fs";
import { join, resolve, relative, isAbsolute } from "node:path";
import { spawnSync } from "node:child_process";
const root = realpathSync(new URL("../", import.meta.url));
const destination = resolve(process.argv[2] ?? "output/cli-release");
const rel = relative(join(root, "output"), destination);
if (!rel || rel.startsWith("..") || isAbsolute(rel)) throw new Error("打包路径必须位于本仓库 output 内");
const { version } = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const common = {
  version,
  license: "MIT",
  repository: { type: "git", url: "https://github.com/com1com4/StillEnough.git" },
};
for (const platform of [false, true]) {
  const dir = join(destination, platform ? "npm-win32-x64" : "npm-cli");
  mkdirSync(dir, { recursive: false });
  copyFileSync(join(root, "LICENSE"), join(dir, "LICENSE"));
  copyFileSync(join(root, "THIRD_PARTY_NOTICES.md"), join(dir, "THIRD_PARTY_NOTICES.md"));
  const pkg = platform
    ? {
        ...common,
        name: "@stillenough/cli-win32-x64",
        os: ["win32"],
        cpu: ["x64"],
        files: ["stillenough-cli.exe", "LICENSE", "THIRD_PARTY_NOTICES.md"],
      }
    : {
        ...common,
        name: "@stillenough/cli",
        type: "module",
        bin: { "stillenough-cli": "stillenough-cli.mjs" },
        files: ["stillenough-cli.mjs", "LICENSE", "THIRD_PARTY_NOTICES.md"],
        optionalDependencies: { "@stillenough/cli-win32-x64": version },
        engines: { node: ">=22" },
      };
  writeFileSync(join(dir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
  copyFileSync(
    platform
      ? join(root, "target/release/stillenough-cli.exe")
      : join(root, "packaging/npm-cli/stillenough-cli.mjs"),
    join(dir, platform ? "stillenough-cli.exe" : "stillenough-cli.mjs"),
  );
  // 使用当前 Node 的 npm JS 入口，不拼接 shell，也不运行 postinstall。
  const npmCli = join(process.execPath, "..", "node_modules", "npm", "bin", "npm-cli.js");
  const result = spawnSync(
    process.execPath,
    [npmCli, "pack", "--ignore-scripts", "--offline", "--cache", join(root,"output/npm-cache"), "--pack-destination", destination],
    { cwd: dir, stdio: "inherit", shell: false },
  );
  if (result.status !== 0) throw new Error("本地 npm pack 失败");
}
