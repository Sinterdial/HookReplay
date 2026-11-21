<script setup lang="ts">
/**
 * 左侧数据源目录、搜索筛选与多分段勾选导航栏（SourceSidebar）
 *
 * 核心功能：
 * 1. 三叶签切换：「数据源」|「配置中心」|「历史报告」；
 * 2. 原生对话框追加输入文件/文件夹，支持删除单个输入源并即时重扫；
 * 3. 支持弹出「临时粘贴 JSON / JSONL」Scratchpad 调试台；
 * 4. 搜索关键词、按事件类型胶囊筛选、仅看异常切换与批量勾选；
 * 5. # 注释分段目录树，支持整段勾选与点击平滑跳转到中间列表对应分段。
 */
import { computed } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ClipboardPaste,
  FilePlus2,
  FolderPlus,
  History,
  Layers,
  PanelLeftClose,
  Pin,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Trash2,
  X,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useWorkspaceLayout } from "../composables/useWorkspaceLayout";
import type { SegmentSummary } from "../types";

const emit = defineEmits<{
  (e: "jump-segment", segmentId: number): void;
}>();

const {
  snapshot,
  leftTab,
  workingConfig,
  scanResult,
  isScanning,
  isScratchpadMode,
  showScratchpadModal,
  selectedIndices,
  searchQuery,
  selectedEventType,
  onlyAbnormal,
  resultsByIndex,
  runStats,
  isBusy,
  scanSources,
  selectYamlProfile,
  mountYamlProfiles,
  toggleSegmentSelection,
  batchSelectFiltered,
  selectAbnormalFromReport,
} = useAppState();

const { leftFloating, toggleFloating } = useWorkspaceLayout();

/** 在侧边栏快速选择并挂载多个 YAML 配置 */
async function quickMountSidebarProfiles(): Promise<void> {
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

/** 计算每个分段当前的勾选情况与执行小统计 */
interface SegmentCheckMeta {
  total: number;
  selectedCount: number;
  allChecked: boolean;
  indeterminate: boolean;
  ok: number;
  unmatched: number;
  failed: number;
}

const segmentStatsMap = computed<Map<number, SegmentCheckMeta>>(() => {
  const map = new Map<number, SegmentCheckMeta>();
  for (const entry of scanResult.value.entries) {
    let meta = map.get(entry.segment);
    if (!meta) {
      meta = {
        total: 0,
        selectedCount: 0,
        allChecked: false,
        indeterminate: false,
        ok: 0,
        unmatched: 0,
        failed: 0,
      };
      map.set(entry.segment, meta);
    }
    meta.total += 1;
    if (selectedIndices.value.has(entry.index)) {
      meta.selectedCount += 1;
    }
    const res = resultsByIndex.get(entry.index);
    if (res) {
      if (res.ok) meta.ok += 1;
      else if (res.unmatched) meta.unmatched += 1;
      else meta.failed += 1;
    }
  }
  for (const meta of map.values()) {
    meta.allChecked = meta.total > 0 && meta.selectedCount === meta.total;
    meta.indeterminate =
      meta.selectedCount > 0 && meta.selectedCount < meta.total;
  }
  return map;
});

/** 调用原生对话框添加日志文件 */
async function addInputFiles(): Promise<void> {
  const picked = await open({
    title: "选择通知日志文件（可多选）",
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
  await scanSources(workingConfig.value);
}

/** 调用原生对话框添加日志目录 */
async function addInputDirectory(): Promise<void> {
  const picked = await open({
    title: "选择包含日志文件的文件夹",
    directory: true,
    multiple: false,
  });
  if (typeof picked === "string" && picked.trim()) {
    if (!workingConfig.value.inputs.includes(picked)) {
      workingConfig.value.inputs.push(picked);
    }
    await scanSources(workingConfig.value);
  }
}

/** 移除某条输入源路径并重扫 */
async function removeInputPath(idx: number): Promise<void> {
  workingConfig.value.inputs.splice(idx, 1);
  await scanSources(workingConfig.value);
}

/** 切换某个分段的全选复选框 */
function onToggleSeg(seg: SegmentSummary, ev: Event): void {
  const checked = (ev.target as HTMLInputElement).checked;
  toggleSegmentSelection(seg.segment, checked);
}
</script>

<template>
  <aside
    class="flex h-full w-full min-w-0 flex-1 flex-col bg-[var(--bg-surface)]"
    :class="leftFloating ? '' : 'border-r border-[var(--border-subtle)]'"
  >
    <!-- 顶部三模块导航页签 + 悬浮/固定切换 -->
    <nav class="flex items-center gap-1 border-b border-[var(--border-subtle)] p-1.5">
      <div class="grid min-w-0 flex-1 grid-cols-3 gap-1">
        <button
          type="button"
          @click="leftTab = 'sources'"
          class="flex min-w-0 items-center justify-center gap-1 rounded-lg px-1 py-1.5 text-xs font-semibold transition"
          :class="
            leftTab === 'sources'
              ? 'aurora-tab-active bg-blue-600/12 text-blue-600 dark:bg-blue-500/20 dark:text-blue-300'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]'
          "
        >
          <Layers class="h-3.5 w-3.5 shrink-0" />
          <span class="truncate">数据源</span>
        </button>

        <button
          type="button"
          @click="leftTab = 'config'"
          class="flex min-w-0 items-center justify-center gap-1 rounded-lg px-1 py-1.5 text-xs font-semibold transition"
          :class="
            leftTab === 'config'
              ? 'aurora-tab-active bg-indigo-600/12 text-indigo-600 dark:bg-indigo-500/20 dark:text-indigo-300'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]'
          "
        >
          <Settings2 class="h-3.5 w-3.5 shrink-0" />
          <span class="truncate">配置中心</span>
        </button>

        <button
          type="button"
          @click="leftTab = 'reports'"
          class="flex min-w-0 items-center justify-center gap-1 rounded-lg px-1 py-1.5 text-xs font-semibold transition"
          :class="
            leftTab === 'reports'
              ? 'aurora-tab-active bg-emerald-600/12 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-300'
              : 'text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]'
          "
        >
          <History class="h-3.5 w-3.5 shrink-0" />
          <span class="truncate">历史报告</span>
        </button>
      </div>

      <button
        type="button"
        @click="toggleFloating('left')"
        :title="
          leftFloating
            ? '固定为常驻分栏（Ctrl+\\）'
            : '转为悬浮侧栏（鼠标移到左边缘自动展开 · Ctrl+\\）'
        "
        class="aurora-btn-subtle flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border transition"
        :class="
          leftFloating
            ? 'border-indigo-500/45 bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
            : 'border-transparent text-[var(--text-muted)] hover:border-[var(--border-subtle)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]'
        "
      >
        <Pin v-if="leftFloating" class="h-3.5 w-3.5" />
        <PanelLeftClose v-else class="h-3.5 w-3.5" />
      </button>
    </nav>

    <!-- 页签 1：数据源与分段目录 -->
    <div v-if="leftTab === 'sources'" class="flex min-h-0 flex-1 flex-col">
      <!-- 输入源工具按钮区 -->
      <div class="border-b border-[var(--border-subtle)] p-2.5 space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-[0.6875rem] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
            输入日志源 ({{ workingConfig.inputs.length }})
          </span>
          <div class="flex items-center gap-1">
            <button
              type="button"
              :disabled="isBusy"
              @click="addInputFiles"
              title="添加日志文件"
              class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            >
              <FilePlus2 class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              :disabled="isBusy"
              @click="addInputDirectory"
              title="添加日志目录"
              class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            >
              <FolderPlus class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              :disabled="isScanning || isBusy"
              @click="scanSources()"
              title="重新扫描数据源并更新索引"
              class="rounded p-1 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)] disabled:opacity-40"
            >
              <RefreshCw class="h-3.5 w-3.5" :class="{ 'animate-spin': isScanning }" />
            </button>
          </div>
        </div>

        <!-- 临时调试台按钮 -->
        <button
          type="button"
          :disabled="isBusy"
          @click="showScratchpadModal = true"
          class="aurora-btn-subtle flex w-full items-center justify-center gap-1.5 rounded-lg border border-dashed border-[var(--border-strong)] bg-[var(--bg-elevated)]/60 py-1.5 text-xs font-medium text-[var(--text-secondary)] transition hover:border-blue-500/60 hover:text-blue-600 dark:hover:text-blue-300 disabled:opacity-45"
        >
          <ClipboardPaste class="h-3.5 w-3.5" />
          <span>临时粘贴 JSON / JSONL 调试</span>
        </button>

        <!-- 当前为临时粘贴模式时提示切回磁盘源 -->
        <div
          v-if="isScratchpadMode"
          class="flex items-center justify-between rounded-lg bg-blue-500/12 px-2.5 py-1.5 text-[0.6875rem] text-blue-700 dark:text-blue-300"
        >
          <span>当前正在调试临时粘贴内容</span>
          <button
            type="button"
            @click="scanSources()"
            class="font-semibold underline hover:opacity-80"
          >
            切回磁盘文件
          </button>
        </div>

        <!-- 配置输入路径小列表 -->
        <div
          v-else-if="workingConfig.inputs.length > 0"
          class="max-h-24 space-y-1 overflow-y-auto pr-0.5"
        >
          <div
            v-for="(inp, idx) in workingConfig.inputs"
            :key="inp + idx"
            class="group flex items-center justify-between gap-1.5 rounded-md bg-[var(--bg-elevated)] px-2 py-1 text-[0.6875rem]"
          >
            <span class="truncate font-mono text-[var(--text-secondary)]" :title="inp">
              {{ inp }}
            </span>
            <button
              type="button"
              :disabled="isBusy"
              @click="removeInputPath(idx)"
              title="移除此输入源"
              class="shrink-0 opacity-60 hover:text-rose-500 hover:opacity-100"
            >
              <Trash2 class="h-3 w-3" />
            </button>
          </div>
        </div>

        <p v-else class="text-[0.6875rem] text-[var(--text-muted)]">
          尚未配置数据源，请点击右上角按钮添加日志或粘贴临时 JSON。
        </p>

        <!-- 缺失路径或解析失败行提示 -->
        <div
          v-if="scanResult.missingPaths.length > 0"
          class="flex items-start gap-1.5 rounded-lg bg-amber-500/12 p-2 text-[0.6875rem] text-amber-700 dark:text-amber-300"
        >
          <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>存在 {{ scanResult.missingPaths.length }} 个无效路径，请核对相对路径基准或重新选择文件。</span>
        </div>

        <div
          v-if="scanResult.parseErrors.length > 0"
          class="rounded-lg bg-rose-500/10 px-2 py-1 text-[0.6875rem] text-rose-600 dark:text-rose-300"
        >
          已自动跳过 {{ scanResult.parseErrors.length }} 行格式错误的日志
        </div>
      </div>

      <!-- 搜索、类型筛选与批量勾选 -->
      <div class="border-b border-[var(--border-subtle)] p-2.5 space-y-2">
        <div class="aurora-focus relative flex items-center rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] transition-all">
          <Search class="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[var(--text-muted)]" />
          <input
            v-model="searchQuery"
            type="text"
            placeholder="过滤标题、注释、事件类型…"
            class="h-7 w-full bg-transparent pl-8 pr-7 text-xs text-[var(--text-primary)] outline-none"
          />
          <button
            v-if="searchQuery"
            type="button"
            @click="searchQuery = ''"
            class="absolute right-2 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
          >
            <X class="h-3 w-3" />
          </button>
        </div>

        <!-- 类型筛选胶囊 -->
        <div class="flex flex-wrap gap-1 max-h-20 overflow-y-auto">
          <button
            type="button"
            @click="selectedEventType = ''"
            class="aurora-pill rounded-full px-2 py-0.5 text-[0.625rem] font-medium transition"
            :class="
              selectedEventType === ''
                ? 'bg-blue-600 text-white'
                : 'bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
            "
          >
            全部 ({{ scanResult.entries.length }})
          </button>
          <button
            v-for="t in scanResult.availableTypes"
            :key="t"
            type="button"
            @click="selectedEventType = selectedEventType === t ? '' : t"
            class="aurora-pill rounded-full px-2 py-0.5 text-[0.625rem] font-medium transition"
            :class="
              selectedEventType === t
                ? 'bg-blue-600 text-white'
                : 'bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
            "
          >
            {{ t }}
          </button>
        </div>

        <!-- 快捷操作栏 -->
        <div class="flex items-center justify-between text-[0.6875rem]">
          <div class="flex items-center gap-1.5">
            <button
              type="button"
              @click="batchSelectFiltered('all')"
              class="text-blue-600 hover:underline dark:text-blue-400"
            >
              全选
            </button>
            <span class="text-[var(--border-strong)]">|</span>
            <button
              type="button"
              @click="batchSelectFiltered('none')"
              class="text-[var(--text-secondary)] hover:underline"
            >
              清空
            </button>
            <span class="text-[var(--border-strong)]">|</span>
            <button
              type="button"
              @click="batchSelectFiltered('invert')"
              class="text-[var(--text-secondary)] hover:underline"
            >
              反选
            </button>
          </div>

          <label class="flex cursor-pointer items-center gap-1 text-[var(--text-secondary)]">
            <input
              v-model="onlyAbnormal"
              type="checkbox"
              class="h-3 w-3 rounded accent-amber-500"
            />
            <span>仅异常</span>
          </label>
        </div>

        <button
          v-if="runStats.failed + runStats.unmatched > 0"
          type="button"
          @click="selectAbnormalFromReport(null)"
          class="w-full rounded-md bg-amber-500/12 py-1 text-[0.6875rem] font-medium text-amber-700 transition hover:bg-amber-500/20 dark:text-amber-300"
        >
          仅勾选本次异常条目 ({{ runStats.failed + runStats.unmatched }})
        </button>
      </div>

      <!-- 分段目录树 -->
      <div class="min-h-0 flex-1 overflow-y-auto p-2 space-y-1">
        <div class="px-1.5 pb-1 text-[0.6875rem] font-semibold uppercase tracking-wider text-[var(--text-muted)]">
          分段目录树 ({{ scanResult.segments.length }} 段)
        </div>

        <div
          v-for="seg in scanResult.segments"
          :key="seg.segment"
          class="flex items-center gap-2 rounded-lg px-2 py-1.5 transition hover:bg-[var(--bg-elevated)]"
        >
          <input
            type="checkbox"
            :checked="segmentStatsMap.get(seg.segment)?.allChecked ?? false"
            :indeterminate="segmentStatsMap.get(seg.segment)?.indeterminate ?? false"
            :disabled="isBusy"
            @change="onToggleSeg(seg, $event)"
            class="h-3.5 w-3.5 shrink-0 cursor-pointer rounded accent-blue-600"
          />

          <button
            type="button"
            @click="emit('jump-segment', seg.segment)"
            class="min-w-0 flex-1 text-left"
          >
            <div class="flex items-center justify-between gap-1">
              <span class="truncate text-xs font-medium text-[var(--text-primary)]">
                #{{ seg.segment }} {{ seg.comment }}
              </span>
              <span class="shrink-0 font-mono text-[0.625rem] text-[var(--text-muted)]">
                {{ seg.count }}
              </span>
            </div>
            <div class="flex items-center justify-between text-[0.625rem] text-[var(--text-muted)]">
              <span class="truncate">{{ seg.sourceName }}:{{ seg.startLine }}</span>
              <span
                v-if="(segmentStatsMap.get(seg.segment)?.ok ?? 0) + (segmentStatsMap.get(seg.segment)?.unmatched ?? 0) + (segmentStatsMap.get(seg.segment)?.failed ?? 0) > 0"
                class="flex shrink-0 items-center gap-1 font-mono"
              >
                <span v-if="segmentStatsMap.get(seg.segment)?.ok" class="text-emerald-500">
                  ✓{{ segmentStatsMap.get(seg.segment)?.ok }}
                </span>
                <span v-if="segmentStatsMap.get(seg.segment)?.unmatched" class="text-amber-500">
                  ~{{ segmentStatsMap.get(seg.segment)?.unmatched }}
                </span>
                <span v-if="segmentStatsMap.get(seg.segment)?.failed" class="text-rose-500">
                  ✕{{ segmentStatsMap.get(seg.segment)?.failed }}
                </span>
              </span>
            </div>
          </button>
        </div>

        <div
          v-if="scanResult.segments.length === 0"
          class="py-8 text-center text-xs text-[var(--text-muted)]"
        >
          暂无分段数据
        </div>
      </div>
    </div>

    <!-- 页签 2：配置档案快速切换与优先级说明 -->
    <div v-else-if="leftTab === 'config'" class="flex min-h-0 flex-1 flex-col p-3.5 text-xs text-[var(--text-secondary)] space-y-2.5 overflow-y-auto">
      <div class="rounded-xl border border-indigo-500/25 bg-indigo-500/8 p-3 space-y-1.5">
        <div class="font-semibold text-indigo-700 dark:text-indigo-300">
          双源配置优先级规则
        </div>
        <p class="leading-relaxed text-[0.6875rem]">
          当绑定了 <code class="font-mono">.yaml / .yml</code> 文件时，文件中<strong>非空的有效配置项</strong>优先级最高；
          若 YAML 里对应项留空（空串 / null / []）或未绑定 YAML，则自动使用 GUI 默认配置。
        </p>
        <p class="text-[0.6875rem] text-[var(--text-muted)]">
          注：YAML 中显式填写 <code class="font-mono">false</code> 或 <code class="font-mono">0</code> 属于有效配置，同样优先生效。
        </p>
      </div>

      <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 p-3 space-y-2">
        <div class="flex items-center justify-between">
          <span class="text-[0.6875rem] font-semibold text-[var(--text-primary)]">
            已挂载档案一键切换
          </span>
          <button
            type="button"
            :disabled="isBusy"
            @click="quickMountSidebarProfiles"
            class="inline-flex items-center gap-0.5 rounded bg-indigo-500/15 px-1.5 py-0.5 text-[0.625rem] font-semibold text-indigo-600 hover:bg-indigo-500/25 dark:text-indigo-300"
          >
            <Plus class="h-3 w-3" />
            <span>挂载</span>
          </button>
        </div>

        <div class="space-y-1 max-h-60 overflow-y-auto">
          <button
            type="button"
            :disabled="isBusy"
            @click="selectYamlProfile(null)"
            class="flex w-full items-center justify-between rounded-lg border px-2.5 py-1.5 text-left text-[0.6875rem] transition"
            :class="
              !snapshot?.yamlPath
                ? 'border-cyan-500 bg-cyan-500/12 font-semibold text-cyan-700 dark:text-cyan-300'
                : 'border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:bg-[var(--bg-elevated)]'
            "
          >
            <span>仅使用 GUI 独立配置</span>
          </button>

          <button
            v-for="prof in snapshot?.profiles ?? []"
            :key="prof.path"
            type="button"
            :disabled="isBusy || !prof.exists"
            @click="selectYamlProfile(prof.path)"
            :title="prof.path"
            class="flex w-full flex-col rounded-lg border px-2.5 py-1.5 text-left transition disabled:opacity-45"
            :class="
              snapshot?.yamlPath === prof.path
                ? 'border-indigo-500 bg-indigo-500/12'
                : 'border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:bg-[var(--bg-elevated)]'
            "
          >
            <div class="flex items-center justify-between gap-1 text-[0.6875rem] font-medium text-[var(--text-primary)]">
              <span class="truncate">{{ prof.name }}</span>
              <span
                v-if="!prof.exists"
                class="shrink-0 text-[0.5625rem] text-rose-500"
              >
                缺失
              </span>
            </div>
            <span class="truncate font-mono text-[0.5625rem] text-[var(--text-muted)]">
              {{ prof.path }}
            </span>
          </button>
        </div>
      </div>

      <button
        type="button"
        @click="leftTab = 'sources'"
        class="aurora-btn-subtle w-full rounded-lg border border-[var(--border-subtle)] py-1.5 text-xs font-medium text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]"
      >
        ← 返回数据源视图
      </button>
    </div>

    <div v-else class="flex-1 p-3.5 text-xs text-[var(--text-secondary)] space-y-2.5">
      <div class="rounded-xl border border-emerald-500/25 bg-emerald-500/8 p-3 space-y-1.5">
        <div class="font-semibold text-emerald-700 dark:text-emerald-300">
          历史测试报告中心
        </div>
        <p class="text-[0.6875rem] leading-relaxed">
          批量回放与单条直接发送均会自动归档保存。自动扫描当前配置目录下的 <code class="font-mono">test_reports/*.json</code> 与配置的报告路径，支持查阅明细与一键仅重跑异常项。
        </p>
      </div>

      <button
        type="button"
        @click="leftTab = 'sources'"
        class="w-full rounded-lg border border-[var(--border-subtle)] py-1.5 text-xs font-medium text-[var(--text-primary)] hover:bg-[var(--bg-elevated)]"
      >
        ← 返回数据源视图
      </button>
    </div>
  </aside>
</template>
