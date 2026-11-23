# HookReplay · 通用 HTTP / Webhook 批量回放与调试工作台

> 一款**双击即开、关窗即退、平时零后台占用**的 **Windows 10 / 11 (x64)** 轻量级桌面应用（基于 Tauri v2 + Rust + Vue 3，[Apache-2.0](./LICENSE) 开源，Publisher: **Sinterdial**），兼顾 **n8n 工作流日志分段回放** 与 **通用 Webhook / REST API 批量调试**。版本变更详见 [CHANGELOG.md](./CHANGELOG.md)。

> **平台支持说明**：当前预编译发布与 CI 验证针对 **Windows 10 / 11 (x64)**（依赖系统内置 Edge WebView2 运行时）；字体枚举与启动容错对非 Windows 平台已预留条件编译适配。
<p align="center">
  <a href="https://github.com/Sinterdial/HookReplay/stargazers"><img alt="GitHub Stars" src="https://img.shields.io/github/stars/Sinterdial/HookReplay?style=flat-square&color=EAB308" /></a>
  <a href="https://github.com/Sinterdial/HookReplay/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/github/license/Sinterdial/HookReplay?style=flat-square&color=6366F1" /></a>
  <img alt="Top Language" src="https://img.shields.io/github/languages/top/Sinterdial/HookReplay?style=flat-square&color=0EA5E9" />
  <img alt="Last Commit" src="https://img.shields.io/github/last-commit/Sinterdial/HookReplay?style=flat-square&color=10B981" />
  <a href="https://linux.do"><img alt="LINUX DO" src="https://img.shields.io/badge/LINUX%20DO-Community-F59E0B?style=flat-square" /></a>
</p>

---

## 核心特性

- **即开即用，便携与安装版并行**：原生桌面窗口进程，无后台常驻服务；按可执行文件路径隔离 WebView2 数据目录并配备异常容错，直接运行便携版稳定不闪退。
- **首次使用配置向导**：首次启动自动进入 4 步配置引导向导，一站式配置挂载文件、发送目标地址与 Ping 验通、原始测试消息文件/文件夹与字体外观。
- **多配置文件挂载与内置模板**：支持自定义路径挂载、多文件同时挂载、自定义扫描目录、别名编辑与一键切换，并内置 **n8n Webhook / 通用 Webhook / 精简配置** 3 套 YAML 模板快速生成。
- **自由工作台分栏与 Notion 风格一键悬浮**：左栏、右栏检视窗与历史报告清单均支持拖拽分隔线调整宽度（双击还原默认，自动记忆），并可一键切换为「悬浮自动隐藏」模式——鼠标移至边缘自动滑出展开、移出自动收起（快捷键 `Ctrl+\` / `Ctrl+Shift+\`），最大化主视图空间。
- **本机字体、全量双字号缩放与扩展流光视觉**：支持 **跟随系统 / 浅色 / 深色** 主题切换，支持枚举本机已安装字体分别定制界面正文字体、JSON 等宽字体，并支持 **界面基准字号（`11–22px`）** 与 **代码/JSON 独立字号（`10–24px`）** 全量等比缩放、弹窗实时预览及 **`Ctrl +` / `Ctrl -` / `Ctrl 0`** 快捷缩放；Gemini 流光动效覆盖顶栏、导航激活、卡片悬浮、选中条纹与交互按钮，支持 `丰富 / 轻量 / 关闭` 三档强度。
- **JSON 语法高亮与关键词突显**：请求体预览、发送体、响应体与历史明细全面支持 JSON 着色、Webhook 重点字段（`type`、`title`、`status`、`deduped`、`__test` 等）突显与搜索定位。
- **临时消息高亮预览与分类归档**：临时粘贴调试台支持左写右看 JSON 高亮实时预览，并可将粘贴的消息按 `#` 注释分类持久化追加或插入到指定测试文件/文件夹。
- **独立历史归档与零重复**：每次批量测试（`run-*`）与单条直接重发（`single-*`）均自动按时间戳生成独立历史报告（同时兼容写入 `latest.json`），支持查看详情、单条删除与异常项一键复测，杜绝覆盖或重复显示两份。
- **目标网址一键切换与大文件秒开**：顶栏一键互换 n8n `/webhook/` ⇄ `/webhook-test/`、支持 `endpoints` 预设与 `Ping` 探测；15MB+ 大文件流式字节偏移懒加载，支持在线改写单条重发。

---

## 安装与运行方式

请前往本仓库的 **GitHub Releases** 页面下载预编译产物（或按文末「源码构建指南」自行打包至本地 `release/` 目录），提供两种使用形态：

### 方式一：使用安装程序（支持自定义安装目录）
1. 下载并双击运行 **`HookReplay_<version>_x64-setup.exe`**（例如 `HookReplay_1.2.0_x64-setup.exe`）。
2. 根据向导提示选择安装范围：**仅为当前用户安装** 或 **为计算机所有用户安装**。
3. 在「选择安装位置」步骤中点击 **浏览…**，即可自由指定任意盘符或自定义文件夹完成安装，并自动创建桌面及开始菜单快捷方式（含标准卸载程序）。

### 方式二：绿色便携单文件版
1. 直接双击 **`HookReplay.exe`** 即可运行，无需安装或额外运行时（可与已安装版同时独立运行）。
2. 建议放入工具文件夹中使用（依赖 Windows 10/11 内置 Edge WebView2 运行时）。

> **配置与历史存放位置**：无论软件安装到 `C:\Program Files` 还是其他路径，图形界面保存的默认配置、挂载档案清单与字体偏好都会独立存放在 `%APPDATA%\com.hookreplay.studio\gui-config.json`；未指定 `report` 路径时的历史运行报告自动归档在 `%APPDATA%\com.hookreplay.studio\history\`。

---

## 解决 Windows 安装/运行提示「未知发布者」

Windows SmartScreen 与 UAC 对**未经受信任代码签名证书签名**的所有新建 `.exe` / 安装包都会默认显示「未知发布者」，可用以下方案按场景处理：

1. **本机日常使用最简方案**：
   - 安装时选择 **「仅为当前用户安装」**（无需触发系统级黄色管理员 UAC 提示）；
   - 若从网络或微信等下载后被标记锁定，右键 EXE「属性」勾选 **解除锁定**（或在 PowerShell 执行 `Unblock-File .\HookReplay.exe`）。
2. **本机 / 团队内部彻底消除「未知发布者」（自签名证书）**：
   - 项目内置脚本 `scripts/sign-local.ps1`（构建时不会擅自修改系统证书，仅按需手动执行）：
     ```powershell
     # 1. 创建本机自签代码签名证书
     npm run sign:local -- -InitCert
     # 2. （可选）将该证书加入当前用户受信任根与发布者列表
     npm run sign:local -- -TrustCurrentUser
     # 3. 为 release/ 下的便携版与安装包签名
     npm run sign:local
     ```
3. **对外公开发布（所有陌生电脑直接信任）**：
   - 需使用第三方权威机构颁发的受微软根信任代码签名证书（如 Microsoft Trusted Signing、SignPath 开源免费签名、或商业 OV/EV 代码签名证书）在打包后执行签名并附带时间戳。

---

## 界面导览与常用操作

### 1. 顶部主控栏
- **配置档案一键切换**：下拉切换已挂载的自定义 YAML 档案、扫描目录中的配置或「仅使用 GUI 独立配置」；点击 **挂载YAML** 可一次多选挂载多个配置文件，点击 **模板** 可快速前往模板生成区。
- **目标 URL 与一键切换**：支持 n8n `/webhook/` ⇄ `/webhook-test/` 毫秒互换、`endpoints` 预设切换与 **Ping** 探测。
- **执行控制与外观向导**：**发送选中**、**全部运行**、**暂停 / 继续 / 停止**，以及 **字体外观**（本机字体、界面/代码双字号设置与快捷键 `Ctrl +/-/0`、流光档位）、**向导**（重新启动配置向导）与 **三态主题切换**。

### 2. 左栏（数据源 / 配置中心 / 历史报告）与工作台分栏
- **分栏调宽与一键悬浮**：拖动左右分栏边缘（或历史报告清单边缘）即可自由调整宽度，双击分隔线恢复默认；点击标题栏右侧收起/图钉按钮（或按 **`Ctrl+\`** / **`Ctrl+Shift+\`**）可一键将侧栏转为悬浮自动隐藏，鼠标移到对应屏幕边缘自动展开，移出自动收起。
- **数据源页签**：
  - 点击右上角图标添加单个或多个日志文件、日志目录，或点击 **临时粘贴 JSON / JSONL 调试** 打开双栏工作台（支持实时 JSON 高亮预览与按分类持久化写入文件/文件夹）；
  - 支持按关键词搜索、事件类型胶囊筛选、仅查看异常（未匹配/失败）项、批量全选/反选与 `#` 注释分段目录树定位。
- **配置中心页签**：
  - 顶部区域支持自定义路径/多选挂载配置文件、添加配置扫描目录、修改别名一键切换与内置模板预览生成；
  - 完整图形化表单覆盖全部原生配置项及 V2 扩展项，支持 **可视化表单 ↔ YAML 源码** 切换与分开持久化。
- **历史报告页签**：
  - 自动按时间倒序列出所有归档测试报告（含批量回放与单条直接重发记录，已去重），支持展开查看每条请求/响应 JSON 高亮、删除历史报告与 **一键仅勾选异常项复测**。

### 3. 中栏（分段卡片与交互确认）
- 开启 `interactive: true`（逐段确认）时，每段发送前顶部亮起流光确认横幅，支持快捷键 **`Y`/`Enter`**（发送本段）、**`N`**（跳过本段）、**`A`**（剩余全自动）、**`Q`/`Esc`**（停止）。
- 每个分段头提供 **发本段** 快捷按钮，运行中的分段与选中条目会同步点亮流光指示。

### 4. 右栏（JSON 高亮检视与在线重发）
- **请求体 Payload**：默认进入 **高亮预览**（语法着色、关键字段突显与关键词搜索定位），双击内容或切到 **源码编辑** 即可直接改写 JSON，支持 **格式化 / 压缩 / 还原 / 单条重发**。
- **请求头 & URL / 响应 Response**：展示脱敏 URL、全部请求头以及带高亮的最终请求 Body 与服务器响应体。

---

## 双源配置优先级规则详解（YAML > GUI > 默认值）

为兼顾「手写 YAML 档案版本管理」与「在界面里随手修改参数」：

1. **优先读取当前绑定的 `.yaml` / `.yml` / `.jsonc`**：凡是在 YAML 里**明确填写的非空字段**，一律覆盖 GUI 设置，并在配置中心右侧展示 **`YAML 优先`** 徽章。
2. **空值自动回退到 GUI**：如果 YAML 里未写该字段，或者写为空值（如 `""`、`null`、`[]`、`{}`），则自动使用你在 GUI 保存的 **`gui-config.json`** 配置；若 GUI 也未改过，则回退系统默认值。
3. **`false` 与 `0` 属于有效覆盖**：在 YAML 里显式写入 `interactive: false` 或 `delayMs: 0`，属于明确的显式配置，仍会优先覆盖 GUI。

---

## 完整配置项说明

| 分组 | 配置键 | 说明 | 默认值 / 可选值 |
| :--- | :--- | :--- | :--- |
| **基础与入口** | `url` / `method` | 目标请求地址与 HTTP 方法 | `POST` / `PUT` / `PATCH` / `GET` |
| | `endpoints` | 命名入口预设数组 `[{ name, url }]`，供顶栏快速切换 | `[]` |
| | `insecure` | 是否跳过 HTTPS 自签名证书校验 | `false` |
| **认证与签名** | `auth` | `{ type, username, password, token, headerName, queryName }` | `type`: `none`/`basic`/`bearer`/`header`/`query` |
| | `headers` | 自定义附加 HTTP 请求头键值对 | `{}` |
| | `signature` | Webhook HMAC 请求体签名：`enabled`, `algorithm`, `headerName`, `prefix`, `secret` | `hmac-sha256` / `hmac-sha1` |
| **流控与确认** | `interactive` | 每段发送前是否暂停弹窗/横幅询问 `y/n/a/q` | `true` |
| | `concurrency` | 同分段内并发请求数（入库聚合测试请保持 `1`） | `1` |
| | `delayMs` / `segmentWaitMs` | 同段条间延迟 / 段与段之间等待间隔（毫秒） | `100` / `0` |
| | `timeoutMs` / `retries` / `retryDelayMs` | 超时毫秒、网络/5xx 重试次数及重试等待 | `15000` / `0` / `800` |
| **测试标记** | `markTest` | 兼容开关：`true` 时自动在 JSON 根注入 `__test: true` | `true` |
| | `testMark` | V2 细粒度标记：`{ mode, field, value }` | `mode`: `body`/`header`/`query`/`none` |
| **数据源与筛选** | `inputs` / `recursive` / `extensions` | 输入日志路径列表、递归扫描与允许后缀 | `["txt","log","json"]` |
| | `format` / `commentPrefixes` | 解析格式与注释分段前缀 | `auto` / `["#", "//"]` |
| | `types` / `include` / `exclude` / `limit` / `reverse` | 类型白名单、正则过滤、条数上限、倒序发送 | - |
| **通用与断言** | `labelRules` | `{ preset, typePaths, titlePaths }`，适配任意工作流 JSON 结构 | `preset`: `auto`/`n8n-renotify`/`generic` |
| | `assertions` | `{ expect, softFailStatus, strict, bodyRules }`，细分响应状态与幂等跳过规则 | `expect: [200]`, `softFailStatus: [400]` |
| **输出与变量** | `report` / `saveResponseDir` | JSON 报告输出路径（自动归档同目录时间戳历史）与逐条响应目录 | - |
| | `variables` | 模板变量字典（在 URL/Header/Body 使用 `{{VAR}}`） | 内置 `{{$timestamp}}`、`{{$timestampMs}}`、`{{$isoTime}}`、`{{$uuid}}` |

---

## 源码架构与构建指南（供维护与二次开发）

项目将 npm 与 Cargo 依赖缓存完全隔离在工程目录内的 `.venv/` 与 `node_modules/` 下，不会向系统全局目录写入项目包缓存：

```powershell
# 1. 进入项目根目录
cd HookReplay

# 2. 使用隔离封装脚本执行 Rust 单元测试
.\scripts\venv-exec.cmd cargo test --manifest-path .\src-tauri\Cargo.toml

# 3. 执行前端 TypeScript 类型检查与 Vite 构建
npm run build

# 4. 打包便携单文件与 NSIS 安装包
.\scripts\venv-exec.cmd npx tauri build

# 5. 输出交付物至 release/ 目录，并保障 Windows Medium 完整性级别（可用 --deploy-dir 指定额外部署目录）
npm run package:release
```

更详细的架构与接口设计请参阅 [DESIGN.md](./DESIGN.md)，版本历史请参阅 [CHANGELOG.md](./CHANGELOG.md)。

---

## 开源协议与发布者

- **开源协议**：[Apache License 2.0](./LICENSE)
- **发布者 / 维护者**：**Sinterdial**
