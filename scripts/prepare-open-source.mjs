import { createHash } from "node:crypto";
import {
  copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync,
  readdirSync, realpathSync, writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = realpathSync(fileURLToPath(new URL("../", import.meta.url)));
const args = process.argv.slice(2);
if (args.length && (args.length !== 2 || args[0] !== "--destination")) {
  throw new Error("用法：node scripts/prepare-open-source.mjs [--destination output/open-source/新目录]");
}
const destination = resolve(root, args[1] ?? "output/open-source/StillEnough");
const boundary = join(root, "output", "open-source");
const inside = (parent, child) => {
  const path = relative(parent, child);
  return path !== "" && path !== ".." && !path.startsWith(`..${sep}`) && !isAbsolute(path);
};
if (!inside(boundary, destination)) throw new Error("目标必须位于本仓库 output/open-source/ 内");
// Reject linked ancestors before writing; never overwrite or recursively delete a target.
for (let path = destination; inside(root, path); path = dirname(path)) {
  if (existsSync(path) && lstatSync(path).isSymbolicLink()) throw new Error("目标路径含链接，停止导出");
}
if (existsSync(destination)) throw new Error("目标已存在，请选择新目录；不会覆盖现有文件");
const manifestPath = `${destination}.manifest.json`;
if (existsSync(manifestPath)) throw new Error("清单已存在，请选择新目录");

const files = [
  ".gitignore", ".prettierrc.json", "AGENTS.md", "README.md", "TASKS.md",
  "LICENSE", "CONTRIBUTING.md", "SECURITY.md", "THIRD_PARTY_NOTICES.md",
  "package.json", "pnpm-lock.yaml", "index.html", "eslint.config.js",
  "tsconfig.json", "tsconfig.node.json", "vite.config.ts", "vitest.config.ts",
  "src-tauri/tests/test-manifest.rc", "src-tauri/tests/test-manifest.xml", "packaging/npm-cli/stillenough-cli.mjs", "src-tauri/build.rs", "src-tauri/Cargo.toml", "Cargo.lock", "Cargo.toml", "src-tauri/tauri.conf.json",
  "assets/brand/stillenough-logo-v15-azure-lime.png",
  "docs/CLI.md", "docs/CLI-IMPLEMENTATION.md", "docs/CLI-DESIGN.md", "docs/CLI-P1.md", "docs/cli-response-v1.schema.json",
  "docs/PRODUCT.md", "docs/ARCHITECTURE.md", "docs/PROVIDERS.md",
  "docs/VERSIONING.md", "docs/OPEN_SOURCE.md", "docs/RELEASE_CHECKLIST.md", "docs/brand/README.md",
];
const trees = [
  "crates", "src", "src-tauri/src", "src-tauri/migrations", "src-tauri/capabilities", "src-tauri/icons",
  "scripts", "fixtures", "public/brand", "public/providers", "docs/providers", ".github",
];
const blockedName = /^(?:\.git|node_modules|target|dist|output|\.local-archive|exports|\.env(?:\..*)?|auth\.json|\.?credentials\.json)$/i;
const blockedExtension = /\.(?:db|sqlite)(?:-(?:wal|shm))?$|\.(?:log|pem|key|exe|msi|patch|tsbuildinfo)$/i;
function inspect(path) {
  const info = lstatSync(join(root, path));
  if (info.isSymbolicLink()) throw new Error(`源文件包含链接：${path}`);
  if (blockedName.test(path.split(/[\\/]/).at(-1)) || blockedExtension.test(path)) {
    throw new Error(`发布清单包含禁止文件：${path}`);
  }
  return info;
}
function collect(path) {
  if (path === "src/assets/react.svg") return; // Unused scaffold artwork.
  const info = inspect(path);
  if (info.isDirectory()) {
    for (const name of readdirSync(join(root, path)).sort()) collect(`${path}/${name}`);
  } else if (info.isFile()) files.push(path);
  else throw new Error(`不支持的文件类型：${path}`);
}
for (const tree of trees) collect(tree);

const patterns = [
  ["疑似 API Key", /\bsk-[A-Za-z0-9_-]{20,}/],
  ["疑似 GitHub Token", /\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{30,})/],
  ["私钥", /-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----/],
  ["个人 Windows 路径", /[A-Za-z]:[\\/]Users[\\/](?!Public(?:[\\/]|\b))[^\s<>"']+/i],
  ["本机开发路径", /[A-Za-z]:[\\/](?:my-projects|pnpm-data)[\\/]/i],
];
const entries = [];
for (const path of [...new Set(files)].sort()) {
  if (!inspect(path).isFile()) throw new Error(`清单需要文件：${path}`);
  const bytes = readFileSync(join(root, path));
  if (/\.(?:md|json|rs|ts|tsx|css|html|mjs|toml|ya?ml|lock|sql)$/.test(path)) {
    for (const [label, pattern] of patterns) {
      if (pattern.test(bytes.toString("utf8"))) throw new Error(`${label}：${path}（不输出匹配内容）`);
    }
  }
  entries.push({ path, bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") });
}
const selected = new Set(entries.map((entry) => entry.path));
for (const { path } of entries.filter((entry) => entry.path.endsWith(".md"))) {
  const markdown = readFileSync(join(root, path), "utf8");
  for (const match of markdown.matchAll(/\]\(([^\s)]+)(?:\s+"[^"]*")?\)/g)) {
    const link = match[1];
    if (/^[a-z][a-z\d+.-]*:|^#/i.test(link)) continue;
    const target = resolve(dirname(join(root, path)), decodeURIComponent(link.split(/[?#]/)[0]));
    const targetPath = relative(root, target).split(sep).join("/");
    if (!selected.has(targetPath)) throw new Error(`本地文档链接未纳入发布：${path} -> ${targetPath}`);
  }
}
// Preflight completes before any source is copied.
mkdirSync(destination, { recursive: true });
for (const entry of entries) {
  const target = join(destination, entry.path);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(join(root, entry.path), target);
  const actual = createHash("sha256").update(readFileSync(target)).digest("hex");
  if (actual !== entry.sha256) throw new Error(`导出期间文件改变：${entry.path}；请使用新目录重试`);
}
writeFileSync(manifestPath, JSON.stringify({
  generatedAt: new Date().toISOString(),
  source: "current-working-directory", gitHistoryIncluded: false,
  note: "有限模式扫描，不替代完整秘密或第三方依赖许可审计。", files: entries,
}, null, 2) + "\n");
console.log(`已导出 ${entries.length} 个文件：${destination}`);
console.log(`SHA-256 清单：${manifestPath}`);
console.log("未创建 Git 提交、标签或远程推送；公开前核对 docs/RELEASE_CHECKLIST.md。");
