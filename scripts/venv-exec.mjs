/**
 * 项目级虚拟环境隔离执行器（Virtual Environment Wrapper）
 *
 * 核心作用：
 * 1. 将 Cargo 依赖缓存（CARGO_HOME）与 npm 缓存（NPM_CONFIG_CACHE）严格隔离至项目 `.venv/` 目录，
 *    避免污染系统用户全局目录；
 * 2. 将工作目录固定在项目根目录，按大小写不敏感清理并透传 Windows 环境变量；
 * 3. 动态探测当前用户 `~/.cargo/bin`，并安全保留用户环境中的代理设置。
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");
const [cmd, ...args] = process.argv.slice(2);

if (!cmd) {
  console.error("用法错误：请传入待执行的命令与参数");
  process.exit(2);
}

// 按大小写不敏感合并并归一化环境变量，避免 Windows CreateProcessW 出现大小写重复变量
const rawPath = process.env.PATH ?? process.env.Path ?? "";
const proxyKeys = new Map([
  ["http_proxy", "HTTP_PROXY"],
  ["https_proxy", "HTTPS_PROXY"],
  ["all_proxy", "ALL_PROXY"],
  ["no_proxy", "NO_PROXY"],
]);
const dropKeys = new Set(["path", "cargo_home", "npm_config_cache"]);

const childEnv = {};
for (const k of Object.keys(process.env)) {
  const lower = k.toLowerCase();
  if (dropKeys.has(lower)) continue;
  const canonicalProxy = proxyKeys.get(lower);
  if (canonicalProxy) {
    if (process.env[k] && !childEnv[canonicalProxy]) {
      childEnv[canonicalProxy] = process.env[k];
    }
    continue;
  }
  childEnv[k] = process.env[k];
}

const cargoBinDir = path.join(os.homedir(), ".cargo", "bin");
const hasCargoInPath = rawPath
  .split(path.delimiter)
  .some((seg) => seg.trim().toLowerCase() === cargoBinDir.toLowerCase());
childEnv.PATH =
  fs.existsSync(cargoBinDir) && !hasCargoInPath
    ? `${cargoBinDir}${path.delimiter}${rawPath}`
    : rawPath;
childEnv.CARGO_HOME = path.join(projectRoot, ".venv", "cargo");
childEnv.NPM_CONFIG_CACHE = path.join(projectRoot, ".venv", "npm-cache");

const res = spawnSync(cmd, args, {
  cwd: projectRoot,
  stdio: "inherit",
  env: childEnv,
  shell: process.platform === "win32",
});

if (res.error) {
  console.error(`执行失败：${res.error.message}`);
  process.exit(1);
}

process.exit(res.status ?? 1);
