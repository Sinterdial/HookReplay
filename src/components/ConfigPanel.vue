<script setup lang="ts">
/**
 * 中心面板：双源高级配置编辑器（ConfigPanel）
 *
 * 完整覆盖原有 `notify-test.config.yaml` 全部 28 个字段与 V2 通用扩展字段，支持：
 * 1. 可视化表单 <-> YAML 源码编辑 一键切换；
 * 2. 按 `fieldSources` 自动标注「YAML 优先」/「GUI 配置」来源徽章；
 * 3. 分开持久化：「保存为 GUI 默认配置」或「同步写入当前 / 另存为 YAML」。
 */
import { computed, onMounted, ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ArrowRightLeft,
  Check,
  CheckCircle2,
  Code2,
  Eye,
  FileCheck,
  FileCode2,
  FilePlus,
  FolderOpen,
  FolderPlus,
  Info,
  Palette,
  Pencil,
  Plus,
  Save,
  SlidersHorizontal,
  Trash2,
  Wand2,
  X,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useTheme } from "../composables/useTheme";
import type { FieldSource, TestMarkConfig } from "../types";

const {
  snapshot,
  workingConfig,
  rawYamlDraft,
  configTemplates,
  isBusy,
  leftTab,
  showAppearanceModal,
  showSetupWizard,
  scanSources,
  selectYamlProfile,
  mountYamlProfiles,
  unmountYamlProfile,
  renameYamlProfile,
  addProfileDir,
  removeProfileDir,
  loadConfigTemplates,
  createConfigFromTemplate,
  saveAsGuiConfig,
  saveToYamlFile,
  saveRawYamlSource,
} = useAppState();

const { uiFont, monoFont, fontSize, codeFontSize, auroraIntensity } = useTheme();

const editorMode = ref<"form" | "yaml">("form");

/** 多配置文件挂载与内置模板生成状态 */
const customProfilePathInput = ref("");
const selectedTemplateId = ref("n8n-webhook");
const showTemplatePreview = ref(false);
const editingAliasPath = ref<string | null>(null);
const editingAliasDraft = ref("");

onMounted(() => {
  if (configTemplates.value.length === 0) {
    void loadConfigTemplates();
  }
});

const selectedTemplateObj = computed(() =>
  configTemplates.value.find((t) => t.id === selectedTemplateId.value) ?? null
);

async function browseMountMultiYaml(): Promise<void> {
  const picked = await open({
    title: "挂载一个或多个 YAML / JSONC 配置文件",
    multiple: true,
    filters: [
      { name: "YAML / JSONC 配置", extensions: ["yaml", "yml", "json", "jsonc"] },
    ],
  });
  if (!picked) return;
  const list = Array.isArray(picked) ? picked : [picked];
  if (list.length > 0) {
    await mountYamlProfiles(list, true);
  }
}

async function mountCustomPath(): Promise<void> {
  const p = customProfilePathInput.value.trim();
  if (!p) return;
  const ok = await mountYamlProfiles([p], true);
  if (ok) {
    customProfilePathInput.value = "";
  }
}

async function browseAddProfileDir(): Promise<void> {
  const picked = await open({
    title: "选择用于自动扫描配置文件的自定义目录",
    directory: true,
    multiple: false,
  });
  if (typeof picked === "string" && picked.trim()) {
    await addProfileDir(picked);
  }
}

function startRenameAlias(path: string, currentName: string): void {
  editingAliasPath.value = path;
  editingAliasDraft.value = currentName;
}

async function commitRenameAlias(): Promise<void> {
  if (!editingAliasPath.value) return;
  await renameYamlProfile(editingAliasPath.value, editingAliasDraft.value);
  editingAliasPath.value = null;
  editingAliasDraft.value = "";
}

async function handleCreateFromTemplate(): Promise<void> {
  const tpl = selectedTemplateObj.value;
  const picked = await save({
    title: "从内置模板生成 YAML 配置文件",
    defaultPath: tpl?.suggestedFileName ?? "hook-replay.config.yaml",
    filters: [{ name: "YAML 配置", extensions: ["yaml", "yml"] }],
  });
  if (!picked) return;
  await createConfigFromTemplate(selectedTemplateId.value, picked, true);
}

/** 用于动态添加请求头 / 模板变量的临时输入草稿 */
const newHeaderKey = ref("");
const newHeaderVal = ref("");
const newVarKey = ref("");
const newVarVal = ref("");

/** 字段来源标注徽章样式与文案 */
function getSourceBadge(fieldKey: string): { label: string; cls: string } | null {
  const src: FieldSource | undefined = snapshot.value?.fieldSources[fieldKey];
  if (src === "yaml") {
    return {
      label: "YAML 优先",
      cls: "bg-indigo-500/15 text-indigo-600 dark:text-indigo-300",
    };
  }
  if (src === "gui") {
    return {
      label: "GUI 配置",
      cls: "bg-cyan-500/15 text-cyan-600 dark:text-cyan-300",
    };
  }
  return null;
}

/** 将数组字段以逗号分隔字符串双向绑定，提升表单编辑效率 */
function useCsvBinding(getter: () => string[], setter: (arr: string[]) => void) {
  return computed<string>({
    get: () => getter().join(", "),
    set: (val: string) => {
      setter(
        val
          .split(",")
          .map((s) => s.trim())
          .filter((s) => s.length > 0)
      );
    },
  });
}

function useNumCsvBinding(getter: () => number[], setter: (arr: number[]) => void) {
  return computed<string>({
    get: () => getter().join(", "),
    set: (val: string) => {
      const nums = val
        .split(",")
        .map((s) => Number(s.trim()))
        .filter((n) => Number.isInteger(n) && n >= 100 && n <= 599);
      setter(nums);
    },
  });
}

const extensionsText = useCsvBinding(
  () => workingConfig.value.extensions,
  (arr) => (workingConfig.value.extensions = arr)
);
const commentPrefixesText = useCsvBinding(
  () => workingConfig.value.commentPrefixes,
  (arr) => (workingConfig.value.commentPrefixes = arr)
);
const typesText = useCsvBinding(
  () => workingConfig.value.types,
  (arr) => (workingConfig.value.types = arr)
);
const typePathsText = useCsvBinding(
  () => workingConfig.value.labelRules.typePaths,
  (arr) => (workingConfig.value.labelRules.typePaths = arr)
);
const titlePathsText = useCsvBinding(
  () => workingConfig.value.labelRules.titlePaths,
  (arr) => (workingConfig.value.labelRules.titlePaths = arr)
);
const expectCodesText = useNumCsvBinding(
  () => workingConfig.value.assertions.expect,
  (arr) => {
    workingConfig.value.assertions.expect = arr;
    workingConfig.value.expect = arr;
  }
);
const softFailCodesText = useNumCsvBinding(
  () => workingConfig.value.assertions.softFailStatus,
  (arr) => (workingConfig.value.assertions.softFailStatus = arr)
);

/** 控制高级 testMark 对象的启用与关闭 */
const useAdvancedTestMark = computed<boolean>({
  get: () => workingConfig.value.testMark !== null,
  set: (enabled: boolean) => {
    if (enabled) {
      const defTm: TestMarkConfig = {
        mode: workingConfig.value.markTest ? "body" : "none",
        field: "__test",
        value: true,
      };
      workingConfig.value.testMark = defTm;
    } else {
      workingConfig.value.testMark = null;
    }
  },
});

/** 将表单里的测试标记值字符串智能转为 boolean / number / string */
const testMarkValueText = computed<string>({
  get: () => {
    const v = workingConfig.value.testMark?.value;
    if (v === undefined || v === null) return "";
    return typeof v === "string" ? v : JSON.stringify(v);
  },
  set: (raw: string) => {
    if (!workingConfig.value.testMark) return;
    const t = raw.trim();
    if (t === "true") workingConfig.value.testMark.value = true;
    else if (t === "false") workingConfig.value.testMark.value = false;
    else if (t !== "" && !Number.isNaN(Number(t))) {
      workingConfig.value.testMark.value = Number(t);
    } else {
      workingConfig.value.testMark.value = raw;
    }
  },
});

/** 动态维护 endpoints 预设 */
function addEndpointPreset(): void {
  workingConfig.value.endpoints.push({ name: "新入口", url: "" });
}
function removeEndpointPreset(idx: number): void {
  workingConfig.value.endpoints.splice(idx, 1);
}

/** 动态维护自定义请求头 */
function addCustomHeader(): void {
  const k = newHeaderKey.value.trim();
  if (!k) return;
  workingConfig.value.headers[k] = newHeaderVal.value;
  newHeaderKey.value = "";
  newHeaderVal.value = "";
}
function removeCustomHeader(key: string): void {
  delete workingConfig.value.headers[key];
}

/** 动态维护 {{VAR}} 环境变量 */
function addVariable(): void {
  const k = newVarKey.value.trim();
  if (!k) return;
  workingConfig.value.variables[k] = newVarVal.value;
  newVarKey.value = "";
  newVarVal.value = "";
}
function removeVariable(key: string): void {
  delete workingConfig.value.variables[key];
}

/** 动态维护响应体 JSON 断言规则 */
function addBodyRule(): void {
  workingConfig.value.assertions.bodyRules.push({
    path: "deduped",
    equals: true,
    badge: "幂等跳过",
    tone: "info",
  });
}
function removeBodyRule(idx: number): void {
  workingConfig.value.assertions.bodyRules.splice(idx, 1);
}
function updateBodyRuleEquals(idx: number, raw: string): void {
  const rule = workingConfig.value.assertions.bodyRules[idx];
  if (!rule) return;
  const t = raw.trim();
  if (t === "true") rule.equals = true;
  else if (t === "false") rule.equals = false;
  else if (t !== "" && !Number.isNaN(Number(t))) rule.equals = Number(t);
  else rule.equals = raw;
}
function formatBodyRuleEquals(val: unknown): string {
  if (val === null || val === undefined) return "";
  return typeof val === "string" ? val : String(val);
}

/** 选取输入文件或目录追加到 inputs */
async function browseAddInput(dirMode: boolean): Promise<void> {
  const picked = await open({
    title: dirMode ? "选择输入日志文件夹" : "选择输入日志文件",
    directory: dirMode,
    multiple: !dirMode,
  });
  if (!picked) return;
  const items = Array.isArray(picked) ? picked : [picked];
  for (const p of items) {
    if (p && !workingConfig.value.inputs.includes(p)) {
      workingConfig.value.inputs.push(p);
    }
  }
}

/** 选取响应保存目录 */
async function browseSaveResponseDir(): Promise<void> {
  const picked = await open({
    title: "选择逐条响应文本保存目录",
    directory: true,
    multiple: false,
  });
  if (typeof picked === "string" && picked.trim()) {
    workingConfig.value.saveResponseDir = picked;
  }
}

/** 同步写回当前 YAML 或弹窗另存为新 YAML */
async function handleWriteYaml(forceSaveAs = false): Promise<void> {
  let targetPath = !forceSaveAs ? snapshot.value?.yamlPath ?? null : null;
  if (!targetPath) {
    const picked = await save({
      title: "保存配置为 YAML 文件",
      defaultPath: "hook-replay.config.yaml",
      filters: [{ name: "YAML 配置", extensions: ["yaml", "yml"] }],
    });
    if (!picked) return;
    targetPath = picked;
  }
  await saveToYamlFile(targetPath, workingConfig.value);
}

/** 校验并保存 YAML 源码 */
async function handleSaveRawYaml(): Promise<void> {
  let targetPath = snapshot.value?.yamlPath ?? null;
  if (!targetPath) {
    const picked = await save({
      title: "保存为 YAML 文件",
      defaultPath: "hook-replay.config.yaml",
      filters: [{ name: "YAML 配置", extensions: ["yaml", "yml"] }],
    });
    if (!picked) return;
    targetPath = picked;
  }
  await saveRawYamlSource(targetPath, rawYamlDraft.value);
}
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col bg-[var(--bg-canvas)]">
    <!-- 顶部栏：表单与源码模式切换 -->
    <div
      class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--border-subtle)] bg-[var(--bg-surface)] px-4 py-2.5"
    >
      <div class="flex items-center gap-2">
        <SlidersHorizontal class="h-4 w-4 text-indigo-500" />
        <h2 class="text-sm font-bold text-[var(--text-primary)]">
          配置中心（YAML > GUI > 默认值）
        </h2>
        <span
          class="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 font-mono text-[0.625rem] text-[var(--text-muted)]"
        >
          基准目录：{{ snapshot?.baseDir ?? "-" }}
        </span>
      </div>

      <div class="flex items-center gap-1.5 rounded-lg bg-[var(--bg-elevated)] p-1">
        <button
          type="button"
          @click="editorMode = 'form'"
          class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition"
          :class="
            editorMode === 'form'
              ? 'aurora-tab-active bg-[var(--bg-surface)] text-[var(--text-primary)] shadow-xs'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
        >
          <SlidersHorizontal class="h-3.5 w-3.5" />
          <span>可视化表单</span>
        </button>
        <button
          type="button"
          @click="editorMode = 'yaml'"
          class="inline-flex items-center gap-1 rounded-md px-2.5 py-1 text-xs font-medium transition"
          :class="
            editorMode === 'yaml'
              ? 'aurora-tab-active bg-[var(--bg-surface)] text-[var(--text-primary)] shadow-xs'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
        >
          <Code2 class="h-3.5 w-3.5" />
          <span>YAML 源码</span>
        </button>
      </div>
    </div>

    <!-- YAML 语法报错条 -->
    <div
      v-if="snapshot?.yamlError"
      class="mx-4 mt-3 rounded-xl border border-rose-500/40 bg-rose-500/10 px-3.5 py-2 text-xs text-rose-700 dark:text-rose-300"
    >
      <strong>YAML 解析错误：</strong>{{ snapshot.yamlError }}
    </div>

    <!-- 模式 A：可视化表单（含多档案挂载、模板与全量字段） -->
    <div v-if="editorMode === 'form'" class="min-h-0 flex-1 overflow-y-auto p-4 space-y-4">
      <!-- 0. 配置文件挂载、多档案一键切换与内置模板 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3.5">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div class="text-xs font-bold uppercase tracking-wider text-indigo-600 dark:text-indigo-400">
            0. 配置文件挂载、多档案一键切换与内置模板
          </div>
          <div class="flex flex-wrap items-center gap-1.5">
            <button
              type="button"
              :disabled="isBusy"
              @click="browseMountMultiYaml"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1 text-[0.6875rem] font-medium text-[var(--text-primary)] disabled:opacity-45"
            >
              <FilePlus class="h-3.5 w-3.5 text-blue-500" />
              <span>+ 挂载多配置</span>
            </button>
            <button
              type="button"
              :disabled="isBusy"
              @click="browseAddProfileDir"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1 text-[0.6875rem] font-medium text-[var(--text-primary)] disabled:opacity-45"
            >
              <FolderPlus class="h-3.5 w-3.5 text-amber-500" />
              <span>+ 扫描配置目录</span>
            </button>
            <button
              type="button"
              :disabled="isBusy"
              @click="selectYamlProfile(null)"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[0.6875rem] font-medium"
              :class="
                !snapshot?.yamlPath
                  ? 'border-cyan-500/50 bg-cyan-500/12 text-cyan-700 dark:text-cyan-300'
                  : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-secondary)]'
              "
            >
              <span>纯 GUI 独立配置</span>
            </button>
          </div>
        </div>

        <!-- 自定义路径直接挂载 -->
        <div class="flex items-center gap-2">
          <input
            v-model="customProfilePathInput"
            type="text"
            placeholder="输入任意自定义 YAML / JSONC 配置文件路径并挂载…"
            @keydown.enter.prevent="mountCustomPath"
            class="h-8 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none focus:border-indigo-500"
          />
          <button
            type="button"
            :disabled="!customProfilePathInput.trim() || isBusy"
            @click="mountCustomPath"
            class="aurora-btn-subtle h-8 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 text-xs font-medium disabled:opacity-45"
          >
            挂载路径
          </button>
        </div>

        <!-- 自定义扫描配置目录列表 -->
        <div
          v-if="(snapshot?.customProfileDirs.length ?? 0) > 0"
          class="flex flex-wrap items-center gap-1.5 text-[0.6875rem]"
        >
          <span class="text-[var(--text-muted)]">已挂载扫描目录：</span>
          <span
            v-for="dir in snapshot?.customProfileDirs ?? []"
            :key="dir"
            class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-0.5 font-mono text-[0.625rem] text-[var(--text-secondary)]"
          >
            <span>{{ dir }}</span>
            <button
              type="button"
              @click="removeProfileDir(dir)"
              title="移除该扫描目录"
              class="text-[var(--text-muted)] hover:text-rose-500"
            >
              <X class="h-3 w-3" />
            </button>
          </span>
        </div>

        <!-- 已挂载/已发现档案一键切换列表 -->
        <div class="space-y-1.5">
          <div class="text-[0.6875rem] font-medium text-[var(--text-secondary)]">
            已挂载与自动发现的配置档案（{{ snapshot?.profiles.length ?? 0 }} 个）
          </div>
          <div class="max-h-48 overflow-y-auto space-y-1.5 pr-0.5">
            <div
              v-for="prof in snapshot?.profiles ?? []"
              :key="prof.path"
              class="flex flex-wrap items-center justify-between gap-2 rounded-xl border px-3 py-2 text-xs transition"
              :class="
                snapshot?.yamlPath === prof.path
                  ? 'border-indigo-500/60 bg-indigo-500/10'
                  : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/55'
              "
            >
              <div class="min-w-0 flex-1 space-y-0.5">
                <div class="flex flex-wrap items-center gap-1.5">
                  <template v-if="editingAliasPath === prof.path">
                    <input
                      v-model="editingAliasDraft"
                      type="text"
                      @keydown.enter.prevent="commitRenameAlias"
                      class="h-6 rounded border border-indigo-500 bg-[var(--bg-surface)] px-2 text-xs outline-none"
                    />
                    <button
                      type="button"
                      @click="commitRenameAlias"
                      class="rounded bg-indigo-600 px-1.5 py-0.5 text-[0.625rem] text-white"
                    >
                      <Check class="h-3 w-3" />
                    </button>
                  </template>
                  <template v-else>
                    <span class="font-semibold text-[var(--text-primary)] truncate">
                      {{ prof.name }}
                    </span>
                    <button
                      type="button"
                      @click="startRenameAlias(prof.path, prof.name)"
                      title="编辑别名"
                      class="text-[var(--text-muted)] hover:text-[var(--text-primary)]"
                    >
                      <Pencil class="h-3 w-3" />
                    </button>
                  </template>

                  <span
                    class="rounded px-1.5 py-0.5 text-[0.625rem] font-medium"
                    :class="
                      prof.sourceKind === 'mounted'
                        ? 'bg-blue-500/12 text-blue-600 dark:text-blue-300'
                        : prof.sourceKind === 'dir'
                          ? 'bg-amber-500/12 text-amber-600 dark:text-amber-300'
                          : 'bg-slate-500/12 text-slate-600 dark:text-slate-300'
                    "
                  >
                    {{
                      prof.sourceKind === "mounted"
                        ? "手动挂载"
                        : prof.sourceKind === "dir"
                          ? "目录扫描"
                          : "自动发现"
                    }}
                  </span>

                  <span
                    v-if="!prof.exists"
                    class="rounded bg-rose-500/15 px-1.5 py-0.5 text-[0.625rem] font-semibold text-rose-600 dark:text-rose-300"
                  >
                    文件缺失
                  </span>
                  <span
                    v-if="snapshot?.yamlPath === prof.path"
                    class="rounded bg-indigo-600 px-1.5 py-0.5 text-[0.625rem] font-semibold text-white"
                  >
                    当前生效
                  </span>
                </div>

                <div class="truncate font-mono text-[0.625rem] text-[var(--text-muted)]" :title="prof.path">
                  {{ prof.path }}
                </div>
              </div>

              <div class="flex shrink-0 items-center gap-1.5">
                <button
                  v-if="snapshot?.yamlPath !== prof.path && prof.exists"
                  type="button"
                  :disabled="isBusy"
                  @click="selectYamlProfile(prof.path)"
                  class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-indigo-500/35 bg-indigo-500/10 px-2.5 py-1 text-[0.6875rem] font-semibold text-indigo-700 dark:text-indigo-300"
                >
                  <ArrowRightLeft class="h-3 w-3" />
                  <span>一键切换</span>
                </button>
                <button
                  type="button"
                  :disabled="isBusy"
                  @click="unmountYamlProfile(prof.path)"
                  title="从挂载列表移除"
                  class="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-1 text-[var(--text-muted)] hover:border-rose-500/40 hover:text-rose-500"
                >
                  <Trash2 class="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div
              v-if="(snapshot?.profiles.length ?? 0) === 0"
              class="rounded-xl border border-dashed border-[var(--border-strong)] py-4 text-center text-xs text-[var(--text-muted)]"
            >
              暂无已挂载的 YAML 档案，可点击上方「+ 挂载多配置」或下方模板快速生成。
            </div>
          </div>
        </div>

        <!-- 内置模板区域 -->
        <div class="border-t border-[var(--border-subtle)] pt-3 space-y-2.5">
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-secondary)]">
              <FileCode2 class="h-4 w-4 text-purple-500" />
              <span>内置模板快速新建配置</span>
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                @click="showTemplatePreview = !showTemplatePreview"
                class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
              >
                <Eye class="h-3 w-3" />
                <span>{{ showTemplatePreview ? "收起模板内容" : "预览模板内容" }}</span>
              </button>
              <button
                type="button"
                @click="handleCreateFromTemplate"
                class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-purple-500/40 bg-purple-500/12 px-2.5 py-1 text-[0.6875rem] font-semibold text-purple-700 dark:text-purple-300"
              >
                <FilePlus class="h-3.5 w-3.5" />
                <span>从此模板新建并挂载…</span>
              </button>
            </div>
          </div>

          <div class="grid grid-cols-1 gap-2 md:grid-cols-3">
            <button
              v-for="tpl in configTemplates"
              :key="tpl.id"
              type="button"
              @click="selectedTemplateId = tpl.id"
              class="rounded-xl border p-2.5 text-left transition"
              :class="
                selectedTemplateId === tpl.id
                  ? 'border-purple-500 bg-purple-500/10'
                  : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50 hover:bg-[var(--bg-elevated)]'
              "
            >
              <div class="text-xs font-semibold text-[var(--text-primary)]">{{ tpl.name }}</div>
              <div class="mt-0.5 text-[0.625rem] leading-snug text-[var(--text-muted)]">
                {{ tpl.description }}
              </div>
            </button>
          </div>

          <pre
            v-if="showTemplatePreview && selectedTemplateObj"
            class="code-editor-text max-h-56 overflow-auto rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)] p-3 text-[var(--text-primary)]"
          >{{ selectedTemplateObj.content }}</pre>
        </div>

        <!-- 外观字体与向导快捷入口 -->
        <div class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--border-subtle)] pt-3 text-xs">
          <div class="text-[0.6875rem] text-[var(--text-muted)]">
            当前字体：正文 <span class="font-medium text-[var(--text-primary)]">{{ uiFont || "系统默认" }}</span> · 等宽 <span class="font-medium text-[var(--text-primary)]">{{ monoFont || "默认等宽" }}</span> · 界面 {{ fontSize }}px · 代码 {{ codeFontSize }}px · 流光 {{ auroraIntensity }}
          </div>
          <div class="flex items-center gap-2">
            <button
              type="button"
              @click="showAppearanceModal = true"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1 text-[0.6875rem] font-medium"
            >
              <Palette class="h-3.5 w-3.5 text-purple-500" />
              <span>本机字体与流光设置</span>
            </button>
            <button
              type="button"
              @click="showSetupWizard = true"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1 text-[0.6875rem] font-medium"
            >
              <Wand2 class="h-3.5 w-3.5 text-indigo-500" />
              <span>重新运行配置向导</span>
            </button>
          </div>
        </div>
      </div>

      <!-- 1. 档案与目标端点 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-blue-600 dark:text-blue-400">
          1. 目标端点与预设入口（Endpoints）
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          <label class="space-y-1 text-xs">
            <div class="flex items-center justify-between">
              <span class="font-medium text-[var(--text-secondary)]">档案名称 (name)</span>
              <span
                v-if="getSourceBadge('name')"
                class="rounded px-1.5 py-0.5 text-[0.625rem] font-semibold"
                :class="getSourceBadge('name')!.cls"
              >
                {{ getSourceBadge("name")!.label }}
              </span>
            </div>
            <input
              v-model="workingConfig.name"
              type="text"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none focus:border-blue-500"
            />
          </label>

          <label class="space-y-1 text-xs">
            <span class="font-medium text-[var(--text-secondary)]">档案说明 (description)</span>
            <input
              v-model="workingConfig.description"
              type="text"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none focus:border-blue-500"
            />
          </label>

          <label class="space-y-1 text-xs md:col-span-2">
            <div class="flex items-center justify-between">
              <span class="font-medium text-[var(--text-secondary)]">
                目标 Webhook URL (支持 <code v-pre class="font-mono">{{VAR}}</code> 插值)
              </span>
              <span
                v-if="getSourceBadge('url')"
                class="rounded px-1.5 py-0.5 text-[0.625rem] font-semibold"
                :class="getSourceBadge('url')!.cls"
              >
                {{ getSourceBadge("url")!.label }}
              </span>
            </div>
            <input
              v-model="workingConfig.url"
              type="text"
              placeholder="https://n8n.example.com/webhook/demo"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none focus:border-blue-500"
            />
          </label>

          <label class="space-y-1 text-xs">
            <span class="font-medium text-[var(--text-secondary)]">HTTP 请求方法 (method)</span>
            <select
              v-model="workingConfig.method"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none focus:border-blue-500"
            >
              <option value="POST">POST</option>
              <option value="PUT">PUT</option>
              <option value="PATCH">PATCH</option>
              <option value="GET">GET</option>
            </select>
          </label>

          <label class="flex items-center gap-2 pt-5 text-xs text-[var(--text-primary)] cursor-pointer">
            <input
              v-model="workingConfig.insecure"
              type="checkbox"
              class="h-4 w-4 rounded accent-blue-600"
            />
            <span>跳过 HTTPS 自签名证书校验 (insecure)</span>
            <span
              v-if="getSourceBadge('insecure')"
              class="rounded px-1.5 py-0.5 text-[0.625rem] font-semibold"
              :class="getSourceBadge('insecure')!.cls"
            >
              {{ getSourceBadge("insecure")!.label }}
            </span>
          </label>
        </div>

        <!-- 预设入口 endpoints -->
        <div class="pt-2 space-y-2 border-t border-[var(--border-subtle)]">
          <div class="flex items-center justify-between text-xs">
            <span class="font-medium text-[var(--text-secondary)]">
              命名预设入口 (endpoints · 供顶栏一键切换)
            </span>
            <button
              type="button"
              @click="addEndpointPreset"
              class="inline-flex items-center gap-1 rounded-md bg-blue-600/12 px-2 py-1 text-[0.6875rem] font-semibold text-blue-600 hover:bg-blue-600/20 dark:text-blue-300"
            >
              <Plus class="h-3 w-3" />
              <span>新增入口</span>
            </button>
          </div>
          <div
            v-for="(ep, idx) in workingConfig.endpoints"
            :key="idx"
            class="flex items-center gap-2"
          >
            <input
              v-model="ep.name"
              placeholder="入口名称（如：正式环境）"
              class="h-7 w-36 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs outline-none"
            />
            <input
              v-model="ep.url"
              placeholder="https://..."
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="workingConfig.url = ep.url"
              class="rounded-md border border-[var(--border-subtle)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-elevated)]"
            >
              设为当前
            </button>
            <button
              type="button"
              @click="removeEndpointPreset(idx)"
              class="text-[var(--text-muted)] hover:text-rose-500"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- 2. 身份认证、自定义请求头与 HMAC 签名 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-indigo-600 dark:text-indigo-400">
          2. 身份认证（Auth）、请求头与 HMAC 签名
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
          <label class="space-y-1 text-xs">
            <div class="flex items-center justify-between">
              <span class="font-medium text-[var(--text-secondary)]">认证方式 (auth.type)</span>
              <span
                v-if="getSourceBadge('auth.type')"
                class="rounded px-1.5 py-0.5 text-[0.625rem] font-semibold"
                :class="getSourceBadge('auth.type')!.cls"
              >
                {{ getSourceBadge("auth.type")!.label }}
              </span>
            </div>
            <select
              v-model="workingConfig.auth.type"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs outline-none"
            >
              <option value="none">none（无认证）</option>
              <option value="basic">basic（用户名/密码）</option>
              <option value="bearer">bearer（Bearer Token）</option>
              <option value="header">header（自定义头令牌）</option>
              <option value="query">query（URL 参数令牌）</option>
            </select>
          </label>

          <template v-if="workingConfig.auth.type === 'basic'">
            <label class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">用户名 (username)</span>
              <input
                v-model="workingConfig.auth.username"
                type="text"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
              />
            </label>
            <label class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">密码 (password)</span>
              <input
                v-model="workingConfig.auth.password"
                type="password"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
              />
            </label>
          </template>

          <template v-else-if="workingConfig.auth.type !== 'none'">
            <label v-if="workingConfig.auth.type === 'header'" class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">请求头名称 (headerName)</span>
              <input
                v-model="workingConfig.auth.headerName"
                type="text"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none"
              />
            </label>
            <label v-if="workingConfig.auth.type === 'query'" class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">URL 参数名 (queryName)</span>
              <input
                v-model="workingConfig.auth.queryName"
                type="text"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none"
              />
            </label>
            <label class="space-y-1 text-xs">
              <span class="font-medium text-[var(--text-secondary)]">令牌值 (token)</span>
              <input
                v-model="workingConfig.auth.token"
                type="password"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none"
              />
            </label>
          </template>
        </div>

        <!-- 自定义请求头 -->
        <div class="pt-2 space-y-2 border-t border-[var(--border-subtle)]">
          <div class="text-xs font-medium text-[var(--text-secondary)]">
            自定义请求头 (headers)
          </div>
          <div
            v-for="(_val, key) in workingConfig.headers"
            :key="key"
            class="flex items-center gap-2 text-xs"
          >
            <span class="w-40 truncate font-mono text-[var(--text-primary)]">{{ key }}</span>
            <input
              v-model="workingConfig.headers[key]"
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="removeCustomHeader(String(key))"
              class="text-[var(--text-muted)] hover:text-rose-500"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>
          <div class="flex items-center gap-2">
            <input
              v-model="newHeaderKey"
              placeholder="Header 键名，如 X-Env"
              class="h-7 w-40 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <input
              v-model="newHeaderVal"
              placeholder="值（支持 {{VAR}}）"
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="addCustomHeader"
              class="rounded-md bg-[var(--bg-elevated)] px-2.5 py-1 text-xs font-medium hover:bg-[var(--bg-hover)]"
            >
              添加 Header
            </button>
          </div>
        </div>

        <!-- Webhook HMAC 签名配置 -->
        <div class="pt-2 space-y-2 border-t border-[var(--border-subtle)]">
          <label class="flex items-center gap-2 text-xs font-medium text-[var(--text-primary)] cursor-pointer">
            <input
              v-model="workingConfig.signature.enabled"
              type="checkbox"
              class="h-4 w-4 rounded accent-indigo-600"
            />
            <span>启用 Webhook HMAC 签名 (signature.enabled)</span>
          </label>
          <div
            v-if="workingConfig.signature.enabled"
            class="grid grid-cols-1 gap-2.5 md:grid-cols-4 pt-1"
          >
            <label class="space-y-1 text-xs">
              <span class="text-[var(--text-muted)]">签名算法</span>
              <select
                v-model="workingConfig.signature.algorithm"
                class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs"
              >
                <option value="hmac-sha256">hmac-sha256</option>
                <option value="hmac-sha1">hmac-sha1</option>
              </select>
            </label>
            <label class="space-y-1 text-xs">
              <span class="text-[var(--text-muted)]">签名头 (headerName)</span>
              <input
                v-model="workingConfig.signature.headerName"
                class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
              />
            </label>
            <label class="space-y-1 text-xs">
              <span class="text-[var(--text-muted)]">前缀 (prefix)</span>
              <input
                v-model="workingConfig.signature.prefix"
                class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
              />
            </label>
            <label class="space-y-1 text-xs">
              <span class="text-[var(--text-muted)]">密钥 (secret)</span>
              <input
                v-model="workingConfig.signature.secret"
                type="password"
                placeholder="{{WEBHOOK_SECRET}}"
                class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
              />
            </label>
          </div>
        </div>
      </div>

      <!-- 3. 发送节奏、重试与测试标记注入 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-emerald-600 dark:text-emerald-400">
          3. 发送流控、逐段确认与测试标记注入
        </div>

        <div class="grid grid-cols-2 gap-3 md:grid-cols-3">
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">同段并发数 (concurrency)</span>
            <input
              v-model.number="workingConfig.concurrency"
              type="number"
              min="1"
              max="32"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">条间间隔 ms (delayMs)</span>
            <input
              v-model.number="workingConfig.delayMs"
              type="number"
              min="0"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">段间等待 ms (segmentWaitMs)</span>
            <input
              v-model.number="workingConfig.segmentWaitMs"
              type="number"
              min="0"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">请求超时 ms (timeoutMs)</span>
            <input
              v-model.number="workingConfig.timeoutMs"
              type="number"
              min="500"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">重试次数 (retries)</span>
            <input
              v-model.number="workingConfig.retries"
              type="number"
              min="0"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">重试间隔 ms (retryDelayMs)</span>
            <input
              v-model.number="workingConfig.retryDelayMs"
              type="number"
              min="0"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none"
            />
          </label>
        </div>

        <div class="flex flex-wrap gap-5 pt-1 text-xs">
          <label class="flex items-center gap-2 cursor-pointer">
            <input
              v-model="workingConfig.interactive"
              type="checkbox"
              class="h-4 w-4 rounded accent-blue-600"
            />
            <span>逐段发送前暂停确认 (interactive)</span>
          </label>

          <label class="flex items-center gap-2 cursor-pointer">
            <input
              v-model="workingConfig.markTest"
              type="checkbox"
              class="h-4 w-4 rounded accent-blue-600"
            />
            <span>注入测试标记 __test: true (markTest)</span>
          </label>

          <label class="flex items-center gap-2 cursor-pointer">
            <input
              v-model="useAdvancedTestMark"
              type="checkbox"
              class="h-4 w-4 rounded accent-indigo-600"
            />
            <span>启用 V2 自定义注入位置 (testMark)</span>
          </label>
        </div>

        <div
          v-if="workingConfig.testMark"
          class="grid grid-cols-1 gap-2.5 md:grid-cols-3 rounded-xl bg-[var(--bg-elevated)]/70 p-3 text-xs"
        >
          <label class="space-y-1">
            <span class="text-[var(--text-muted)]">注入位置 (mode)</span>
            <select
              v-model="workingConfig.testMark.mode"
              class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 text-xs"
            >
              <option value="body">body (JSON 体点分路径)</option>
              <option value="header">header (HTTP 请求头)</option>
              <option value="query">query (URL 查询参数)</option>
              <option value="none">none (不注入)</option>
            </select>
          </label>
          <label class="space-y-1">
            <span class="text-[var(--text-muted)]">字段名 / 路径 (field)</span>
            <input
              v-model="workingConfig.testMark.field"
              placeholder="__test 或 meta.is_test"
              class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 font-mono text-xs"
            />
          </label>
          <label class="space-y-1">
            <span class="text-[var(--text-muted)]">注入值 (value)</span>
            <input
              v-model="testMarkValueText"
              placeholder="true / replay"
              class="h-7 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 font-mono text-xs"
            />
          </label>
        </div>
      </div>

      <!-- 4. 数据源扫描与过滤规则 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-amber-600 dark:text-amber-400">
          4. 数据源扫描、格式与过滤规则
        </div>

        <div class="space-y-2">
          <div class="flex items-center justify-between text-xs">
            <span class="font-medium text-[var(--text-secondary)]">输入文件或目录列表 (inputs)</span>
            <div class="flex items-center gap-1.5">
              <button
                type="button"
                @click="browseAddInput(false)"
                class="inline-flex items-center gap-1 rounded bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
              >
                <FilePlus class="h-3 w-3" />
                <span>+ 文件</span>
              </button>
              <button
                type="button"
                @click="browseAddInput(true)"
                class="inline-flex items-center gap-1 rounded bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
              >
                <FolderOpen class="h-3 w-3" />
                <span>+ 目录</span>
              </button>
            </div>
          </div>

          <div
            v-for="(_, idx) in workingConfig.inputs"
            :key="idx"
            class="flex items-center gap-2"
          >
            <input
              v-model="workingConfig.inputs[idx]"
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="workingConfig.inputs.splice(idx, 1)"
              class="text-[var(--text-muted)] hover:text-rose-500"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">日志格式 (format)</span>
            <select
              v-model="workingConfig.format"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs"
            >
              <option value="auto">auto（自动探测）</option>
              <option value="jsonl">jsonl（逐行 JSON）</option>
              <option value="json">json（单对象或数组）</option>
            </select>
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">目录允许扩展名 (extensions)</span>
            <input
              v-model="extensionsText"
              placeholder="txt, log, json"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">分段注释前缀 (commentPrefixes)</span>
            <input
              v-model="commentPrefixesText"
              placeholder="#, //"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">仅发事件类型 (types)</span>
            <input
              v-model="typesText"
              placeholder="逗号分隔，留空不限制"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">包含正则 (include)</span>
            <input
              v-model="workingConfig.include"
              placeholder="例如：order|alert"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">排除正则 (exclude)</span>
            <input
              v-model="workingConfig.exclude"
              placeholder="例如：【测试】"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">最大发送条数 (limit, 0=不限)</span>
            <input
              v-model.number="workingConfig.limit"
              type="number"
              min="0"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs"
            />
          </label>
          <label class="flex items-center gap-2 pt-5 text-xs cursor-pointer">
            <input
              v-model="workingConfig.recursive"
              type="checkbox"
              class="h-4 w-4 rounded accent-blue-600"
            />
            <span>递归扫描目录 (recursive)</span>
          </label>
          <label class="flex items-center gap-2 pt-5 text-xs cursor-pointer">
            <input
              v-model="workingConfig.reverse"
              type="checkbox"
              class="h-4 w-4 rounded accent-blue-600"
            />
            <span>倒序发送 (reverse)</span>
          </label>
        </div>
      </div>

      <!-- 5. 通用摘要提取（LabelRules）与响应断言（Assertions） -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-purple-600 dark:text-purple-400">
          5. 通用 Payload 摘要提取与响应断言（可扩展兼容任意工作流）
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">摘要提取预设 (labelRules.preset)</span>
            <select
              v-model="workingConfig.labelRules.preset"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs"
            >
              <option value="auto">auto（兼顾 n8n 通知与通用 JSON）</option>
              <option value="n8n-renotify">n8n-renotify（通知转发专属）</option>
              <option value="generic">generic（严格按下方路径读取）</option>
            </select>
          </label>
          <label class="space-y-1 text-xs md:col-span-2">
            <span class="text-[var(--text-secondary)]">事件类型 JSON 路径 (typePaths)</span>
            <input
              v-model="typePathsText"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs md:col-span-3">
            <span class="text-[var(--text-secondary)]">标题摘要 JSON 路径 (titlePaths)</span>
            <input
              v-model="titlePathsText"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-3 pt-2 border-t border-[var(--border-subtle)]">
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">成功状态码 (assertions.expect)</span>
            <input
              v-model="expectCodesText"
              placeholder="200, 201"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">软跳过状态码 (softFailStatus)</span>
            <input
              v-model="softFailCodesText"
              placeholder="400"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <label class="flex items-center gap-2 pt-5 text-xs cursor-pointer">
            <input
              v-model="workingConfig.assertions.strict"
              @change="workingConfig.strict = workingConfig.assertions.strict"
              type="checkbox"
              class="h-4 w-4 rounded accent-rose-600"
            />
            <span>严格模式：软跳过状态码也判失败 (strict)</span>
          </label>
        </div>

        <!-- 响应体 JSON 规则表 -->
        <div class="space-y-2 pt-2">
          <div class="flex items-center justify-between text-xs">
            <span class="font-medium text-[var(--text-secondary)]">
              响应体 JSON 规则 (bodyRules · 如识别 deduped: true)
            </span>
            <button
              type="button"
              @click="addBodyRule"
              class="inline-flex items-center gap-1 rounded bg-purple-500/12 px-2 py-1 text-[0.6875rem] font-semibold text-purple-600 hover:bg-purple-500/20 dark:text-purple-300"
            >
              <Plus class="h-3 w-3" />
              <span>新增规则</span>
            </button>
          </div>
          <div
            v-for="(rule, idx) in workingConfig.assertions.bodyRules"
            :key="idx"
            class="grid grid-cols-12 items-center gap-2 text-xs"
          >
            <input
              v-model="rule.path"
              placeholder="JSON路径如 deduped"
              class="col-span-4 h-7 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
            />
            <input
              :value="formatBodyRuleEquals(rule.equals)"
              @input="updateBodyRuleEquals(idx, ($event.target as HTMLInputElement).value)"
              placeholder="期望值如 true"
              class="col-span-2 h-7 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
            />
            <input
              v-model="rule.badge"
              placeholder="展示徽章文案"
              class="col-span-3 h-7 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs"
            />
            <select
              v-model="rule.tone"
              class="col-span-2 h-7 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-1 text-xs"
            >
              <option value="info">info</option>
              <option value="success">success</option>
              <option value="warning">warning</option>
              <option value="error">error</option>
            </select>
            <button
              type="button"
              @click="removeBodyRule(idx)"
              class="col-span-1 flex justify-center text-[var(--text-muted)] hover:text-rose-500"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- 6. 报告输出与环境变量模板 -->
      <div class="aurora-card-interactive rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 space-y-3">
        <div class="text-xs font-bold uppercase tracking-wider text-cyan-600 dark:text-cyan-400">
          6. 报告落盘、环境变量与动态占位符
        </div>

        <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
          <label class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">JSON 报告输出路径 (report)</span>
            <input
              v-model="workingConfig.report"
              placeholder="test_reports/latest.json（留空不保存）"
              class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
            />
          </label>
          <div class="space-y-1 text-xs">
            <span class="text-[var(--text-secondary)]">逐条响应保存目录 (saveResponseDir)</span>
            <div class="flex items-center gap-1.5">
              <input
                v-model="workingConfig.saveResponseDir"
                placeholder="留空不保存逐条响应体"
                class="h-8 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs"
              />
              <button
                type="button"
                @click="browseSaveResponseDir"
                class="h-8 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs hover:bg-[var(--bg-hover)]"
              >
                浏览
              </button>
            </div>
          </div>
        </div>

        <div class="pt-2 space-y-2 border-t border-[var(--border-subtle)]">
          <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
            <span class="font-medium text-[var(--text-secondary)]">
              模板环境变量 (variables · 在 URL / Header / Body 中使用 <code v-pre class="font-mono">{{VAR}}</code>)
            </span>
            <span class="inline-flex items-center gap-1 text-[0.6875rem] text-[var(--text-muted)]">
              <Info class="h-3.5 w-3.5" />
              内置占位符：<code v-pre class="font-mono">{{$timestamp}}</code> / <code v-pre class="font-mono">{{$isoTime}}</code> / <code v-pre class="font-mono">{{$uuid}}</code>
            </span>
          </div>

          <div
            v-for="(_val, key) in workingConfig.variables"
            :key="key"
            class="flex items-center gap-2 text-xs"
          >
            <span class="w-40 truncate font-mono text-[var(--text-primary)]">{{ key }}</span>
            <input
              v-model="workingConfig.variables[key]"
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="removeVariable(String(key))"
              class="text-[var(--text-muted)] hover:text-rose-500"
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>

          <div class="flex items-center gap-2">
            <input
              v-model="newVarKey"
              placeholder="变量名，如 HOST"
              class="h-7 w-40 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <input
              v-model="newVarVal"
              placeholder="变量值，如 127.0.0.1:5678"
              class="h-7 flex-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs outline-none"
            />
            <button
              type="button"
              @click="addVariable"
              class="rounded-md bg-[var(--bg-elevated)] px-2.5 py-1 text-xs font-medium hover:bg-[var(--bg-hover)]"
            >
              添加变量
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 模式 B：YAML 源码直接编辑 -->
    <div v-else class="flex min-h-0 flex-1 flex-col p-4 space-y-2.5">
      <div class="flex items-center justify-between text-xs text-[var(--text-secondary)]">
        <span>
          正在编辑：
          <code class="font-mono text-[var(--text-primary)]">
            {{ snapshot?.yamlPath ?? "新建 YAML 草稿（保存时选择存放位置）" }}
          </code>
        </span>
        <button
          type="button"
          :disabled="isBusy"
          @click="handleSaveRawYaml"
          class="inline-flex items-center gap-1 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-indigo-500 disabled:opacity-45"
        >
          <FileCheck class="h-3.5 w-3.5" />
          <span>校验并保存 YAML 源码</span>
        </button>
      </div>

      <textarea
        v-model="rawYamlDraft"
        spellcheck="false"
        placeholder="# 在此输入或编辑 YAML 配置..."
        class="code-editor-text min-h-0 flex-1 resize-none rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-4 text-[var(--text-primary)] outline-none focus:border-indigo-500"
      />
    </div>

    <!-- 底部持久化双模式动作条 -->
    <div
      class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--border-subtle)] bg-[var(--bg-surface)] px-4 py-2.5"
    >
      <button
        type="button"
        @click="scanSources(workingConfig); leftTab = 'sources'"
        class="aurora-btn-subtle inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
      >
        <CheckCircle2 class="h-3.5 w-3.5 text-emerald-500" />
        <span>应用到当前会话并查看数据源</span>
      </button>

      <div v-if="editorMode === 'form'" class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          :disabled="isBusy"
          @click="saveAsGuiConfig(workingConfig)"
          class="aurora-btn-subtle inline-flex items-center gap-1.5 rounded-lg border border-cyan-500/40 bg-cyan-500/12 px-3 py-1.5 text-xs font-semibold text-cyan-700 hover:bg-cyan-500/20 dark:text-cyan-300 disabled:opacity-45"
        >
          <Save class="h-3.5 w-3.5" />
          <span>保存为 GUI 默认配置</span>
        </button>

        <button
          type="button"
          :disabled="isBusy"
          @click="handleWriteYaml(false)"
          class="aurora-btn inline-flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-semibold text-white disabled:opacity-45"
        >
          <FileCheck class="h-3.5 w-3.5" />
          <span>{{ snapshot?.yamlPath ? "同步写入当前 YAML" : "保存为 YAML 档案" }}</span>
        </button>

        <button
          v-if="snapshot?.yamlPath"
          type="button"
          :disabled="isBusy"
          @click="handleWriteYaml(true)"
          class="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] px-2.5 py-1.5 text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]"
        >
          <span>另存 YAML…</span>
        </button>
      </div>
    </div>
  </section>
</template>
