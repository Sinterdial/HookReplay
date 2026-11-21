/**
 * HookReplay 全局响应式工作台状态中心（Store Composable）
 *
 * 职责覆盖：
 * 1. 管理 ConfigSnapshot 与用户当前内存可编辑工作配置（workingConfig）；
 * 2. 驱动数据源扫描、分段目录树选中集合、搜索与类型过滤；
 * 3. 管理批量运行状态机、Tauri Event 流式进度、交互确认横幅（y/n/a/q）与热键；
 * 4. 懒加载右侧单条请求负载详情并支持在线修改重发；
 * 5. 管理历史测试报告列表与失败条目一键重跑。
 */
import { computed, reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppConfig,
  ConfigSnapshot,
  ConfigTemplateInfo,
  FullRunReport,
  IndexedEntry,
  ItemProgressPayload,
  LeftTab,
  PersistScratchpadRequest,
  PersistScratchpadResult,
  PingResult,
  ReportSummary,
  RunItemResult,
  RunLifecycleState,
  RunStatusPayload,
  SegmentAction,
  SegmentWaitCountdownPayload,
  SegmentWaitingPayload,
  SourceScanResult,
  ToastItem,
} from "../types";
import { useTheme } from "./useTheme";
import { useWorkspaceLayout } from "./useWorkspaceLayout";

/** 生成一份安全兜底的默认 AppConfig，确保首屏同步渲染前无空引用 */
function createEmptyConfig(): AppConfig {
  return {
    name: "默认配置档案",
    description: "",
    url: "",
    method: "POST",
    endpoints: [],
    auth: {
      type: "none",
      username: "",
      password: "",
      token: "",
      headerName: "Authorization",
      queryName: "token",
    },
    headers: {},
    signature: {
      enabled: false,
      algorithm: "hmac-sha256",
      headerName: "X-Hub-Signature-256",
      prefix: "sha256=",
      secret: "",
    },
    timeoutMs: 15000,
    retries: 0,
    retryDelayMs: 800,
    concurrency: 1,
    delayMs: 100,
    segmentWaitMs: 0,
    interactive: true,
    markTest: true,
    testMark: null,
    insecure: false,
    inputs: [],
    recursive: false,
    extensions: ["txt", "log", "json"],
    format: "auto",
    commentPrefixes: ["#", "//"],
    types: [],
    include: "",
    exclude: "",
    limit: 0,
    reverse: false,
    expect: [200],
    strict: false,
    labelRules: {
      preset: "auto",
      typePaths: [
        "type",
        "event_type",
        "event",
        "action",
        "status",
      ],
      titlePaths: [
        "data.mediainfo.title_year",
        "data.title",
        "data.subscribe_info.name",
        "entries[0].title",
        "entry.title",
        "title",
        "name",
        "subject",
      ],
    },
    assertions: {
      expect: [200],
      softFailStatus: [400],
      strict: false,
      bodyRules: [
        {
          path: "deduped",
          equals: true,
          badge: "幂等跳过",
          tone: "info",
        },
      ],
    },
    report: "",
    saveResponseDir: "",
    quiet: false,
    variables: {},
  };
}

/** 深拷贝对象工具（避免 Vue Reactive Proxy 影响序列化） */
export function cloneData<T>(obj: T): T {
  return JSON.parse(JSON.stringify(obj)) as T;
}

// ===========================
// 全局单例响应式状态
// ===========================
const leftTab = ref<LeftTab>("sources");
const isStartupShimmer = ref<boolean>(true);
const showScratchpadModal = ref<boolean>(false);
const showSetupWizard = ref<boolean>(false);
const showAppearanceModal = ref<boolean>(false);
const scratchpadDraft = ref<string>("");
const isScratchpadMode = ref<boolean>(false);

const snapshot = ref<ConfigSnapshot | null>(null);
const workingConfig = ref<AppConfig>(createEmptyConfig());
const rawYamlDraft = ref<string>("");
const configTemplates = ref<ConfigTemplateInfo[]>([]);

const scanResult = ref<SourceScanResult>({
  files: [],
  missingPaths: [],
  entries: [],
  segments: [],
  parseErrors: [],
  commentsSkipped: 0,
  totalBeforeFilter: 0,
  availableTypes: [],
});
const isScanning = ref<boolean>(false);

// 当前勾选待发条目的全局序号集合（IndexedEntry.index）
const selectedIndices = ref<Set<number>>(new Set());

// 数据列表筛选条件
const searchQuery = ref<string>("");
const selectedEventType = ref<string>("");
const onlyAbnormal = ref<boolean>(false);

// 右侧单条检视器（Inspector）状态
const activeEntry = ref<IndexedEntry | null>(null);
const activeEntryRaw = ref<string>("");
const editedEntryRaw = ref<string>("");
const isLoadingEntryRaw = ref<boolean>(false);
const isReplayingSingle = ref<boolean>(false);

// 批量执行生命周期与实时结果状态
const runState = ref<RunLifecycleState>("idle");
const activeRunId = ref<string>("");
const doneCount = ref<number>(0);
const totalSelected = ref<number>(0);
const currentSegment = ref<number>(0);
const resultsByIndex = reactive<Map<number, RunItemResult>>(new Map());
const lastReport = ref<FullRunReport | null>(null);
const segmentWaiting = ref<SegmentWaitingPayload | null>(null);
const waitCountdownMs = ref<number>(0);

// 连通性探测结果
const pingStatus = ref<PingResult | null>(null);
const isPinging = ref<boolean>(false);

// 历史报告列表与详情预览
const reportsList = ref<ReportSummary[]>([]);
const inspectedReport = ref<FullRunReport | null>(null);
const isLoadingReport = ref<boolean>(false);

// 通知轻提示栈
const toasts = ref<ToastItem[]>([]);
let nextToastId = 1;
let initialized = false;
const unlistenCallbacks: UnlistenFn[] = [];
let countdownTimer: number | null = null;

/** 推送一条右下角自动消失的轻提示 */
function pushToast(
  message: string,
  tone: ToastItem["tone"] = "info",
  durationMs = 3600
): void {
  const id = nextToastId++;
  toasts.value.push({ id, message, tone });
  window.setTimeout(() => {
    toasts.value = toasts.value.filter((t) => t.id !== id);
  }, durationMs);
}

/** 清理段间等待本地倒计时器 */
function clearWaitCountdown(): void {
  if (countdownTimer !== null) {
    window.clearInterval(countdownTimer);
    countdownTimer = null;
  }
  waitCountdownMs.value = 0;
}

export function useAppState() {
  const {
    hydrateAppearance,
    fontSize,
    codeFontSize,
    stepFontSizes,
    resetFontSizesOnly,
  } = useTheme();
  const { toggleFloating, closeAllFloatingDrawers } = useWorkspaceLayout();

  /** 根据搜索词、事件类型与「仅看异常」筛选条目 */
  const filteredEntries = computed<IndexedEntry[]>(() => {
    const q = searchQuery.value.trim().toLowerCase();
    const typeFilter = selectedEventType.value.trim();
    const abnormal = onlyAbnormal.value;

    return scanResult.value.entries.filter((entry) => {
      if (typeFilter && entry.eventType !== typeFilter) {
        return false;
      }
      if (abnormal) {
        const res = resultsByIndex.get(entry.index);
        if (!res || (res.outcome !== "failed" && res.outcome !== "unmatched")) {
          return false;
        }
      }
      if (q) {
        const hay =
          `${entry.index} ${entry.label} ${entry.eventType} ${entry.comment} ${entry.sourceName}`.toLowerCase();
        if (!hay.includes(q)) {
          return false;
        }
      }
      return true;
    });
  });

  /** 实时执行统计汇总 */
  const runStats = computed(() => {
    let ok = 0;
    let deduped = 0;
    let unmatched = 0;
    let failed = 0;
    for (const r of resultsByIndex.values()) {
      if (r.outcome === "deduped") deduped += 1;
      else if (r.outcome === "ok") ok += 1;
      else if (r.outcome === "unmatched") unmatched += 1;
      else if (r.outcome === "failed") failed += 1;
    }
    return { ok, deduped, unmatched, failed, totalDone: resultsByIndex.size };
  });

  /** 判断当前任务是否正在运行或挂起等待中 */
  const isBusy = computed(
    () =>
      runState.value === "running" ||
      runState.value === "paused" ||
      runState.value === "waiting_confirm"
  );

  /** 判断当前 URL 是否包含 n8n 标准路径，便于展示一键切换正式/测试开关 */
  const n8nUrlMode = computed<"prod" | "test" | null>(() => {
    const u = workingConfig.value.url;
    if (u.includes("/webhook-test/")) return "test";
    if (u.includes("/webhook/")) return "prod";
    return null;
  });

  /** 根据配置快照刷新工作台内存配置 */
  function applySnapshot(snap: ConfigSnapshot): void {
    snapshot.value = snap;
    workingConfig.value = cloneData(snap.effectiveConfig);
    rawYamlDraft.value = snap.rawYaml;
    hydrateAppearance(snap);
  }

  /** 读取当前后端配置快照 */
  async function loadConfigSnapshot(): Promise<void> {
    const snap = await invoke<ConfigSnapshot>("get_config_snapshot");
    applySnapshot(snap);
  }

  /** 扫描配置的输入源并默认全选条目 */
  async function scanSources(overrideCfg?: AppConfig | null): Promise<void> {
    isScanning.value = true;
    isScratchpadMode.value = false;
    try {
      const res = await invoke<SourceScanResult>("scan_input_sources", {
        configOverride: overrideCfg ?? cloneData(workingConfig.value),
      });
      scanResult.value = res;
      selectedIndices.value = new Set(res.entries.map((e) => e.index));
      // 尽量保持当前选中条目同步，若不在新列表则选中第一条
      if (res.entries.length > 0) {
        const currentId = activeEntry.value?.index;
        const matched = res.entries.find((e) => e.index === currentId);
        await selectActiveEntry(matched ?? res.entries[0]);
      } else {
        activeEntry.value = null;
        activeEntryRaw.value = "";
        editedEntryRaw.value = "";
      }
      if (res.missingPaths.length > 0) {
        pushToast(
          `有 ${res.missingPaths.length} 个配置路径不存在，请检查数据源设置`,
          "warning"
        );
      }
    } catch (err) {
      pushToast(`扫描日志失败：${String(err)}`, "error");
    } finally {
      isScanning.value = false;
    }
  }

  /** 解析临时粘贴 JSON / JSONL 文本到工作区 */
  async function loadScratchpadText(text: string): Promise<boolean> {
    try {
      const res = await invoke<SourceScanResult>("parse_scratchpad_text", {
        rawText: text,
        configOverride: cloneData(workingConfig.value),
      });
      scanResult.value = res;
      isScratchpadMode.value = true;
      selectedIndices.value = new Set(res.entries.map((e) => e.index));
      resultsByIndex.clear();
      if (res.entries.length > 0) {
        await selectActiveEntry(res.entries[0]);
      }
      pushToast(`临时调试台已加载 ${res.entries.length} 条消息`, "success");
      showScratchpadModal.value = false;
      leftTab.value = "sources";
      return true;
    } catch (err) {
      pushToast(`临时内容解析失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 按文件偏移懒加载单条原始 JSON 并在 Inspector 打开 */
  async function selectActiveEntry(entry: IndexedEntry): Promise<void> {
    activeEntry.value = entry;
    isLoadingEntryRaw.value = true;
    try {
      const raw = await invoke<string>("get_entry_raw", { entry });
      activeEntryRaw.value = raw;
      // 自动尝试美化缩进，提升在线改写可读性
      try {
        editedEntryRaw.value = JSON.stringify(JSON.parse(raw), null, 2);
      } catch {
        editedEntryRaw.value = raw;
      }
    } catch (err) {
      activeEntryRaw.value = "";
      editedEntryRaw.value = "";
      pushToast(`读取条目详情失败：${String(err)}`, "error");
    } finally {
      isLoadingEntryRaw.value = false;
    }
  }

  /** 切换绑定 YAML 档案（传 null 表示纯 GUI 模式） */
  async function selectYamlProfile(yamlPath: string | null): Promise<void> {
    try {
      const snap = await invoke<ConfigSnapshot>("select_yaml_profile", {
        yamlPath,
      });
      applySnapshot(snap);
      pingStatus.value = null;
      await scanSources(snap.effectiveConfig);
      await fetchReportsList(snap.effectiveConfig);
      pushToast(
        yamlPath ? "已切换 YAML 配置档案并应用优先级合并" : "已切换为纯 GUI 独立配置模式",
        "success"
      );
    } catch (err) {
      pushToast(`切换配置档案失败：${String(err)}`, "error");
    }
  }

  /** 挂载一个或多个外部 YAML/JSONC 配置文件 */
  async function mountYamlProfiles(
    paths: string[],
    activateFirst = true
  ): Promise<boolean> {
    try {
      const snap = await invoke<ConfigSnapshot>("mount_yaml_profiles", {
        paths,
        activateFirst,
      });
      applySnapshot(snap);
      if (activateFirst) {
        pingStatus.value = null;
        await scanSources(snap.effectiveConfig);
        await fetchReportsList(snap.effectiveConfig);
      }
      pushToast(`已成功挂载 ${paths.length} 个配置文件`, "success");
      return true;
    } catch (err) {
      pushToast(`挂载配置文件失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 卸载指定已挂载的配置文件 */
  async function unmountYamlProfile(path: string): Promise<boolean> {
    try {
      const wasActive = snapshot.value?.yamlPath === path;
      const snap = await invoke<ConfigSnapshot>("unmount_yaml_profile", {
        path,
      });
      applySnapshot(snap);
      if (wasActive) {
        await scanSources(snap.effectiveConfig);
        await fetchReportsList(snap.effectiveConfig);
      }
      pushToast("已从挂载列表移除该配置文件", "info");
      return true;
    } catch (err) {
      pushToast(`移除挂载失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 重命名已挂载配置文件别名 */
  async function renameYamlProfile(path: string, alias: string): Promise<boolean> {
    try {
      const snap = await invoke<ConfigSnapshot>("rename_yaml_profile", {
        path,
        alias,
      });
      applySnapshot(snap);
      pushToast("配置档案别名已更新", "success");
      return true;
    } catch (err) {
      pushToast(`更新档案别名失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 添加自定义配置扫描目录 */
  async function addProfileDir(dirPath: string): Promise<boolean> {
    try {
      const snap = await invoke<ConfigSnapshot>("add_profile_dir", {
        dirPath,
      });
      applySnapshot(snap);
      pushToast("已添加自定义配置扫描目录", "success");
      return true;
    } catch (err) {
      pushToast(`添加配置目录失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 移除自定义配置扫描目录 */
  async function removeProfileDir(dirPath: string): Promise<boolean> {
    try {
      const snap = await invoke<ConfigSnapshot>("remove_profile_dir", {
        dirPath,
      });
      applySnapshot(snap);
      pushToast("已移除自定义配置扫描目录", "info");
      return true;
    } catch (err) {
      pushToast(`移除配置目录失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 加载内置配置模板列表 */
  async function loadConfigTemplates(): Promise<ConfigTemplateInfo[]> {
    try {
      const list = await invoke<ConfigTemplateInfo[]>("list_config_templates");
      configTemplates.value = list;
      return list;
    } catch {
      return configTemplates.value;
    }
  }

  /** 从内置模板生成 YAML 配置文件并挂载激活 */
  async function createConfigFromTemplate(
    templateId: string,
    targetPath: string,
    mountAndActivate = true
  ): Promise<boolean> {
    try {
      const snap = await invoke<ConfigSnapshot>("create_config_from_template", {
        templateId,
        targetPath,
        mountAndActivate,
      });
      applySnapshot(snap);
      if (mountAndActivate) {
        pingStatus.value = null;
        await scanSources(snap.effectiveConfig);
        await fetchReportsList(snap.effectiveConfig);
      }
      pushToast(`已根据模板创建配置：${targetPath}`, "success");
      return true;
    } catch (err) {
      pushToast(`创建模板配置失败：${String(err)}`, "error");
      return false;
    }
  }

  /** 标记完成首次使用引导向导 */
  async function completeOnboarding(completed = true): Promise<void> {
    try {
      const snap = await invoke<ConfigSnapshot>("complete_onboarding", {
        completed,
      });
      applySnapshot(snap);
    } catch {
      // 忽略异常
    } finally {
      showSetupWizard.value = false;
    }
  }

  /** 读取目标日志文件中已有的注释分段分类列表 */
  async function inspectLogCategories(filePath: string): Promise<string[]> {
    if (!filePath.trim()) return [];
    try {
      return await invoke<string[]>("inspect_log_categories_cmd", {
        filePath,
        commentPrefixes: workingConfig.value.commentPrefixes,
      });
    } catch {
      return [];
    }
  }

  /** 列出指定测试目录下的可追加日志文件列表 */
  async function listDirLogFiles(dirPath: string): Promise<string[]> {
    if (!dirPath.trim()) return [];
    try {
      return await invoke<string[]>("list_dir_log_files_cmd", {
        dirPath,
        extensions: workingConfig.value.extensions,
      });
    } catch {
      return [];
    }
  }

  /** 将临时粘贴的原始消息持久化追加到指定测试文件/目录并刷新数据源 */
  async function persistScratchpadMessages(
    request: PersistScratchpadRequest,
    addToInputsAndScan = true
  ): Promise<PersistScratchpadResult | null> {
    try {
      const res = await invoke<PersistScratchpadResult>("persist_scratchpad_cmd", {
        request,
      });
      if (addToInputsAndScan) {
        const lowerSaved = res.savedPath.toLowerCase();
        const alreadyCovered = workingConfig.value.inputs.some((inp) => {
          const norm = inp.trim().toLowerCase().replace(/[\\/]+$/, "");
          return (
            norm === lowerSaved ||
            (norm.length > 0 && lowerSaved.startsWith(`${norm}\\`)) ||
            (norm.length > 0 && lowerSaved.startsWith(`${norm}/`))
          );
        });
        if (!alreadyCovered) {
          workingConfig.value.inputs.push(res.savedPath);
        }
        await scanSources(workingConfig.value);
        leftTab.value = "sources";
      }
      const modeHint = res.insertedIntoExisting
        ? `已归类插入分类「${res.category}」`
        : res.category
          ? `已按分类「${res.category}」追加`
          : "已追加保存";
      pushToast(`${modeHint}（共 ${res.writtenCount} 条）-> ${res.savedPath}`, "success");
      showScratchpadModal.value = false;
      return res;
    } catch (err) {
      pushToast(`持久化保存失败：${String(err)}`, "error");
      return null;
    }
  }

  /** 将当前表单编辑结果保存到 GUI 兜底配置文件 gui-config.json */
  async function saveAsGuiConfig(cfgToSave?: AppConfig): Promise<void> {
    try {
      const target = cloneData(cfgToSave ?? workingConfig.value);
      const snap = await invoke<ConfigSnapshot>("save_gui_config", {
        guiConfig: target,
      });
      applySnapshot(snap);
      pushToast("已保存到 GUI 默认配置（YAML 非空字段仍具更高优先级）", "success");
      await scanSources(snap.effectiveConfig);
    } catch (err) {
      pushToast(`保存 GUI 配置失败：${String(err)}`, "error");
    }
  }

  /** 将当前配置写回指定 YAML 文件 */
  async function saveToYamlFile(targetPath: string, cfgToSave?: AppConfig): Promise<void> {
    try {
      const target = cloneData(cfgToSave ?? workingConfig.value);
      const snap = await invoke<ConfigSnapshot>("save_yaml_from_config", {
        targetPath,
        config: target,
      });
      applySnapshot(snap);
      pushToast(`配置已同步写入 YAML：${targetPath}`, "success");
      await scanSources(snap.effectiveConfig);
      await fetchReportsList(snap.effectiveConfig);
    } catch (err) {
      pushToast(`写入 YAML 失败：${String(err)}`, "error");
    }
  }

  /** 校验并保存原始 YAML 源码 */
  async function saveRawYamlSource(targetPath: string, yamlText: string): Promise<void> {
    try {
      const snap = await invoke<ConfigSnapshot>("save_raw_yaml", {
        targetPath,
        rawYaml: yamlText,
      });
      applySnapshot(snap);
      pushToast("YAML 源码校验通过并已生效", "success");
      await scanSources(snap.effectiveConfig);
      await fetchReportsList(snap.effectiveConfig);
    } catch (err) {
      pushToast(`YAML 校验或保存失败：${String(err)}`, "error");
    }
  }

  /** 一键切换 n8n /webhook/ <-> /webhook-test/ 路径 */
  async function toggleN8nTargetUrl(): Promise<void> {
    const toggled = await invoke<string>("toggle_n8n_url", {
      url: workingConfig.value.url,
    });
    workingConfig.value.url = toggled;
    pingStatus.value = null;
    if (toggled.includes("/webhook-test/")) {
      pushToast("已切换至 n8n 测试入口（请在 n8n 画布点击 Execute workflow 后发送）", "info");
    } else if (toggled.includes("/webhook/")) {
      pushToast("已切换回 n8n 正式 Webhook 入口", "success");
    }
  }

  /** 探测当前目标 URL 连通性 */
  async function testEndpointPing(): Promise<void> {
    isPinging.value = true;
    try {
      const res = await invoke<PingResult>("test_endpoint_ping", {
        overrideUrl: workingConfig.value.url,
        configOverride: cloneData(workingConfig.value),
      });
      pingStatus.value = res;
      pushToast(res.message, res.reachable ? "success" : "error");
    } catch (err) {
      pushToast(`连通性探测异常：${String(err)}`, "error");
    } finally {
      isPinging.value = false;
    }
  }

  /** 在线编辑后单条即时重发（后端自动持久化历史报告并刷新列表） */
  async function replaySingleActive(): Promise<void> {
    if (!activeEntry.value) return;
    isReplayingSingle.value = true;
    try {
      const res = await invoke<RunItemResult>("replay_single_item", {
        rawJson: editedEntryRaw.value,
        entry: activeEntry.value,
        overrideUrl: workingConfig.value.url,
        configOverride: cloneData(workingConfig.value),
      });
      resultsByIndex.set(res.index, res);
      void fetchReportsList();
      pushToast(`#${res.index} 重发完成：${res.badge} (${res.ms}ms)`, res.ok ? "success" : "warning");
    } catch (err) {
      pushToast(`单条重发失败：${String(err)}`, "error");
    } finally {
      isReplayingSingle.value = false;
    }
  }

  /** 发起批量回放（支持自定义条目集、如仅选某段或全部条目） */
  async function startBatchRun(customEntries?: IndexedEntry[]): Promise<void> {
    const targets =
      customEntries ??
      scanResult.value.entries.filter((e) => selectedIndices.value.has(e.index));

    if (targets.length === 0) {
      pushToast("当前没有选中任何待发消息", "warning");
      return;
    }

    // 重置当次需要发送条目的结果占位
    for (const item of targets) {
      resultsByIndex.delete(item.index);
    }
    if (!customEntries) {
      resultsByIndex.clear();
    }

    doneCount.value = 0;
    totalSelected.value = targets.length;
    currentSegment.value = targets[0].segment;
    segmentWaiting.value = null;
    clearWaitCountdown();

    try {
      const runId = await invoke<string>("start_run", {
        entries: targets,
        configOverride: cloneData(workingConfig.value),
        parseErrorsCount: scanResult.value.parseErrors.length,
        commentsSkipped: scanResult.value.commentsSkipped,
        files: scanResult.value.files,
      });
      activeRunId.value = runId;
      runState.value = "running";
    } catch (err) {
      runState.value = "idle";
      pushToast(`启动回放失败：${String(err)}`, "error");
    }
  }

  /** 发送单个分段 */
  async function runSingleSegment(segmentId: number): Promise<void> {
    const segEntries = scanResult.value.entries.filter(
      (e) => e.segment === segmentId
    );
    await startBatchRun(segEntries);
  }

  /** 控制运行中的批量任务（暂停/恢复/停止/交互确认） */
  async function controlRun(
    action: "pause" | "resume" | "abort" | "confirm",
    segmentAction: SegmentAction | null = null
  ): Promise<void> {
    try {
      await invoke("control_active_run", { action, segmentAction });
      if (action === "pause") {
        runState.value = "paused";
      } else if (action === "resume") {
        runState.value = segmentWaiting.value ? "waiting_confirm" : "running";
      } else if (action === "confirm") {
        segmentWaiting.value = null;
        runState.value = "running";
      }
    } catch (err) {
      pushToast(`任务控制失败：${String(err)}`, "error");
    }
  }

  /** 刷新历史运行报告列表 */
  async function fetchReportsList(overrideCfg?: AppConfig | null): Promise<void> {
    try {
      const list = await invoke<ReportSummary[]>("get_reports_list", {
        configOverride: overrideCfg ?? cloneData(workingConfig.value),
      });
      reportsList.value = list;
    } catch {
      // 忽略报告目录不存在等非关键错误
    }
  }

  /** 读取单份历史报告详情 */
  async function openReportDetail(path: string): Promise<void> {
    isLoadingReport.value = true;
    try {
      const report = await invoke<FullRunReport>("read_report_detail", { path });
      inspectedReport.value = report;
    } catch (err) {
      pushToast(`读取报告详情失败：${String(err)}`, "error");
    } finally {
      isLoadingReport.value = false;
    }
  }

  /** 删除单份历史报告并刷新列表 */
  async function deleteReport(path: string, finishedAt?: string): Promise<void> {
    try {
      const updated = await invoke<ReportSummary[]>("delete_report", {
        path,
        configOverride: cloneData(workingConfig.value),
      });
      reportsList.value = updated;
      if (
        inspectedReport.value &&
        finishedAt &&
        inspectedReport.value.finishedAt === finishedAt
      ) {
        inspectedReport.value = null;
      }
      pushToast("已删除该条历史测试报告", "info");
    } catch (err) {
      pushToast(`删除报告失败：${String(err)}`, "error");
    }
  }

  /** 从当前或历史报告中提取异常条目并仅勾选它们用于复测 */
  function selectAbnormalFromReport(report?: FullRunReport | null): void {
    const abnormalIndices = new Set<number>();
    if (report) {
      for (const r of report.results) {
        if (r.outcome === "failed" || r.outcome === "unmatched") {
          abnormalIndices.add(r.index);
        }
      }
    } else {
      for (const [idx, r] of resultsByIndex.entries()) {
        if (r.outcome === "failed" || r.outcome === "unmatched") {
          abnormalIndices.add(idx);
        }
      }
    }

    if (abnormalIndices.size === 0) {
      pushToast("当前记录中没有失败或未匹配条目", "info");
      return;
    }

    const next = new Set<number>();
    for (const entry of scanResult.value.entries) {
      if (abnormalIndices.has(entry.index)) {
        next.add(entry.index);
      }
    }
    selectedIndices.value = next;
    inspectedReport.value = null;
    leftTab.value = "sources";
    pushToast(`已勾选 ${next.size} 条异常条目，可直接点击「发送选中」重跑`, "success");
  }

  /** 勾选 / 取消单条 */
  function toggleEntrySelection(index: number): void {
    const next = new Set(selectedIndices.value);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    selectedIndices.value = next;
  }

  /** 全选或取消全选某个分段 */
  function toggleSegmentSelection(segmentId: number, checked: boolean): void {
    const next = new Set(selectedIndices.value);
    for (const entry of scanResult.value.entries) {
      if (entry.segment === segmentId) {
        if (checked) next.add(entry.index);
        else next.delete(entry.index);
      }
    }
    selectedIndices.value = next;
  }

  /** 对当前筛选结果全选、清空或反选 */
  function batchSelectFiltered(mode: "all" | "none" | "invert"): void {
    const next = new Set(selectedIndices.value);
    for (const entry of filteredEntries.value) {
      if (mode === "all") next.add(entry.index);
      else if (mode === "none") next.delete(entry.index);
      else {
        if (next.has(entry.index)) next.delete(entry.index);
        else next.add(entry.index);
      }
    }
    selectedIndices.value = next;
  }

  /** 初始化应用：注册 Tauri Event 监听器、全局快捷键并拉取初始状态 */
  async function initApp(): Promise<void> {
    if (initialized) return;
    initialized = true;

    // 1. 首屏欢迎态展示 1.2 秒 Gemini 流光后自动收敛，保证闲置零功耗
    window.setTimeout(() => {
      isStartupShimmer.value = false;
    }, 1200);

    // 2. 注册实时事件监听器
    unlistenCallbacks.push(
      await listen<RunStatusPayload>("run://status", (ev) => {
        const st = ev.payload.state;
        if (st === "running") runState.value = "running";
        else if (st === "aborted") {
          runState.value = "aborted";
          segmentWaiting.value = null;
          clearWaitCountdown();
        } else if (st === "finished") {
          runState.value = "finished";
          segmentWaiting.value = null;
          clearWaitCountdown();
        }
      })
    );

    unlistenCallbacks.push(
      await listen<SegmentWaitingPayload>("run://segment-waiting", (ev) => {
        segmentWaiting.value = ev.payload;
        currentSegment.value = ev.payload.segmentId;
        runState.value = "waiting_confirm";
        clearWaitCountdown();
      })
    );

    unlistenCallbacks.push(
      await listen<ItemProgressPayload>("run://item-done", (ev) => {
        doneCount.value = ev.payload.doneCount;
        totalSelected.value = ev.payload.totalSelected;
        currentSegment.value = ev.payload.currentSegment;
        resultsByIndex.set(ev.payload.result.index, ev.payload.result);
      })
    );

    unlistenCallbacks.push(
      await listen<SegmentWaitCountdownPayload>("run://segment-wait-countdown", (ev) => {
        clearWaitCountdown();
        waitCountdownMs.value = ev.payload.waitMs;
        countdownTimer = window.setInterval(() => {
          waitCountdownMs.value = Math.max(0, waitCountdownMs.value - 200);
          if (waitCountdownMs.value <= 0) {
            clearWaitCountdown();
          }
        }, 200);
      })
    );

    unlistenCallbacks.push(
      await listen<FullRunReport>("run://finished", (ev) => {
        lastReport.value = ev.payload;
        segmentWaiting.value = null;
        clearWaitCountdown();
        const r = ev.payload;
        const summary = `完成 ${r.total} 条：成功 ${r.ok} / 未匹配 ${r.unmatched} / 失败 ${r.failed} (${r.durationMs}ms)`;
        pushToast(summary, r.failed > 0 ? "warning" : "success", 5000);
        void fetchReportsList();
      })
    );

    // 3. 全局字号缩放快捷键（Ctrl/Cmd + / - / 0）与分段确认快捷键（Y/Enter/N/A/Q）
    window.addEventListener("keydown", (ev: KeyboardEvent) => {
      if ((ev.ctrlKey || ev.metaKey) && !ev.altKey) {
        if (ev.key === "=" || ev.key === "+") {
          ev.preventDefault();
          void stepFontSizes(1).then(() => {
            pushToast(
              `字号已放大：界面 ${fontSize.value}px / 代码 ${codeFontSize.value}px`,
              "info",
              1600
            );
          });
          return;
        }
        if (ev.key === "-" || ev.key === "_") {
          ev.preventDefault();
          void stepFontSizes(-1).then(() => {
            pushToast(
              `字号已缩小：界面 ${fontSize.value}px / 代码 ${codeFontSize.value}px`,
              "info",
              1600
            );
          });
          return;
        }
        if (ev.key === "0") {
          ev.preventDefault();
          void resetFontSizesOnly().then(() => {
            pushToast("字号已重置：界面 14px / 代码 13px", "info", 1600);
          });
          return;
        }
        if (ev.key === "\\" || ev.code === "Backslash") {
          ev.preventDefault();
          if (ev.shiftKey) {
            if (leftTab.value === "sources") {
              const floating = toggleFloating("right");
              pushToast(
                floating ? "右侧检视栏已切为悬浮模式" : "右侧检视栏已固定",
                "info",
                1600
              );
            } else if (leftTab.value === "reports") {
              const floating = toggleFloating("reports");
              pushToast(
                floating ? "历史报告清单已切为悬浮模式" : "历史报告清单已固定",
                "info",
                1600
              );
            }
          } else {
            const floating = toggleFloating("left");
            pushToast(
              floating ? "左侧栏已切为悬浮模式（移到左边缘展开）" : "左侧栏已固定",
              "info",
              1600
            );
          }
          return;
        }
      }

      if (
        ev.key === "Escape" &&
        !segmentWaiting.value &&
        !showAppearanceModal.value &&
        !showSetupWizard.value &&
        !showScratchpadModal.value
      ) {
        closeAllFloatingDrawers();
      }

      if (!segmentWaiting.value) return;
      const target = ev.target as HTMLElement | null;
      const tag = target?.tagName?.toLowerCase();
      if (tag === "input" || tag === "textarea" || tag === "select") {
        return;
      }
      const key = ev.key.toLowerCase();
      if (key === "y" || ev.key === "Enter") {
        ev.preventDefault();
        void controlRun("confirm", "y");
      } else if (key === "n") {
        ev.preventDefault();
        void controlRun("confirm", "n");
      } else if (key === "a") {
        ev.preventDefault();
        void controlRun("confirm", "a");
      } else if (key === "q" || ev.key === "Escape") {
        ev.preventDefault();
        void controlRun("confirm", "q");
      }
    });

    // 4. 加载初始配置并扫描数据源、历史报告与内置模板
    await loadConfigSnapshot();
    void loadConfigTemplates();
    if (snapshot.value && !snapshot.value.onboardingCompleted) {
      showSetupWizard.value = true;
    }
    await scanSources(workingConfig.value);
    await fetchReportsList(workingConfig.value);
  }

  return {
    leftTab,
    isStartupShimmer,
    showScratchpadModal,
    showSetupWizard,
    showAppearanceModal,
    scratchpadDraft,
    isScratchpadMode,
    snapshot,
    workingConfig,
    rawYamlDraft,
    configTemplates,
    scanResult,
    isScanning,
    selectedIndices,
    searchQuery,
    selectedEventType,
    onlyAbnormal,
    filteredEntries,
    activeEntry,
    activeEntryRaw,
    editedEntryRaw,
    isLoadingEntryRaw,
    isReplayingSingle,
    runState,
    activeRunId,
    doneCount,
    totalSelected,
    currentSegment,
    resultsByIndex,
    runStats,
    lastReport,
    segmentWaiting,
    waitCountdownMs,
    pingStatus,
    isPinging,
    reportsList,
    inspectedReport,
    isLoadingReport,
    toasts,
    isBusy,
    n8nUrlMode,
    pushToast,
    applySnapshot,
    initApp,
    loadConfigSnapshot,
    scanSources,
    loadScratchpadText,
    selectActiveEntry,
    selectYamlProfile,
    mountYamlProfiles,
    unmountYamlProfile,
    renameYamlProfile,
    addProfileDir,
    removeProfileDir,
    loadConfigTemplates,
    createConfigFromTemplate,
    completeOnboarding,
    inspectLogCategories,
    listDirLogFiles,
    persistScratchpadMessages,
    saveAsGuiConfig,
    saveToYamlFile,
    saveRawYamlSource,
    toggleN8nTargetUrl,
    testEndpointPing,
    replaySingleActive,
    startBatchRun,
    runSingleSegment,
    controlRun,
    fetchReportsList,
    openReportDetail,
    deleteReport,
    selectAbnormalFromReport,
    toggleEntrySelection,
    toggleSegmentSelection,
    batchSelectFiltered,
  };
}
