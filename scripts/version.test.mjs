import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

test("版本升级同步应用配置和锁文件，检查拒绝漂移与非法版本", () => {
  const dir = mkdtempSync(join(tmpdir(), "stillenough-version-"));
  try {
    mkdirSync(join(dir, "scripts"));
    mkdirSync(join(dir, "src-tauri"));
    for (const path of [
      "scripts/version.mjs",
      "package.json",
      "src-tauri/Cargo.toml",
      "src-tauri/Cargo.lock",
      "src-tauri/tauri.conf.json",
    ]) {
      copyFileSync(new URL("../" + path, import.meta.url), join(dir, path));
    }
    const run = (arg) =>
      spawnSync(process.execPath, [join(dir, "scripts/version.mjs"), arg], { encoding: "utf8" });
    const read = (path) => readFileSync(join(dir, path), "utf8");
    assert.equal(run("--check").status, 0);
    assert.equal(run("2.3.4").status, 0);
    for (const [bump, expected] of [
      ["patch", "2.3.5"],
      ["minor", "2.4.0"],
      ["major", "3.0.0"],
    ]) {
      assert.equal(run(bump).status, 0);
      assert.equal(JSON.parse(read("package.json")).version, expected);
      assert.equal(run("--check").status, 0);
    }
    const before = read("package.json");
    assert.notEqual(run("01.0.0").status, 0);
    assert.equal(read("package.json"), before);
    writeFileSync(
      join(dir, "src-tauri/Cargo.toml"),
      read("src-tauri/Cargo.toml").replace('version = "3.0.0"', 'version = "2.0.0"'),
    );
    assert.notEqual(run("--check").status, 0);
    assert.equal(run("3.0.0").status, 0);
    assert.equal(run("--check").status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
