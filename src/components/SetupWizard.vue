<script setup lang="ts">
/**
 * 首次安装/使用配置引导向导（SetupWizard）
 *
 * 4 步向导引导用户完成关键初始化设置：
 * 1. 配置文件（挂载现有 YAML/JSONC、从内置模板生成、或纯 GUI 模式）；
 * 2. 发送地址与认证（URL、Method、认证方式、自签名证书及当场 Ping 探测）；
 * 3. 原始测试消息文件/文件夹（添加多文件/目录、格式、交互确认与当场试扫描）；
 * 4. 外观字体设置与汇总保存。
 */
import { computed, ref, watch } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  FileCode2,
  FilePlus2,
  FolderOpen,
  FolderPlus,
  Monitor,
  Moon,
  Palette,
  RefreshCw,
  Rocket,
  Sparkles,
  Sun,
  Trash2,
  Wand2,
  X,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useTheme } from "../composables/useTheme";
import type { SourceScanResult, ThemeMode } from "../types";
import { cloneData } from "../composables/useAppState";

const {
  showSetupWizard,
  snapshot,
  workingConfig,
  configTemplates,
  pingStatus,
  isPinging,
  n8nUrlMode,
  selectYamlProfile,
  mountYamlProfiles,
  loadConfigTemplates,
  createConfigFromTemplate,
  toggleN8nTargetUrl,
  testEndpointPing,
  saveAsGuiConfig,
  saveToYamlFile,
  scanSources,
  completeOnboarding,
  pushToast,
} = useAppState();

const {
  themeMode,
  uiFont,
  monoFont,
  fontSize,
  codeFontSize,
  auroraIntensity,
  systemFonts,
  setTheme,
  loadSystemFonts,
  saveAppearanceSettings,
} = useTheme();

const currentStep = ref<1 | 2 | 3 | 4>(1);
const manualYamlPath = ref("");
const manualInputPath = ref("");
const selectedTemplateId = ref("n8n-webhook");
const previewScan = ref<SourceScanResult | null>(null);
const isTestingScan = ref(false);
const saveTargetMode = ref<"gui" | "yaml" | "new-yaml">("gui");
const isFinishing = ref(false);

const STEPS = [
  { num: 1 as const, title: "1. 配置文件" },
  { num: 2 as const, title: "2. 发送地址与认证" },
  { num: 3 as const, title: "3. 原始测试消息" },
  { num: 4 as const, title: "4. 字体外观与完成" },
];

watch(showSetupWizard, (opened) => {
  if (opened) {
    currentStep.value = 1;
    previewScan.value = null;
    saveTargetMode.value = snapshot.value?.yamlPath ? "yaml" : "gui";
    void loadConfigTemplates();
    void loadSystemFonts(false);
  }
});

const monospaceFonts = computed(() =>
  systemFonts.value.filter((f) => f.isMonospace)
);

/** 选择并挂载多个配置文件 */
async function browseAndMountProfiles(): Promise<void> {
  const picked = await open({
    title: "选择一个或多个 YAML / JSONC 配置文件",
    multiple: true,
    filters: [
      { name: "YAML / JSONC 配置", extensions: ["yaml", "yml", "json", "jsonc"] },
    ],
  });
  if (!picked) return;
  const list = Array.isArray(picked) ? picked : [picked];
  if (list.length > 0) {
    await mountYamlProfiles(list, true);
    saveTargetMode.value = "yaml";
  }
}

/** 通过手动路径挂载配置文件 */
async function mountManualProfile(): Promise<void> {
  const p = manualYamlPath.value.trim();
  if (!p) return;
  const ok = await mountYamlProfiles([p], true);
  if (ok) {
    manualYamlPath.value = "";
    saveTargetMode.value = "yaml";
  }
}

/** 从内置模板生成配置文件并立即激活 */
async function generateFromTemplate(): Promise<void> {
  const tpl = configTemplates.value.find((t) => t.id === selectedTemplateId.value);
  const picked = await save({
    title: "选择新模板配置文件的保存路径",
    defaultPath: tpl?.suggestedFileName ?? "hook-replay.config.yaml",
    filters: [{ name: "YAML 配置", extensions: ["yaml", "yml"] }],
  });
  if (!picked) return;
  const ok = await createConfigFromTemplate(selectedTemplateId.value, picked, true);
  if (ok) {
    saveTargetMode.value = "yaml";
  }
}

/** 添加原始消息文件 */
async function browseAddLogFiles(): Promise<void> {
  const picked = await open({
    title: "选择原始测试消息文件（可多选）",
    multiple: true,
    filters: [
      { name: "日志 / JSON / 文本", extensions: ["txt", "log", "json", "jsonl"] },
      { name: "所有文件", extensions: ["*"] },
    ],
  });
  if (!picked) return;
  const list = Array.isArray(picked) ? picked : [picked];
  for (const p of list) {
    if (p && !workingConfig.value.inputs.includes(p)) {
      workingConfig.value.inputs.push(p);
    }
  }
}

/** 添加原始消息目录 */
async function browseAddLogDir(): Promise<void> {
  const picked = await open({
    title: "选择包含测试消息文件的文件夹",
    directory: true,
    multiple: false,
  });
  if (typeof picked === "string" && picked.trim()) {
    if (!workingConfig.value.inputs.includes(picked)) {
      workingConfig.value.inputs.push(picked);
    }
  }
}

/** 手动输入原始测试消息文件或目录 */
function addManualInputPath(): void {
  const trimmed = manualInputPath.value.trim();
  if (!trimmed) return;
  if (!workingConfig.value.inputs.includes(trimmed)) {
    workingConfig.value.inputs.push(trimmed);
  }
  manualInputPath.value = "";
}

/** 试扫描当前配置的输入源 */
async function runPreviewScan(): Promise<void> {
  isTestingScan.value = true;
  try {
    previewScan.value = await invoke<SourceScanResult>("scan_input_sources", {
      configOverride: cloneData(workingConfig.value),
    });
  } catch (err) {
    pushToast(`试扫描失败：${String(err)}`, "error");
  } finally {
    isTestingScan.value = false;
  }
}

/** 跳过引导直接进入主界面 */
async function handleSkipWizard(): Promise<void> {
  await completeOnboarding(true);
}

/** 完成引导并保存全部关键设置 */
async function handleFinishWizard(): Promise<void> {
  isFinishing.value = true;
  try {
    await saveAppearanceSettings({
      uiFont: uiFont.value,
      monoFont: monoFont.value,
      fontSize: fontSize.value,
      codeFontSize: codeFontSize.value,
      auroraIntensity: auroraIntensity.value,
    });

    if (saveTargetMode.value === "yaml" && snapshot.value?.yamlPath) {
      await saveToYamlFile(snapshot.value.yamlPath, workingConfig.value);
    } else if (saveTargetMode.value === "new-yaml") {
      const picked = await save({
        title: "保存配置向导结果为 YAML 文件",
        defaultPath: "hook-replay.config.yaml",
        filters: [{ name: "YAML 配置", extensions: ["yaml", "yml"] }],
      });
      if (picked) {
        await saveToYamlFile(picked, workingConfig.value);
      } else {
        await saveAsGuiConfig(workingConfig.value);
      }
    } else {
      await saveAsGuiConfig(workingConfig.value);
    }

    await completeOnboarding(true);
    await scanSources(workingConfig.value);
    pushToast("初始化配置完成，欢迎使用 HookReplay！", "success");
  } finally {
    isFinishing.value = false;
  }
}
</script>

<template>
  <div
    v-if="showSetupWizard"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
  >
    <div
      class="aurora-border-active flex max-h-[92vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl bg-[var(--bg-surface)] shadow-2xl"
    >
      <!-- 顶部标题与步骤导航条 -->
      <div class="border-b border-[var(--border-subtle)] px-5 py-4 space-y-3">
        <div class="flex items-center justify-between gap-2">
          <div class="flex items-center gap-2.5">
            <div
              class="aurora-ring-soft flex h-9 w-9 items-center justify-center rounded-xl bg-[var(--bg-elevated)] text-indigo-500"
            >
              <Wand2 class="h-4.5 w-4.5" />
            </div>
            <div>
              <h2 class="text-sm font-bold text-[var(--text-primary)]">
                欢迎使用 HookReplay · 初始化配置向导
              </h2>
              <p class="text-[0.6875rem] text-[var(--text-muted)]">
                只需几步即可完成配置文件挂载、发送目标、测试日志源与外观设置（之后随时可在顶部或配置中心修改）
              </p>
            </div>
          </div>

          <button
            type="button"
            @click="handleSkipWizard"
            class="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-xs text-[var(--text-muted)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
          >
            <span>跳过向导</span>
            <X class="h-3.5 w-3.5" />
          </button>
        </div>

        <!-- 4 步进度胶囊 -->
        <div class="grid grid-cols-4 gap-2">
          <button
            v-for="s in STEPS"
            :key="s.num"
            type="button"
            @click="currentStep = s.num"
            class="rounded-lg border px-2.5 py-1.5 text-left text-[0.6875rem] font-semibold transition"
            :class="
              currentStep === s.num
                ? 'aurora-tab-active border-indigo-500/60 bg-indigo-500/12 text-indigo-700 dark:text-indigo-300'
                : currentStep > s.num
                  ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 text-[var(--text-muted)]'
            "
          >
            {{ s.title }}
          </button>
        </div>
      </div>

      <!-- 主体步骤内容 -->
      <div class="min-h-0 flex-1 overflow-y-auto p-5 space-y-4">
        <!-- 第 1 步：配置文件 -->
        <div v-if="currentStep === 1" class="space-y-4">
          <div class="rounded-xl border border-indigo-500/25 bg-indigo-500/8 p-3.5 text-xs text-[var(--text-secondary)] space-y-1">
            <div class="font-bold text-indigo-700 dark:text-indigo-300">
              选择或挂载配置文件（支持多个切换，也可不挂载纯 GUI 使用）
            </div>
            <p class="text-[0.6875rem]">
              绑定 YAML / JSONC 配置文件后，配置中的非空字段会自动优先于 GUI 兜底配置；也可以直接从下方内置模板新建一份。
            </p>
          </div>

          <!-- 快速操作按钮组 -->
          <div class="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
            <button
              type="button"
              @click="browseAndMountProfiles"
              class="aurora-btn-subtle flex items-center gap-2.5 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-3 text-left"
            >
              <FolderOpen class="h-5 w-5 shrink-0 text-blue-500" />
              <div>
                <div class="text-xs font-semibold text-[var(--text-primary)]">
                  挂载已有 YAML / JSONC（支持多选）
                </div>
                <div class="text-[0.6875rem] text-[var(--text-muted)]">
                  同时挂载多套环境配置并在顶栏一键切换
                </div>
              </div>
            </button>

            <button
              type="button"
              @click="selectYamlProfile(null); saveTargetMode = 'gui'"
              class="aurora-btn-subtle flex items-center gap-2.5 rounded-xl border p-3 text-left"
              :class="
                !snapshot?.yamlPath
                  ? 'border-cyan-500 bg-cyan-500/10'
                  : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]'
              "
            >
              <Sparkles class="h-5 w-5 shrink-0 text-cyan-500" />
              <div>
                <div class="text-xs font-semibold text-[var(--text-primary)]">
                  暂不挂载，仅使用纯 GUI 模式
                </div>
                <div class="text-[0.6875rem] text-[var(--text-muted)]">
                  所有配置自动保存至用户目录 gui-config.json
                </div>
              </div>
            </button>
          </div>

          <!-- 手动输入配置文件路径 -->
          <div class="flex items-center gap-2">
            <input
              v-model="manualYamlPath"
              type="text"
              placeholder="或者直接输入配置文件路径，例如 ./hook-replay.config.yaml"
              class="h-8 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 font-mono text-xs outline-none focus:border-blue-500"
            />
            <button
              type="button"
              :disabled="!manualYamlPath.trim()"
              @click="mountManualProfile"
              class="aurora-btn-subtle h-8 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 text-xs font-medium disabled:opacity-45"
            >
              挂载路径
            </button>
          </div>

          <!-- 从内置模板生成 -->
          <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50 p-3.5 space-y-2.5">
            <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-primary)]">
              <FileCode2 class="h-4 w-4 text-purple-500" />
              <span>或从内置模板新建配置文件</span>
            </div>
            <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
              <button
                v-for="tpl in configTemplates"
                :key="tpl.id"
                type="button"
                @click="selectedTemplateId = tpl.id"
                class="rounded-lg border p-2.5 text-left transition"
                :class="
                  selectedTemplateId === tpl.id
                    ? 'border-purple-500 bg-purple-500/12'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:bg-[var(--bg-elevated)]'
                "
              >
                <div class="text-xs font-semibold text-[var(--text-primary)]">
                  {{ tpl.name }}
                </div>
                <div class="mt-0.5 text-[0.625rem] leading-snug text-[var(--text-muted)]">
                  {{ tpl.description }}
                </div>
              </button>
            </div>
            <div class="flex justify-end">
              <button
                type="button"
                @click="generateFromTemplate"
                class="aurora-btn-subtle inline-flex items-center gap-1.5 rounded-lg border border-purple-500/40 bg-purple-500/12 px-3 py-1.5 text-xs font-semibold text-purple-700 dark:text-purple-300"
              >
                <FilePlus2 class="h-3.5 w-3.5" />
                <span>生成模板文件并挂载…</span>
              </button>
            </div>
          </div>

          <!-- 已发现/已挂载档案清单 -->
          <div v-if="(snapshot?.profiles.length ?? 0) > 0" class="space-y-1.5">
            <div class="text-[0.6875rem] font-semibold text-[var(--text-muted)]">
              当前已发现 / 已挂载配置档案（点击即可设为当前生效）：
            </div>
            <div class="max-h-36 overflow-y-auto space-y-1">
              <button
                v-for="prof in snapshot?.profiles ?? []"
                :key="prof.path"
                type="button"
                @click="selectYamlProfile(prof.path); saveTargetMode = 'yaml'"
                class="flex w-full items-center justify-between gap-2 rounded-lg border px-3 py-1.5 text-left text-xs transition"
                :class="
                  snapshot?.yamlPath === prof.path
                    ? 'border-blue-500 bg-blue-500/12'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 hover:bg-[var(--bg-elevated)]'
                "
              >
                <div class="min-w-0 flex-1">
                  <div class="font-medium text-[var(--text-primary)] truncate">
                    {{ prof.name }}
                  </div>
                  <div class="font-mono text-[0.625rem] text-[var(--text-muted)] truncate">
                    {{ prof.path }}
                  </div>
                </div>
                <span
                  v-if="snapshot?.yamlPath === prof.path"
                  class="shrink-0 rounded bg-blue-600 px-1.5 py-0.5 text-[0.625rem] font-semibold text-white"
                >
                  当前生效
                </span>
              </button>
            </div>
          </div>
        </div>

        <!-- 第 2 步：发送地址与认证 -->
        <div v-else-if="currentStep === 2" class="space-y-4">
          <div class="grid grid-cols-1 gap-3 md:grid-cols-4">
            <label class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">HTTP 方法</span>
              <select
                v-model="workingConfig.method"
                class="h-9 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs font-bold text-blue-600 dark:text-blue-400 outline-none"
              >
                <option value="POST">POST</option>
                <option value="PUT">PUT</option>
                <option value="PATCH">PATCH</option>
                <option value="GET">GET</option>
              </select>
            </label>

            <label class="space-y-1 text-xs md:col-span-3">
              <div class="flex items-center justify-between">
                <span class="font-medium text-[var(--text-secondary)]">
                  目标发送地址（Webhook / REST URL）
                </span>
                <button
                  v-if="n8nUrlMode !== null"
                  type="button"
                  @click="toggleN8nTargetUrl"
                  class="inline-flex items-center gap-1 rounded-md bg-indigo-500/15 px-2 py-0.5 text-[0.625rem] font-semibold text-indigo-600 dark:text-indigo-300"
                >
                  <RefreshCw class="h-3 w-3" />
                  <span>
                    {{
                      n8nUrlMode === "test"
                        ? "当前为测试入口 /webhook-test/ · 点击切回正式"
                        : "当前为正式入口 /webhook/ · 点击切至测试"
                    }}
                  </span>
                </button>
              </div>
              <input
                v-model="workingConfig.url"
                type="text"
                placeholder="例如：https://n8n.example.com/webhook/demo"
                class="h-9 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 font-mono text-xs outline-none focus:border-blue-500"
              />
            </label>
          </div>

          <!-- 认证设置 -->
          <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/45 p-3.5 space-y-3">
            <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
              <label class="space-y-1 text-xs">
                <span class="font-medium text-[var(--text-secondary)]">认证方式</span>
                <select
                  v-model="workingConfig.auth.type"
                  class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs outline-none"
                >
                  <option value="none">none（无认证）</option>
                  <option value="basic">basic（用户名/密码）</option>
                  <option value="bearer">bearer（Bearer Token）</option>
                  <option value="header">header（自定义请求头）</option>
                  <option value="query">query（URL 查询参数）</option>
                </select>
              </label>

              <template v-if="workingConfig.auth.type === 'basic'">
                <label class="space-y-1 text-xs">
                  <span class="font-medium text-[var(--text-secondary)]">用户名</span>
                  <input
                    v-model="workingConfig.auth.username"
                    type="text"
                    class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 text-xs outline-none"
                  />
                </label>
                <label class="space-y-1 text-xs">
                  <span class="font-medium text-[var(--text-secondary)]">密码</span>
                  <input
                    v-model="workingConfig.auth.password"
                    type="password"
                    class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 text-xs outline-none"
                  />
                </label>
              </template>

              <template v-else-if="workingConfig.auth.type !== 'none'">
                <label v-if="workingConfig.auth.type === 'header'" class="space-y-1 text-xs">
                  <span class="font-medium text-[var(--text-secondary)]">Header 名称</span>
                  <input
                    v-model="workingConfig.auth.headerName"
                    type="text"
                    class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 font-mono text-xs outline-none"
                  />
                </label>
                <label v-if="workingConfig.auth.type === 'query'" class="space-y-1 text-xs">
                  <span class="font-medium text-[var(--text-secondary)]">Query 参数名</span>
                  <input
                    v-model="workingConfig.auth.queryName"
                    type="text"
                    class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 font-mono text-xs outline-none"
                  />
                </label>
                <label class="space-y-1 text-xs">
                  <span class="font-medium text-[var(--text-secondary)]">令牌 (Token)</span>
                  <input
                    v-model="workingConfig.auth.token"
                    type="password"
                    class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 font-mono text-xs outline-none"
                  />
                </label>
              </template>
            </div>

            <div class="flex flex-wrap items-center justify-between gap-2 pt-1">
              <label class="flex cursor-pointer items-center gap-2 text-xs text-[var(--text-primary)]">
                <input
                  v-model="workingConfig.insecure"
                  type="checkbox"
                  class="h-4 w-4 rounded accent-blue-600"
                />
                <span>跳过 HTTPS 自签名证书校验 (insecure)</span>
              </label>

              <button
                type="button"
                :disabled="isPinging || !workingConfig.url.trim()"
                @click="testEndpointPing"
                class="aurora-btn-subtle inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-3 py-1.5 text-xs font-medium disabled:opacity-45"
              >
                <Activity class="h-3.5 w-3.5" :class="{ 'animate-pulse text-blue-500': isPinging }" />
                <span>立即测试连通性 (Ping)</span>
              </button>
            </div>

            <div
              v-if="pingStatus"
              class="rounded-lg px-3 py-2 text-xs font-medium"
              :class="
                pingStatus.reachable
                  ? 'bg-emerald-500/12 text-emerald-700 dark:text-emerald-300'
                  : 'bg-rose-500/12 text-rose-700 dark:text-rose-300'
              "
            >
              {{ pingStatus.message }}
            </div>
          </div>
        </div>

        <!-- 第 3 步：原始测试消息文件/文件夹 -->
        <div v-else-if="currentStep === 3" class="space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div>
              <div class="text-xs font-bold text-[var(--text-primary)]">
                原始测试消息文件或文件夹 (inputs)
              </div>
              <div class="text-[0.6875rem] text-[var(--text-muted)]">
                支持单个/多个日志文本、JSON/JSONL 文件，或整个测试消息文件夹
              </div>
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                @click="browseAddLogFiles"
                class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1.5 text-xs font-medium"
              >
                <FilePlus2 class="h-3.5 w-3.5 text-blue-500" />
                <span>+ 添加文件</span>
              </button>
              <button
                type="button"
                @click="browseAddLogDir"
                class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1.5 text-xs font-medium"
              >
                <FolderPlus class="h-3.5 w-3.5 text-amber-500" />
                <span>+ 添加文件夹</span>
              </button>
            </div>
          </div>

          <!-- 手动输入路径 -->
          <div class="flex items-center gap-2">
            <input
              v-model="manualInputPath"
              type="text"
              placeholder="可手动输入相对或绝对路径，如 ./test_raw_msgs/test_set_01.txt"
              @keydown.enter.prevent="addManualInputPath"
              class="h-8 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 font-mono text-xs outline-none focus:border-blue-500"
            />
            <button
              type="button"
              :disabled="!manualInputPath.trim()"
              @click="addManualInputPath"
              class="h-8 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 text-xs font-medium disabled:opacity-45"
            >
              添加
            </button>
          </div>

          <!-- 已添加数据源列表 -->
          <div class="max-h-36 overflow-y-auto space-y-1.5">
            <div
              v-for="(inp, idx) in workingConfig.inputs"
              :key="inp + idx"
              class="flex items-center justify-between gap-2 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 px-3 py-1.5 text-xs"
            >
              <span class="truncate font-mono text-[var(--text-primary)]">{{ inp }}</span>
              <button
                type="button"
                @click="workingConfig.inputs.splice(idx, 1)"
                class="shrink-0 text-[var(--text-muted)] hover:text-rose-500"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </button>
            </div>
            <div
              v-if="workingConfig.inputs.length === 0"
              class="rounded-xl border border-dashed border-[var(--border-strong)] py-6 text-center text-xs text-[var(--text-muted)]"
            >
              暂未添加任何输入文件或文件夹（也可以完成向导后随时添加，或使用「临时粘贴调试台」）
            </div>
          </div>

          <!-- 扫描选项与试扫描 -->
          <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/45 p-3 text-xs">
            <div class="flex flex-wrap items-center gap-4">
              <label class="flex cursor-pointer items-center gap-1.5">
                <input
                  v-model="workingConfig.recursive"
                  type="checkbox"
                  class="h-3.5 w-3.5 rounded accent-blue-600"
                />
                <span>递归扫描子目录</span>
              </label>

              <label class="flex cursor-pointer items-center gap-1.5">
                <input
                  v-model="workingConfig.interactive"
                  type="checkbox"
                  class="h-3.5 w-3.5 rounded accent-blue-600"
                />
                <span>逐段确认后再发送 (interactive)</span>
              </label>

              <label class="flex items-center gap-1.5">
                <span class="text-[var(--text-muted)]">解析格式:</span>
                <select
                  v-model="workingConfig.format"
                  class="h-7 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs"
                >
                  <option value="auto">auto</option>
                  <option value="jsonl">jsonl</option>
                  <option value="json">json</option>
                </select>
              </label>
            </div>

            <button
              type="button"
              :disabled="isTestingScan || workingConfig.inputs.length === 0"
              @click="runPreviewScan"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-blue-500/40 bg-blue-500/12 px-3 py-1 text-xs font-semibold text-blue-700 dark:text-blue-300 disabled:opacity-45"
            >
              <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': isTestingScan }" />
              <span>试扫描测试消息</span>
            </button>
          </div>

          <div
            v-if="previewScan"
            class="rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-3.5 py-2.5 text-xs text-emerald-700 dark:text-emerald-300"
          >
            试扫描成功：共识别 <strong>{{ previewScan.files.length }}</strong> 个文件、
            <strong>{{ previewScan.segments.length }}</strong> 个分段、
            <strong>{{ previewScan.entries.length }}</strong> 条有效测试消息
            <span v-if="previewScan.missingPaths.length > 0" class="text-amber-600 dark:text-amber-300">
              （注意：有 {{ previewScan.missingPaths.length }} 个路径未找到）
            </span>
          </div>
        </div>

        <!-- 第 4 步：外观字体与完成保存 -->
        <div v-else class="space-y-4">
          <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
            <div class="space-y-1.5">
              <span class="text-xs font-semibold text-[var(--text-secondary)]">外观主题</span>
              <div class="grid grid-cols-3 gap-1.5">
                <button
                  v-for="opt in [
                    { id: 'system', label: '系统', icon: Monitor },
                    { id: 'light', label: '浅色', icon: Sun },
                    { id: 'dark', label: '深色', icon: Moon },
                  ]"
                  :key="opt.id"
                  type="button"
                  @click="setTheme(opt.id as ThemeMode)"
                  class="flex items-center justify-center gap-1 rounded-lg border py-1.5 text-xs font-medium"
                  :class="
                    themeMode === opt.id
                      ? 'border-indigo-500 bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
                      : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]'
                  "
                >
                  <component :is="opt.icon" class="h-3.5 w-3.5" />
                  <span>{{ opt.label }}</span>
                </button>
              </div>
            </div>

            <label class="space-y-1.5 text-xs">
              <span class="font-semibold text-[var(--text-secondary)]">界面正文字体（本机）</span>
              <select
                v-model="uiFont"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs outline-none"
              >
                <option value="">系统默认字体</option>
                <option v-for="f in systemFonts" :key="'w-ui-' + f.name" :value="f.name">
                  {{ f.name }}
                </option>
              </select>
            </label>

            <label class="space-y-1.5 text-xs">
              <span class="font-semibold text-[var(--text-secondary)]">代码 / JSON 等宽字体</span>
              <select
                v-model="monoFont"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs outline-none"
              >
                <option value="">默认等宽字体</option>
                <option v-for="f in monospaceFonts" :key="'w-mono-' + f.name" :value="f.name">
                  {{ f.name }}
                </option>
              </select>
            </label>
          </div>

          <div class="grid grid-cols-1 gap-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/45 p-3 text-xs md:grid-cols-3">
            <label class="flex items-center justify-between gap-2">
              <span class="font-medium text-[var(--text-secondary)] shrink-0">界面字号：</span>
              <select
                v-model.number="fontSize"
                class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs font-mono"
              >
                <option v-for="sz in [11, 12, 13, 14, 15, 16, 18, 20, 22]" :key="'w-fs-' + sz" :value="sz">
                  {{ sz }}px{{ sz === 14 ? " (标准)" : "" }}
                </option>
              </select>
            </label>

            <label class="flex items-center justify-between gap-2">
              <span class="font-medium text-[var(--text-secondary)] shrink-0">代码字号：</span>
              <select
                v-model.number="codeFontSize"
                class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs font-mono"
              >
                <option v-for="sz in [10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24]" :key="'w-cfs-' + sz" :value="sz">
                  {{ sz }}px{{ sz === 13 ? " (标准)" : "" }}
                </option>
              </select>
            </label>

            <label class="flex items-center justify-between gap-2">
              <span class="flex items-center gap-1 font-medium text-[var(--text-secondary)] shrink-0">
                <Palette class="h-3.5 w-3.5 text-purple-500" />
                <span>流光：</span>
              </span>
              <select
                v-model="auroraIntensity"
                class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs"
              >
                <option value="rich">丰富流光（推荐）</option>
                <option value="subtle">轻量流光</option>
                <option value="off">关闭动效</option>
              </select>
            </label>
          </div>

          <!-- 关键配置汇总 -->
          <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 p-3.5 space-y-1.5 text-xs">
            <div class="font-bold text-[var(--text-primary)]">初始化配置汇总</div>
            <div class="grid grid-cols-1 gap-1 font-mono text-[0.6875rem] text-[var(--text-secondary)]">
              <div>配置文件：{{ snapshot?.yamlPath ?? "仅使用 GUI 独立配置" }}</div>
              <div>发送地址：{{ workingConfig.method }} {{ workingConfig.url || "(未填写)" }}（认证：{{ workingConfig.auth.type }}）</div>
              <div>测试消息源：{{ workingConfig.inputs.length }} 个路径（逐段确认：{{ workingConfig.interactive ? "开启" : "关闭" }}）</div>
              <div>字体与字号：正文={{ uiFont || "默认" }}({{ fontSize }}px) | 代码={{ monoFont || "默认" }}({{ codeFontSize }}px) | 流光={{ auroraIntensity }}</div>
            </div>
          </div>

          <!-- 持久化保存去向 -->
          <div class="space-y-2 text-xs">
            <div class="font-semibold text-[var(--text-secondary)]">选择配置保存方式：</div>
            <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
              <label
                class="flex cursor-pointer items-center gap-2 rounded-xl border p-2.5"
                :class="
                  saveTargetMode === 'gui'
                    ? 'border-cyan-500 bg-cyan-500/12'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50'
                "
              >
                <input v-model="saveTargetMode" type="radio" value="gui" class="accent-cyan-600" />
                <span>保存为 GUI 默认配置</span>
              </label>

              <label
                v-if="snapshot?.yamlPath"
                class="flex cursor-pointer items-center gap-2 rounded-xl border p-2.5"
                :class="
                  saveTargetMode === 'yaml'
                    ? 'border-indigo-500 bg-indigo-500/12'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50'
                "
              >
                <input v-model="saveTargetMode" type="radio" value="yaml" class="accent-indigo-600" />
                <span>同步写入当前 YAML</span>
              </label>

              <label
                class="flex cursor-pointer items-center gap-2 rounded-xl border p-2.5"
                :class="
                  saveTargetMode === 'new-yaml'
                    ? 'border-purple-500 bg-purple-500/12'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50'
                "
              >
                <input v-model="saveTargetMode" type="radio" value="new-yaml" class="accent-purple-600" />
                <span>另存为新 YAML 档案</span>
              </label>
            </div>
          </div>
        </div>
      </div>

      <!-- 底部步骤按钮 -->
      <div class="flex items-center justify-between border-t border-[var(--border-subtle)] px-5 py-3">
        <button
          type="button"
          :disabled="currentStep === 1"
          @click="currentStep = (currentStep - 1) as 1 | 2 | 3 | 4"
          class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium disabled:opacity-40"
        >
          <ArrowLeft class="h-3.5 w-3.5" />
          <span>上一步</span>
        </button>

        <div class="flex items-center gap-2">
          <button
            v-if="currentStep < 4"
            type="button"
            @click="currentStep = (currentStep + 1) as 1 | 2 | 3 | 4"
            class="aurora-btn inline-flex items-center gap-1 rounded-lg px-4 py-1.5 text-xs font-semibold text-white"
          >
            <span>下一步</span>
            <ArrowRight class="h-3.5 w-3.5" />
          </button>

          <button
            v-else
            type="button"
            :disabled="isFinishing"
            @click="handleFinishWizard"
            class="aurora-btn inline-flex items-center gap-1.5 rounded-lg px-4 py-1.5 text-xs font-semibold text-white disabled:opacity-50"
          >
            <CheckCircle2 v-if="isFinishing" class="h-3.5 w-3.5 animate-pulse" />
            <Rocket v-else class="h-3.5 w-3.5" />
            <span>{{ isFinishing ? "正在保存…" : "完成并开始使用" }}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
