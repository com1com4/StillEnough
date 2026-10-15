import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const write = (path, value) => writeFileSync(new URL(path, root), value);
const manifest = JSON.parse(read("package.json"));
const argument = process.argv[2];
const current = manifest.version;
const parts = current.split(".").map(Number);
const bumps = {
  patch: `${parts[0]}.${parts[1]}.${parts[2] + 1}`,
  minor: `${parts[0]}.${parts[1] + 1}.0`,
  major: `${parts[0] + 1}.0.0`,
};
const version = argument === "--check" ? current : (bumps[argument] ?? argument);
if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version ?? "")) {
  throw new Error("用法：node scripts/version.mjs <patch|minor|major|X.Y.Z|--check>");
}
const cargoPath = "src-tauri/Cargo.toml";
const lockPath = "src-tauri/Cargo.lock";
const cargo = read(cargoPath);
const lock = read(lockPath);
const cargoPattern = /(\[package\][\s\S]*?\nversion = ")([^"]+)(")/;
const lockPattern = /(\[\[package\]\]\r?\nname = "stillenough"\r?\nversion = ")([^"]+)(")/;
if (!cargoPattern.test(cargo) || !lockPattern.test(lock)) {
  throw new Error("未找到 Rust 应用版本字段，未写入任何文件");
}
if (JSON.parse(read("src-tauri/tauri.conf.json")).version !== "../package.json") {
  throw new Error("Tauri version 必须引用 ../package.json");
}
if (argument === "--check") {
  if (cargo.match(cargoPattern)[2] !== version || lock.match(lockPattern)[2] !== version) {
    throw new Error("版本不一致，请运行 pnpm version:set " + version);
  }
  console.log(`版本一致：${version}`);
} else {
  manifest.version = version;
  write("package.json", JSON.stringify(manifest, null, 2) + "\n");
  write(
    cargoPath,
    cargo.replace(cargoPattern, (_, before, _old, after) => before + version + after),
  );
  write(
    lockPath,
    lock.replace(lockPattern, (_, before, _old, after) => before + version + after),
  );
  console.log(`${current} → ${version}（${fileURLToPath(root)}）`);
}
