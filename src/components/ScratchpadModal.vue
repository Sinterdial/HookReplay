<script setup lang="ts">
/**
 * 临时粘贴调试台 + JSON 高亮预览 + 分类持久化归档模态框（ScratchpadModal）
 *
 * 功能：
 * 1. 左栏支持粘贴单条/美化 JSON、JSON 数组或带 `#` 注释 JSONL，提供格式化/压缩/剪贴板粘贴；
 * 2. 右栏实时解析并在 `JsonViewer` 中以语法高亮 + 关键词突显预览，多条时支持胶囊切换，错误行精准提示；
 * 3. 底部可展开持久化设置：指定测试文件或测试文件夹（支持一键选用当前配置输入源）、自动读取已有 `#` 分类并支持归类插入到指定分段下方。
 */
import { computed, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  Braces,
  Clipboard,
  ClipboardPaste,
  FolderOpen,
  Minimize2,
  Save,
  Sparkles,
  Tag,
  Trash2,
  X,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import JsonViewer from "./JsonViewer.vue";

const {
  showScratchpadModal,
  scratchpadDraft,
  workingConfig,
  scanResult,
  loadScratchpadText,
  inspectLogCategories,
  listDirLogFiles,
  persistScratchpadMessages,
  pushToast,
} = useAppState();

interface ParsedPreviewItem {
  index: number;
  category: string;
  label: string;
  formatted: string;
  line: number;
}

interface ParsedPreviewError {
  line: number;
  error: string;
}

const activePreviewIdx = ref(0);
const showPersistPanel = ref(false);

// 持久化参数状态
const targetMode = ref<"file" | "directory">("file");
const targetPath = ref("");
const newFileName = ref("");
const category = ref("");
const commentPrefix = ref("#");
const insertIntoExistingCategory = ref(true);
const addToInputsAndScan = ref(true);
const isPersisting = ref(false);

const existingCategories = ref<string[]>([]);
const dirExistingFiles = ref<string[]>([]);

/** 实时解析左栏粘贴内容用于右栏高亮预览 */
const parsedPreview = computed<{
  items: ParsedPreviewItem[];
  errors: ParsedPreviewError[];
}>(() => {
  const raw = scratchpadDraft.value.replace(/^\uFEFF/, "").trim();
  if (!raw) return { items: [], errors: [] };

  // 尝试作为完整 JSON 对象或 JSON 数组解析
  if (raw.startsWith("{") || raw.startsWith("[")) {
    try {
      const val = JSON.parse(raw);
      if (Array.isArray(val)) {
        const items: ParsedPreviewItem[] = val.map((el, i) => ({
          index: i + 1,
          category: "JSON 数组",
          label: extractQuickLabel(el, i + 1),
          formatted: JSON.stringify(el, null, 2),
          line: i + 1,
        }));
        return { items, errors: [] };
      }
      return {
        items: [
          {
            index: 1,
            category: "单条 JSON",
            label: extractQuickLabel(val, 1),
            formatted: JSON.stringify(val, null, 2),
            line: 1,
          },
        ],
        errors: [],
      };
    } catch {
      // 继续尝试按多行 JSONL 解析
    }
  }

  const items: ParsedPreviewItem[] = [];
  const errors: ParsedPreviewError[] = [];
  const prefixes =
    workingConfig.value.commentPrefixes.length > 0
      ? workingConfig.value.commentPrefixes
      : ["#", "//"];

  let curCategory = "临时调试";
  let lastWasComment = false;
  const lines = raw.split(/\r?\n/);

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (!trimmed) {
      lastWasComment = false;
      continue;
    }
    const matchedPrefix = prefixes.find((p) => p && trimmed.startsWith(p));
    if (matchedPrefix) {
      const text = trimmed.slice(matchedPrefix.length).trim();
      if (!lastWasComment) {
        curCategory = text || "未命名分类";
      } else if (text) {
        curCategory = curCategory ? `${curCategory} / ${text}` : text;
      }
      lastWasComment = true;
      continue;
    }
    lastWasComment = false;
    try {
      const val = JSON.parse(trimmed);
      const idx = items.length + 1;
      items.push({
        index: idx,
        category: curCategory,
        label: extractQuickLabel(val, idx),
        formatted: JSON.stringify(val, null, 2),
        line: i + 1,
      });
    } catch (err) {
      errors.push({ line: i + 1, error: String(err) });
    }
  }

  return { items, errors };
});

function extractQuickLabel(val: unknown, idx: number): string {
  if (val && typeof val === "object" && !Array.isArray(val)) {
    const obj = val as Record<string, unknown>;
    const type =
      (typeof obj.type === "string" && obj.type) ||
      (typeof obj.event_type === "string" && obj.event_type) ||
      (typeof obj.event === "string" && obj.event) ||
      (typeof obj.action === "string" && obj.action) ||
      "";
    const title =
      (typeof obj.title === "string" && obj.title) ||
      (typeof obj.name === "string" && obj.name) ||
      "";
    if (type && title) return `#${idx} ${type} · ${title}`;
    if (type) return `#${idx} ${type}`;
    if (title) return `#${idx} ${title}`;
  }
  return `#${idx}`;
}

watch(
  () => parsedPreview.value.items.length,
  (len) => {
    if (activePreviewIdx.value >= len) {
      activePreviewIdx.value = 0;
    }
  }
);

const currentPreviewItem = computed<ParsedPreviewItem | null>(
  () => parsedPreview.value.items[activePreviewIdx.value] ?? null
);

/** 打开弹窗时默认预填已有输入源 */
watch(showScratchpadModal, (opened) => {
  if (opened) {
    if (!targetPath.value) {
      const firstFile = scanResult.value.files[0] ?? workingConfig.value.inputs[0] ?? "";
      if (firstFile) {
        targetPath.value = firstFile;
      }
    }
    void refreshTargetMetadata();
  }
});

/** 计算用于检查已有分类的完整目标文件路径 */
const effectiveInspectionFile = computed<string>(() => {
  const base = targetPath.value.trim();
  if (!base) return "";
  if (targetMode.value === "file") return base;
  const fname = newFileName.value.trim();
  if (!fname) return "";
  const sep = base.includes("\\") ? "\\" : "/";
  const cleanBase = base.replace(/[\\/]+$/, "");
  return `${cleanBase}${sep}${fname}`;
});

async function refreshTargetMetadata(): Promise<void> {
  if (targetMode.value === "directory" && targetPath.value.trim()) {
    dirExistingFiles.value = await listDirLogFiles(targetPath.value);
  } else {
    dirExistingFiles.value = [];
  }

  const fileToInspect = effectiveInspectionFile.value;
  if (fileToInspect) {
    existingCategories.value = await inspectLogCategories(fileToInspect);
  } else {
    existingCategories.value = [];
  }
}

watch([targetMode, targetPath, newFileName], () => {
  if (showScratchpadModal.value && showPersistPanel.value) {
    void refreshTargetMetadata();
  }
});

watch(showPersistPanel, (visible) => {
  if (visible) {
    void refreshTargetMetadata();
  }
});

/** 格式化左栏内容 */
function formatDraft(): void {
  const raw = scratchpadDraft.value.trim();
  if (!raw) return;
  try {
    scratchpadDraft.value = JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    pushToast("仅单条 JSON 或 JSON 数组支持整块美化；JSONL 请保持每行一条", "info");
  }
}

/** 压缩左栏内容 */
function minifyDraft(): void {
  const raw = scratchpadDraft.value.trim();
  if (!raw) return;
  try {
    scratchpadDraft.value = JSON.stringify(JSON.parse(raw));
  } catch {
    if (parsedPreview.value.items.length > 0) {
      scratchpadDraft.value = parsedPreview.value.items
        .map((it) => JSON.stringify(JSON.parse(it.formatted)))
        .join("\n");
    } else {
      pushToast("当前内容不是合法的 JSON", "warning");
    }
  }
}

async function pasteFromClipboard(): Promise<void> {
  try {
    const txt = await navigator.clipboard.readText();
    if (txt) {
      scratchpadDraft.value = txt;
    }
  } catch {
    pushToast("无法读取系统剪贴板，请直接使用 Ctrl+V 粘贴", "warning");
  }
}

async function browseTarget(): Promise<void> {
  if (targetMode.value === "directory") {
    const picked = await open({
      title: "选择测试消息目标文件夹",
      directory: true,
      multiple: false,
    });
    if (typeof picked === "string" && picked.trim()) {
      targetPath.value = picked;
    }
  } else {
    const picked = await open({
      title: "选择要追加写入的测试文件",
      multiple: false,
      filters: [
        { name: "测试日志 / JSONL / 文本", extensions: ["txt", "log", "json", "jsonl"] },
        { name: "所有文件", extensions: ["*"] },
      ],
    });
    if (typeof picked === "string" && picked.trim()) {
      targetPath.value = picked;
    }
  }
}

function selectQuickInput(inp: string): void {
  // 根据路径末段是否有扩展名智能判断文件或目录
  const trimmed = inp.trim();
  const lastSeg = trimmed.split(/[\\/]/).pop() ?? "";
  if (lastSeg.includes(".")) {
    targetMode.value = "file";
    targetPath.value = trimmed;
  } else {
    targetMode.value = "directory";
    targetPath.value = trimmed;
  }
}

function pickDirFile(filePath: string): void {
  const name = filePath.split(/[\\/]/).pop() ?? "";
  if (name) {
    newFileName.value = name;
  }
}

async function submitLoadOnly(): Promise<void> {
  await loadScratchpadText(scratchpadDraft.value);
}

async function submitPersist(): Promise<void> {
  if (!showPersistPanel.value) {
    showPersistPanel.value = true;
    return;
  }
  if (!targetPath.value.trim()) {
    pushToast("请先指定保存的目标文件或文件夹路径", "warning");
    return;
  }
  isPersisting.value = true;
  try {
    await persistScratchpadMessages(
      {
        rawText: scratchpadDraft.value,
        targetMode: targetMode.value,
        targetPath: targetPath.value.trim(),
        newFileName: targetMode.value === "directory" ? newFileName.value.trim() || null : null,
        category: category.value.trim(),
        insertIntoExistingCategory: insertIntoExistingCategory.value,
        commentPrefix: commentPrefix.value,
      },
      addToInputsAndScan.value
    );
  } finally {
    isPersisting.value = false;
  }
}
</script>

<template>
  <div
    v-if="showScratchpadModal"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4 backdrop-blur-xs"
    @click.self="showScratchpadModal = false"
  >
    <div
      class="aurora-border-active flex max-h-[92vh] w-full max-w-6xl flex-col overflow-hidden rounded-2xl bg-[var(--bg-surface)] shadow-2xl"
    >
      <!-- 标题栏 -->
      <div
        class="flex items-center justify-between border-b border-[var(--border-subtle)] px-5 py-3"
      >
        <div class="flex items-center gap-2.5">
          <div
            class="aurora-ring-soft flex h-8 w-8 items-center justify-center rounded-lg bg-[var(--bg-elevated)] text-blue-500"
          >
            <ClipboardPaste class="h-4 w-4" />
          </div>
          <div>
            <h3 class="text-sm font-bold text-[var(--text-primary)]">
              临时粘贴调试台 · JSON 格式化高亮预览与分类归档
            </h3>
            <p class="text-[0.6875rem] text-[var(--text-muted)]">
              左侧粘贴单条 JSON / 数组 / 带 # 注释 JSONL，右侧实时语法高亮预览，底部支持按分类持久化写入测试文件
            </p>
          </div>
        </div>

        <button
          type="button"
          @click="showScratchpadModal = false"
          class="rounded-md p-1 text-[var(--text-muted)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- 主内容左右双栏 -->
      <div class="grid min-h-0 flex-1 grid-cols-1 divide-y divide-[var(--border-subtle)] lg:grid-cols-2 lg:divide-x lg:divide-y-0">
        <!-- 左栏：原始粘贴输入 -->
        <div class="flex min-h-[260px] flex-col p-3.5 space-y-2">
          <div class="flex flex-wrap items-center justify-between gap-1.5">
            <span class="text-xs font-semibold text-[var(--text-secondary)]">
              原始消息输入区
            </span>
            <div class="flex items-center gap-1">
              <button
                type="button"
                @click="pasteFromClipboard"
                class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
              >
                <Clipboard class="h-3 w-3" />
                <span>粘贴</span>
              </button>
              <button
                type="button"
                @click="formatDraft"
                class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
              >
                <Braces class="h-3 w-3" />
                <span>格式化</span>
              </button>
              <button
                type="button"
                @click="minifyDraft"
                class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
              >
                <Minimize2 class="h-3 w-3" />
                <span>压缩</span>
              </button>
              <button
                type="button"
                @click="scratchpadDraft = ''"
                class="inline-flex items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem] text-[var(--text-muted)] hover:text-rose-500"
              >
                <Trash2 class="h-3 w-3" />
                <span>清空</span>
              </button>
            </div>
          </div>

          <textarea
            v-model="scratchpadDraft"
            spellcheck="false"
            placeholder='# 示例分类&#10;{"type":"webhook.event","title":"示例通知消息 #1024","status":"ok"}'
            class="code-editor-text min-h-0 flex-1 resize-none rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/70 p-3 text-[var(--text-primary)] outline-none focus:border-blue-500"
          />
        </div>

        <!-- 右栏：JSON 格式化高亮实时预览 -->
        <div class="flex min-h-[260px] flex-col p-3.5 space-y-2">
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-secondary)]">
              <Sparkles class="h-3.5 w-3.5 text-purple-500" />
              <span>
                JSON 格式化高亮预览（{{ parsedPreview.items.length }} 条有效消息）
              </span>
            </div>
            <span
              v-if="currentPreviewItem"
              class="rounded-full bg-indigo-500/12 px-2 py-0.5 text-[0.625rem] font-medium text-indigo-600 dark:text-indigo-300"
            >
              分类：{{ currentPreviewItem.category }}
            </span>
          </div>

          <!-- 解析报错行提醒 -->
          <div
            v-if="parsedPreview.errors.length > 0"
            class="flex items-start gap-1.5 rounded-lg border border-rose-500/35 bg-rose-500/10 px-2.5 py-1.5 text-[0.6875rem] text-rose-700 dark:text-rose-300"
          >
            <AlertCircle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>
              第 {{ parsedPreview.errors[0].line }} 行 JSON 语法错误：{{
                parsedPreview.errors[0].error
              }}
              <template v-if="parsedPreview.errors.length > 1">
                （另有 {{ parsedPreview.errors.length - 1 }} 行错误）
              </template>
            </span>
          </div>

          <!-- 多条条目切换胶囊 -->
          <div
            v-if="parsedPreview.items.length > 1"
            class="flex max-h-16 flex-wrap gap-1 overflow-y-auto"
          >
            <button
              v-for="(item, idx) in parsedPreview.items"
              :key="item.index"
              type="button"
              @click="activePreviewIdx = idx"
              class="aurora-pill rounded-full px-2 py-0.5 text-[0.625rem] font-medium transition"
              :class="
                activePreviewIdx === idx
                  ? 'bg-blue-600 text-white'
                  : 'bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
              "
            >
              {{ item.label }}
            </button>
          </div>

          <JsonViewer
            :code="currentPreviewItem?.formatted ?? ''"
            empty-text="在左侧粘贴 JSON、数组或 JSONL 后，此处实时展示格式化语法高亮"
            class="min-h-0 flex-1"
          />
        </div>
      </div>

      <!-- 底部持久化存储与分类设置折叠区 -->
      <div class="border-t border-[var(--border-subtle)] bg-[var(--bg-elevated)]/45 px-5 py-3 space-y-3">
        <label class="flex cursor-pointer items-center justify-between text-xs font-semibold text-[var(--text-primary)]">
          <div class="flex items-center gap-2">
            <input
              v-model="showPersistPanel"
              type="checkbox"
              class="h-4 w-4 rounded accent-indigo-600"
            />
            <Save class="h-3.5 w-3.5 text-indigo-500" />
            <span>持久化保存到指定测试文件 / 文件夹（并设置 # 注释分类）</span>
          </div>
          <span class="text-[0.6875rem] font-normal text-[var(--text-muted)]">
            可将美化 JSON 自动整理成规范 JSONL 单行归档
          </span>
        </label>

        <div v-if="showPersistPanel" class="space-y-3 rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-3.5 text-xs">
          <!-- 目标模式 + 快速选用已配置输入源 -->
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              <span class="text-[var(--text-secondary)]">保存目标类型：</span>
              <button
                type="button"
                @click="targetMode = 'file'"
                class="rounded-lg border px-2.5 py-1 text-[0.6875rem] font-medium transition"
                :class="
                  targetMode === 'file'
                    ? 'border-blue-500 bg-blue-500/12 text-blue-700 dark:text-blue-300'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]'
                "
              >
                追加到指定测试文件
              </button>
              <button
                type="button"
                @click="targetMode = 'directory'"
                class="rounded-lg border px-2.5 py-1 text-[0.6875rem] font-medium transition"
                :class="
                  targetMode === 'directory'
                    ? 'border-blue-500 bg-blue-500/12 text-blue-700 dark:text-blue-300'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)]'
                "
              >
                保存到指定测试文件夹
              </button>
            </div>

            <div
              v-if="workingConfig.inputs.length > 0"
              class="flex flex-wrap items-center gap-1"
            >
              <span class="text-[0.6875rem] text-[var(--text-muted)]">快速选用配置源：</span>
              <button
                v-for="inp in workingConfig.inputs.slice(0, 4)"
                :key="inp"
                type="button"
                @click="selectQuickInput(inp)"
                class="max-w-[180px] truncate rounded bg-[var(--bg-elevated)] px-2 py-0.5 font-mono text-[0.625rem] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
                :title="inp"
              >
                {{ inp }}
              </button>
            </div>
          </div>

          <!-- 路径选择行 -->
          <div class="grid grid-cols-1 gap-2.5 md:grid-cols-12">
            <div :class="targetMode === 'directory' ? 'md:col-span-8' : 'md:col-span-12'" class="space-y-1">
              <span class="text-[0.6875rem] text-[var(--text-secondary)]">
                {{ targetMode === "directory" ? "目标测试文件夹路径" : "目标测试文件路径" }}
              </span>
              <div class="flex items-center gap-1.5">
                <input
                  v-model="targetPath"
                  type="text"
                  :placeholder="
                    targetMode === 'directory'
                      ? '例如：./logs 或 ./messages'
                      : '例如：./logs/sample.jsonl'
                  "
                  class="h-8 flex-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none focus:border-blue-500"
                />
                <button
                  type="button"
                  @click="browseTarget"
                  class="inline-flex h-8 items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs hover:bg-[var(--bg-hover)]"
                >
                  <FolderOpen class="h-3.5 w-3.5" />
                  <span>浏览</span>
                </button>
              </div>
            </div>

            <div v-if="targetMode === 'directory'" class="md:col-span-4 space-y-1">
              <span class="text-[0.6875rem] text-[var(--text-secondary)]">
                目录内文件名（留空按分类命名）
              </span>
              <input
                v-model="newFileName"
                type="text"
                placeholder="例如：custom_cases.txt"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 font-mono text-xs outline-none focus:border-blue-500"
              />
            </div>
          </div>

          <!-- 目录下已有文件快选 -->
          <div
            v-if="targetMode === 'directory' && dirExistingFiles.length > 0"
            class="flex flex-wrap items-center gap-1 text-[0.625rem]"
          >
            <span class="text-[var(--text-muted)]">目录内已有文件（点击选中）：</span>
            <button
              v-for="fp in dirExistingFiles"
              :key="fp"
              type="button"
              @click="pickDirFile(fp)"
              class="rounded bg-[var(--bg-elevated)] px-2 py-0.5 font-mono text-[var(--text-secondary)] hover:bg-blue-500/15 hover:text-blue-600 dark:hover:text-blue-300"
            >
              {{ fp.split(/[\\/]/).pop() }}
            </button>
          </div>

          <!-- 分类设置 -->
          <div class="grid grid-cols-1 gap-2.5 md:grid-cols-12">
            <div class="md:col-span-2 space-y-1">
              <span class="text-[0.6875rem] text-[var(--text-secondary)]">注释前缀</span>
              <select
                v-model="commentPrefix"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 font-mono text-xs"
              >
                <option value="#">#</option>
                <option value="//">//</option>
              </select>
            </div>

            <div class="md:col-span-10 space-y-1">
              <div class="flex items-center gap-1.5 text-[0.6875rem] text-[var(--text-secondary)]">
                <Tag class="h-3 w-3 text-emerald-500" />
                <span>分类名称（将写入为 <code class="font-mono">{{ commentPrefix }} 分类名</code> 分段标签，留空不写分类头）</span>
              </div>
              <input
                v-model="category"
                type="text"
                placeholder="例如：RSS消息、MoviePilot通知、边界回归用例"
                class="h-8 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs outline-none focus:border-emerald-500"
              />
            </div>
          </div>

          <!-- 目标文件已有分类快选 -->
          <div
            v-if="existingCategories.length > 0"
            class="flex flex-wrap items-center gap-1 text-[0.625rem]"
          >
            <span class="text-[var(--text-muted)]">目标文件已有分类（点击复用）：</span>
            <button
              v-for="cat in existingCategories"
              :key="cat"
              type="button"
              @click="category = cat"
              class="rounded-full px-2 py-0.5 font-medium transition"
              :class="
                category === cat
                  ? 'bg-emerald-600 text-white'
                  : 'bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
              "
            >
              {{ cat }}
            </button>
          </div>

          <div class="flex flex-wrap items-center gap-5 pt-0.5 text-[0.6875rem] text-[var(--text-secondary)]">
            <label class="flex cursor-pointer items-center gap-1.5">
              <input
                v-model="insertIntoExistingCategory"
                type="checkbox"
                class="h-3.5 w-3.5 rounded accent-emerald-600"
              />
              <span>若目标文件已存在该分类，直接归类插入到该分段下方</span>
            </label>

            <label class="flex cursor-pointer items-center gap-1.5">
              <input
                v-model="addToInputsAndScan"
                type="checkbox"
                class="h-3.5 w-3.5 rounded accent-blue-600"
              />
              <span>保存后自动纳入输入日志源并刷新工作区</span>
            </label>
          </div>
        </div>
      </div>

      <!-- 底部操作栏 -->
      <div class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--border-subtle)] px-5 py-3">
        <button
          type="button"
          @click="showScratchpadModal = false"
          class="rounded-lg border border-[var(--border-subtle)] px-3.5 py-1.5 text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]"
        >
          取消
        </button>

        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            :disabled="parsedPreview.items.length === 0"
            @click="submitLoadOnly"
            class="aurora-btn-subtle rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3.5 py-1.5 text-xs font-semibold text-[var(--text-primary)] disabled:opacity-45"
          >
            仅加载到临时工作区调试
          </button>

          <button
            type="button"
            :disabled="parsedPreview.items.length === 0 || isPersisting"
            @click="submitPersist"
            class="aurora-btn inline-flex items-center gap-1.5 rounded-lg px-4 py-1.5 text-xs font-semibold text-white disabled:opacity-45"
          >
            <Save class="h-3.5 w-3.5" />
            <span>
              {{
                !showPersistPanel
                  ? "保存到测试文件…"
                  : isPersisting
                    ? "正在写入…"
                    : "持久化保存并加载"
              }}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
