/**
 * HookReplay 发布产物打包与完整性级别保障脚本（Release Packager）
 *
 * 职责：
 * 1. 读取 package.json 版本号，将 Release 单文件版与 NSIS 安装包复制到项目 `release/` 目录；
 * 2. 若通过 `--deploy-dir <path>` 或环境变量 `HOOKREPLAY_DEPLOY_DIR` 指定额外部署目录，
 *    通过同卷暂存原子移动（`rename`）同步至目标目录，使 NTFS 保留 `Medium` 安全描述符；
 * 3. 在 Windows 下通过 `icacls` 只读核查输出文件的完整性标签，若仍检测到 `Low Mandatory Level`
 *    则尝试校准并给出清晰提示，杜绝直接双击触发 0xC0000142 闪退；
 * 4. 若通过 `--sign-thumbprint <hash>` 或环境变量 `HOOKREPLAY_SIGN_THUMBPRINT` 指定证书指纹，
 *    则自动对输出的可执行文件与安装程序执行 Authenticode 代码签名。
 */
import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "..");

function readCliArg(flag) {
  const idx = process.argv.indexOf(flag);
  if (idx !== -1 && idx + 1 < process.argv.length) {
    return process.argv[idx + 1];
  }
  return "";
}

const pkg = JSON.parse(
  fs.readFileSync(path.join(projectRoot, "package.json"), "utf8")
);
const version = pkg.version || "1.1.0";

const portableSrc = path.join(
  projectRoot,
  "src-tauri",
  "target",
  "release",
  "hook-replay.exe"
);
const installerName = `HookReplay_${version}_x64-setup.exe`;
const installerSrc = path.join(
  projectRoot,
  "src-tauri",
  "target",
  "release",
  "bundle",
  "nsis",
  installerName
);

if (!fs.existsSync(portableSrc)) {
  console.error(`找不到 Release 单文件构建产物：${portableSrc}`);
  process.exit(1);
}

const releaseDir = path.join(projectRoot, "release");
fs.mkdirSync(releaseDir, { recursive: true });

function verifyOrRepairIntegrity(filePath) {
  if (process.platform !== "win32") return;
  try {
    const aclOutput = execFileSync("icacls", [filePath], {
      encoding: "utf8",
    });
    if (aclOutput.includes("Low Mandatory Level")) {
      try {
        execFileSync("icacls", [filePath, "/setintegritylevel", "Medium"], {
          stdio: "inherit",
        });
      } catch {
        console.warn(
          `[警告] 检测到 ${filePath} 仍带有 Low Mandatory Level，请将其移出低完整性目录或使用管理员终端执行：\n  icacls "${filePath}" /setintegritylevel Medium`
        );
      }
    }
  } catch {
    // 忽略只读 ACL 检查异常
  }
}

function copyToLocal(src, dest) {
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  if (fs.existsSync(dest)) {
    fs.unlinkSync(dest);
  }
  fs.copyFileSync(src, dest);
  verifyOrRepairIntegrity(dest);
  console.log(`已输出本地交付产物：${dest}`);
}

function deployWithCleanIntegrity(src, dest, stagingDir) {
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.mkdirSync(stagingDir, { recursive: true });
  const stageFile = path.join(stagingDir, `.stage-${path.basename(dest)}`);
  if (fs.existsSync(stageFile)) {
    fs.unlinkSync(stageFile);
  }
  fs.copyFileSync(src, stageFile);

  if (fs.existsSync(dest)) {
    fs.unlinkSync(dest);
  }
  try {
    // NTFS 同卷移动保留来源文件（release/ 暂存区）的无 Low 标签安全描述符
    fs.renameSync(stageFile, dest);
  } catch (err) {
    if (err && err.code === "EXDEV") {
      fs.copyFileSync(stageFile, dest);
      fs.unlinkSync(stageFile);
    } else {
      throw err;
    }
  }

  verifyOrRepairIntegrity(dest);
  console.log(`已部署并核查完整性级别：${dest}`);
}

const shippedFiles = [];

const localPortable = path.join(releaseDir, "HookReplay.exe");
copyToLocal(portableSrc, localPortable);
shippedFiles.push(localPortable);

const localInstaller = path.join(releaseDir, installerName);
if (fs.existsSync(installerSrc)) {
  copyToLocal(installerSrc, localInstaller);
  shippedFiles.push(localInstaller);
} else {
  console.warn(`跳过本地安装包输出（未找到文件）：${installerSrc}`);
}

const deployDir = (
  readCliArg("--deploy-dir") ||
  process.env.HOOKREPLAY_DEPLOY_DIR ||
  ""
).trim();
if (deployDir && !process.argv.includes("--local-only") && fs.existsSync(deployDir)) {
  const destPortable = path.join(deployDir, "HookReplay.exe");
  deployWithCleanIntegrity(portableSrc, destPortable, releaseDir);
  shippedFiles.push(destPortable);
  if (fs.existsSync(installerSrc)) {
    const destInstaller = path.join(deployDir, installerName);
    deployWithCleanIntegrity(installerSrc, destInstaller, releaseDir);
    shippedFiles.push(destInstaller);
  } else {
    console.warn(`跳过目标目录安装包输出（未找到文件）：${installerSrc}`);
  }
}

const signThumbprint = (
  readCliArg("--sign-thumbprint") ||
  process.env.HOOKREPLAY_SIGN_THUMBPRINT ||
  ""
).trim();
if (signThumbprint && process.platform === "win32" && shippedFiles.length > 0) {
  const signScript = path.join(__dirname, "sign-local.ps1");
  if (fs.existsSync(signScript)) {
    execFileSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        signScript,
        "-Thumbprint",
        signThumbprint,
        "-Files",
        ...shippedFiles,
      ],
      { stdio: "inherit" }
    );
  }
}
