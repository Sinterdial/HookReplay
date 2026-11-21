# HookReplay 架构与设计文档

> 本文档记录 **HookReplay（Tauri v2 + Rust + Vue 3）** 的核心架构、模块划分与关键工程决策。开源协议为 [Apache License 2.0](./LICENSE)，版本更新记录见 [CHANGELOG.md](./CHANGELOG.md)。

## 1. 架构概览

HookReplay 采用 **Tauri v2（Rust 原生后端） + Vue 3 / TypeScript / Tailwind CSS v4（WebView2 轻量前端）** 架构：

1. **按需桌面运行、零后台残留与便携启动防闪退**：
   - 无后台服务进程，关闭窗口即退出释放全部资源；Release 版本禁用 Windows 控制台黑框。
   - **WebView2 目录隔离与双阶段容错**：`lib.rs` 在 `.setup()` 阶段按当前可执行文件路径 FNV-1a 哈希分配独立 `webview2-bin-<hash>` 数据目录，使便携版与已安装版并发启动互不争抢 `EBWebView` 文件锁（规避 `0x800700AA`）；若被占用则自动回退到进程级临时目录 `webview2-inst-<pid>-<ts>` 并重试创建窗口，同时配置全局 Panic Hook、`crash.log` 与原生 `MessageBoxW` 错误提示弹窗。
   - **完整性级别保障**：`scripts/package-release.mjs` 先在正常目录暂存再通过同卷原子移动部署至目标工作区以保留 `Medium` 完整性标签，并在检测到 `Low Mandatory Level` 时自动尝试 `icacls` 校准与提示，杜绝直接双击触发 `0xC0000142` DLL 初始化失败。
2. **三态视觉主题 + 扩展 Gemini 流光动效 + 本机字体与可定制工作台布局**：
   - 支持 `system`（跟随系统）/ `light` / `dark` 三态无缝切换，支持枚举系统已安装字体独立定制界面正文字体、代码等宽字体；全界面采用 `rem` 相对缩放（界面字号 `11px–22px` + 代码/JSON 独立字号 `10px–24px`），支持主界面实时预览与 `Ctrl + / - / 0` 快捷缩放；顶栏采用自适应弹性包裹与 `overflow-hidden` 防重叠布局。
   - 桌面工作台支持分栏宽度 Pointer Events 拖拽调节（双击还原默认，视口动态防挤压并持久化于 `localStorage`）以及 Notion 风格一键切换悬浮自动隐藏（边缘感应延时滑出、表单聚焦防误收、点击主区 / `Esc` 收起，支持 `Ctrl+\` / `Ctrl+Shift+\`）。
   - 流光动效覆盖顶部装饰条（`.aurora-top-bar`）、徽标光环（`.aurora-ring-soft`）、导航激活底线（`.aurora-tab-active`）、交互卡片悬浮（`.aurora-card-interactive`）、选中行流光竖条（`.aurora-row-active`）与操作按钮，支持 `rich` / `subtle` / `off` 三档强度及系统 `prefers-reduced-motion` 自动降级。
3. **首次配置引导向导与多档案/模板体系**：
   - 首次安装或未完成引导时自动弹出 4 步 `SetupWizard`，引导配置 YAML 档案、发送地址与 Ping 验通、原始测试消息文件/文件夹试扫描及字体外观。
   - 支持同时挂载多个自定义 YAML/JSONC 档案、自定义配置扫描目录、编辑别名一键切换，以及从编译内嵌的 3 套模板（`n8n-webhook` / `generic-webhook` / `minimal`）新建配置。
4. **双源配置合并引擎（`YAML > GUI > Default`）**：
   - `.yaml`/`.yml`/`.jsonc` 中显式填写的非空值（含 `false` 与 `0`）优先覆盖 GUI；
   - 缺失或空值（`""`/`null`/`[]`/`{}`）自动回退到 `%APPDATA%\com.hookreplay.studio\gui-config.json` 与默认值；
   - 界面实时显示字段来源徽章，支持可视化表单 ↔ YAML 源码切换与分开持久化。
5. **大文件流式索引、JSON 高亮预览与历史归档去重**：
   - 流式扫描日志生成分段目录树与 `byte_offset` 索引，点击右侧检视器时按偏移回读原始 JSON；支持 XSS 安全的 JSON/JSONL 语法高亮、Webhook 关键字段突显与关键词定位搜索。
   - 临时粘贴台（`ScratchpadModal`）支持实时 JSON 高亮预览，并支持按 `#` 注释分类持久化追加或归类插入到指定测试文件/目录。
   - 每次批量运行（`run-YYYYMMDD-HHMMSS-<id>.json`）与单条直接重发（`single-YYYYMMDD-HHMMSS-<id>.json`）均独立归档并兼容写入 `latest.json`；历史扫描同时对规范化路径与 `runId`/运行签名去重。

---

## 2. 模块清单

| 模块路径 | 核心职责 |
| :--- | :--- |
| `src-tauri/src/main.rs` | Release 无黑框桌面二进制入口 |
| `src-tauri/src/lib.rs` | AppState、WebView2 隔离容错建窗、异常弹窗与全部 IPC Command 注册中心 |
| `src-tauri/src/fonts.rs` | 基于 Win32 `EnumFontFamiliesExW` 的零依赖本机已安装字体枚举（含等宽标识） |
| `src-tauri/src/config.rs` | 28 项+V2 配置模型、YAML > GUI 优先级合并、多档案挂载/目录扫描、内置模板、外观与引导状态持久化 |
| `src-tauri/src/parser.rs` | 多文件/目录扫描、# 注释分段、流式索引与 `Byte Offset` 懒加载、临时消息分类解析与持久化写入 |
| `src-tauri/src/mutator.rs` | `testMark` 多位置注入、5 种 Auth 模式、HMAC-SHA256/SHA1 签名及脱敏头/URL 生成 |
| `src-tauri/src/matcher.rs` | `labelRules` 通用 JSON 点路径摘要提取与 `assertions`/`bodyRules` 响应评估 |
| `src-tauri/src/runner.rs` | Semaphore 并发池、`interactive` 逐段确认、Ping、单条重发、时间戳历史归档与规范化去重读取/删除 |
| `src/utils/jsonHighlight.ts` | 无依赖 XSS 安全 JSON/JSONL 词法高亮与核心字段突显引擎 |
| `src/composables/useTheme.ts` | `system / light / dark` 三态主题、本机字体 CSS Token 与流光强度控制 |
| `src/composables/useWorkspaceLayout.ts` | 三栏与历史清单拖拽调宽持久化、视口防挤压上限及 Notion 悬浮抽屉状态机 |
| `src/composables/useAppState.ts` | 全局工作台响应式状态、档案挂载/模板、Scratchpad 归档、向导状态与事件监听 |
| `src/components/*.vue` | `TopBar`、`SourceSidebar`、`BatchList`、`ConfigPanel`、`InspectorPane`、`ReportHistory`、`SplitResizer`、`JsonViewer`、`AppearanceModal`、`SetupWizard`、`ScratchpadModal` |

---

## 3. 项目级依赖虚拟环境（`.venv`）与打包机制

- 前端 npm 缓存与 Rust Cargo `CARGO_HOME` 均通过 `scripts/venv-exec.mjs` + `.npmrc` 隔离在项目 `.venv/` 目录下，避免污染系统全局缓存。
- Windows 安装包使用 Tauri 内置 NSIS（`installMode: "both"`），支持自定义安装路径与中文安装界面；`scripts/package-release.mjs` 统一输出单文件与安装器至 `release/`，并支持通过 `--deploy-dir` 与 `--sign-thumbprint` 按需部署及签名。

## 4. 安全策略、Windows 代码签名与 GitHub CI/Release

- **严格内容安全策略（CSP）**：`src-tauri/tauri.conf.json` 已启用严格 CSP（`default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'`），阻止内联脚本与外部未授权资源加载。
- **发布者元数据**：`src-tauri/tauri.conf.json` 已声明 `bundle.publisher = "Sinterdial"` 与 `copyright`；若安装时选择「仅为当前用户安装」，无需管理员 UAC 提权（未签名的「以管理员安装」则必然弹出黄色未知发布者 UAC）。
- **本机 / 团队内网自签名（零成本）**：仓库提供纯 PowerShell 工具 [scripts/sign-local.ps1](./scripts/sign-local.ps1)，可通过 `-InitCert` 在 `Cert:\CurrentUser\My` 创建自签代码签名证书、`-TrustCurrentUser` 显式导入当前用户信任库，并在打包时通过 `--sign-thumbprint` 自动完成 SHA256 Authenticode 签名（所有证书私钥路径均已加入 `.gitignore`）。
- **公网公信分发与 CI/CD**：公网分发彻底消除 SmartScreen 提示需受微软根信任的证书（如 Microsoft Trusted Signing、SignPath.io 或商业 OV/EV 证书）。仓库内置 [.github/workflows/ci.yml](./.github/workflows/ci.yml) 与 [.github/workflows/release.yml](./.github/workflows/release.yml)，在 `windows-latest` Runner 自动直连官方依赖源执行构建、测试与 GitHub Releases 产物发布。
