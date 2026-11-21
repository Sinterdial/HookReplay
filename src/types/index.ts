/**
 * HookReplay 前端共享 TypeScript 类型定义
 *
 * 与 Rust 后端 `#[serde(rename_all = "camelCase")]` 结构体保持严格一一对应。
 */

/** 界面外观主题模式：跟随系统 / 浅色 / 深色 */
export type ThemeMode = "system" | "light" | "dark";

/** 流光动效强度档位：丰富 / 轻量 / 关闭 */
export type AuroraIntensity = "rich" | "subtle" | "off";

/** 本机已安装字体元数据 */
export interface SystemFontInfo {
  name: string;
  isMonospace: boolean;
}

/** 内置配置模板元数据 */
export interface ConfigTemplateInfo {
  id: string;
  name: string;
  description: string;
  suggestedFileName: string;
  content: string;
}

/** 临时粘贴消息持久化保存到文件/目录的请求参数 */
export interface PersistScratchpadRequest {
  rawText: string;
  targetMode: "file" | "directory";
  targetPath: string;
  newFileName?: string | null;
  category: string;
  insertIntoExistingCategory: boolean;
  commentPrefix: string;
}

/** 临时消息持久化保存结果 */
export interface PersistScratchpadResult {
  savedPath: string;
  writtenCount: number;
  category: string;
  insertedIntoExisting: boolean;
}

/** 左栏导航页签 */
export type LeftTab = "sources" | "config" | "reports";

/** 字段来源标识：YAML 配置覆盖 / GUI 图形配置 / 系统默认值 */
export type FieldSource = "yaml" | "gui" | "default";

/** 预设目标入口（Endpoint Preset），用于在顶栏一键切换环境 */
export interface EndpointPreset {
  name: string;
  url: string;
}

/** 认证配置（注意字段 `type` 与 Rust `#[serde(rename = "type")]` 对应） */
export interface AuthConfig {
  type: "none" | "basic" | "bearer" | "header" | "query" | string;
  username: string;
  password: string;
  token: string;
  headerName: string;
  queryName: string;
}

/** Webhook HMAC 签名配置 */
export interface SignatureConfig {
  enabled: boolean;
  algorithm: "hmac-sha256" | "hmac-sha1" | string;
  headerName: string;
  prefix: string;
  secret: string;
}

/** 精细化测试标记注入配置 */
export interface TestMarkConfig {
  mode: "body" | "header" | "query" | "none" | string;
  field: string;
  value: unknown;
}

/** 通用 JSON 摘要路径提取规则 */
export interface LabelRulesConfig {
  preset: "auto" | "n8n-renotify" | "generic" | string;
  typePaths: string[];
  titlePaths: string[];
}

/** 响应体 JSON 细粒度断言规则 */
export interface BodyRule {
  path: string;
  equals: unknown;
  badge: string;
  tone: "success" | "info" | "warning" | "error" | string;
}

/** 响应判定与断言配置 */
export interface AssertionsConfig {
  expect: number[];
  softFailStatus: number[];
  strict: boolean;
  bodyRules: BodyRule[];
}

/** 应用核心配置结构（覆盖原有 28 个字段 + V2 通用扩展字段） */
export interface AppConfig {
  name: string;
  description: string;
  url: string;
  method: string;
  endpoints: EndpointPreset[];
  auth: AuthConfig;
  headers: Record<string, string>;
  signature: SignatureConfig;
  timeoutMs: number;
  retries: number;
  retryDelayMs: number;
  concurrency: number;
  delayMs: number;
  segmentWaitMs: number;
  interactive: boolean;
  markTest: boolean;
  testMark: TestMarkConfig | null;
  insecure: boolean;
  inputs: string[];
  recursive: boolean;
  extensions: string[];
  format: "auto" | "jsonl" | "json" | string;
  commentPrefixes: string[];
  types: string[];
  include: string;
  exclude: string;
  limit: number;
  reverse: boolean;
  expect: number[];
  strict: boolean;
  labelRules: LabelRulesConfig;
  assertions: AssertionsConfig;
  report: string;
  saveResponseDir: string;
  quiet: boolean;
  variables: Record<string, string>;
}

/** 自动探测或手动挂载的 YAML 档案摘要 */
export interface ProfileSummary {
  name: string;
  path: string;
  exists: boolean;
  sourceKind: "mounted" | "dir" | "auto" | string;
}

/** 后端返回的完整配置快照（含 YAML > GUI 合并结果与逐字段来源标注） */
export interface ConfigSnapshot {
  effectiveConfig: AppConfig;
  guiConfig: AppConfig;
  theme: ThemeMode | string;
  uiFont: string;
  monoFont: string;
  fontSize: number;
  codeFontSize: number;
  auroraIntensity: AuroraIntensity | string;
  onboardingCompleted: boolean;
  customProfileDirs: string[];
  yamlPath: string | null;
  baseDir: string;
  rawYaml: string;
  fieldSources: Record<string, FieldSource>;
  profiles: ProfileSummary[];
  yamlError: string | null;
}

/** 单条日志消息的轻量偏移索引 */
export interface IndexedEntry {
  index: number;
  segment: number;
  comment: string;
  source: string;
  sourceName: string;
  line: number;
  byteOffset: number;
  byteLen: number;
  inlineRaw?: string;
  eventType: string;
  title: string;
  label: string;
}

/** 分段摘要（来自 # / // 注释边界或独立文件） */
export interface SegmentSummary {
  segment: number;
  comment: string;
  sourceName: string;
  startLine: number;
  count: number;
}

/** 日志解析失败行记录 */
export interface ParseErrorItem {
  source: string;
  line: number;
  error: string;
}

/** 数据源扫描结果 */
export interface SourceScanResult {
  files: string[];
  missingPaths: string[];
  entries: IndexedEntry[];
  segments: SegmentSummary[];
  parseErrors: ParseErrorItem[];
  commentsSkipped: number;
  totalBeforeFilter: number;
  availableTypes: string[];
}

/** 单条执行结果（含脱敏请求视图与完整响应体） */
export interface RunItemResult {
  index: number;
  segment: number;
  source: string;
  line: number;
  label: string;
  eventType: string;
  status: number;
  outcome: "ok" | "deduped" | "unmatched" | "failed" | string;
  badge: string;
  ok: boolean;
  unmatched: boolean;
  ms: number;
  error: string;
  response: string;
  finalUrl: string;
  maskedHeaders: Record<string, string>;
  sentBody: string;
}

/** 历史报告列表摘要 */
export interface ReportSummary {
  runId: string;
  fileName: string;
  path: string;
  startedAt: string;
  finishedAt: string;
  target: string;
  total: number;
  ok: number;
  deduped: number;
  unmatched: number;
  failed: number;
  durationMs: number;
}

/** 完整历史报告文件结构 */
export interface FullRunReport {
  runId: string;
  version: string;
  startedAt: string;
  finishedAt: string;
  target: string;
  auth: string;
  strict: boolean;
  markTest: boolean;
  total: number;
  ok: number;
  deduped: number;
  unmatched: number;
  failed: number;
  parseErrors: number;
  commentsSkipped: number;
  segments: number;
  segmentWaitMs: number;
  interactive: boolean;
  skippedItems: number;
  skippedSegments: number;
  durationMs: number;
  files: string[];
  results: RunItemResult[];
}

/** 分段交互确认事件载荷（run://segment-waiting） */
export interface SegmentWaitingPayload {
  runId: string;
  segmentIndex: number;
  totalSegments: number;
  segmentId: number;
  comment: string;
  sourceName: string;
  startLine: number;
  count: number;
}

/** 单条完成进度事件载荷（run://item-done） */
export interface ItemProgressPayload {
  runId: string;
  doneCount: number;
  totalSelected: number;
  currentSegment: number;
  result: RunItemResult;
}

/** 运行状态变更事件载荷（run://status） */
export interface RunStatusPayload {
  runId: string;
  state: string;
  message: string;
}

/** 段间等待倒计时事件载荷（run://segment-wait-countdown） */
export interface SegmentWaitCountdownPayload {
  runId: string;
  waitMs: number;
  nextSegment: number;
}

/** Ping 连通性探测结果 */
export interface PingResult {
  reachable: boolean;
  status: number;
  ms: number;
  message: string;
}

/** 交互确认动作类型 */
export type SegmentAction = "y" | "n" | "a" | "q";

/** 任务生命周期状态 */
export type RunLifecycleState =
  | "idle"
  | "running"
  | "paused"
  | "waiting_confirm"
  | "finished"
  | "aborted";

/** Toast 轻提示条目 */
export interface ToastItem {
  id: number;
  message: string;
  tone: "info" | "success" | "warning" | "error";
}
