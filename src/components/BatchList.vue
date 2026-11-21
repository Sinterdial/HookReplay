<script setup lang="ts">
/**
 * 中栏：分段任务流卡片列表与逐段交互确认横幅（BatchList）
 *
 * 设计亮点：
 * 1. 当后端开启 `interactive: true` 并在每段前暂停询问时，悬浮展示带 Gemini 流光边框的确认横幅；
 * 2. 支持大文件（15MB+ 超大 JSONL 日志文件）按分段每页 120 条增量渲染，防止 DOM 节点过多卡顿；
 * 3. 每段卡片头支持整段勾选、折叠/展开、一键「仅发本段」；
 * 4. 每条消息实时显示语义状态徽章（200 成功 / 200 幂等跳过 / 400 未匹配 / 失败）与耗时 ms。
 */
import { computed, reactive, ref } from "vue";
import {
  ChevronDown,
  ChevronRight,
  Clock,
  Play,
  SkipForward,
  Sparkles,
  Square,
  Zap,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import type { IndexedEntry } from "../types";

const {
  filteredEntries,
  selectedIndices,
  activeEntry,
  resultsByIndex,
  segmentWaiting,
  waitCountdownMs,
  currentSegment,
  isBusy,
  selectActiveEntry,
  toggleEntrySelection,
  toggleSegmentSelection,
  runSingleSegment,
  controlRun,
} = useAppState();

/** 折叠状态集合（存放被手动折叠的分段编号） */
const collapsedSegments = reactive<Set<number>>(new Set());

/** 分段渲染窗口上限（默认 120 条，避免超大日志一次性挂载全部节点） */
const segmentRenderCap = reactive<Record<number, number>>({});

/** 分段 DOM 引用，用于从左侧目录树平滑跳转定位 */
const segmentRefs = reactive<Record<number, HTMLElement | null>>({});

const DEFAULT_SEG_CAP = 120;

interface GroupedSegment {
  segment: number;
  comment: string;
  sourceName: string;
  startLine: number;
  items: IndexedEntry[];
  selectedCount: number;
  allChecked: boolean;
  indeterminate: boolean;
  okCount: number;
  dedupedCount: number;
  unmatchedCount: number;
  failedCount: number;
}

/** 将筛选后的条目按出现分段聚合为分组卡片模型 */
const groupedSegments = computed<GroupedSegment[]>(() => {
  const groups: GroupedSegment[] = [];
  const bySeg = new Map<number, GroupedSegment>();

  for (const entry of filteredEntries.value) {
    let g = bySeg.get(entry.segment);
    if (!g) {
      g = {
        segment: entry.segment,
        comment: entry.comment,
        sourceName: entry.sourceName,
        startLine: entry.line,
        items: [],
        selectedCount: 0,
        allChecked: false,
        indeterminate: false,
        okCount: 0,
        dedupedCount: 0,
        unmatchedCount: 0,
        failedCount: 0,
      };
      bySeg.set(entry.segment, g);
      groups.push(g);
    }
    g.items.push(entry);
    if (selectedIndices.value.has(entry.index)) {
      g.selectedCount += 1;
    }
    const res = resultsByIndex.get(entry.index);
    if (res) {
      if (res.outcome === "deduped") g.dedupedCount += 1;
      else if (res.outcome === "ok") g.okCount += 1;
      else if (res.outcome === "unmatched") g.unmatchedCount += 1;
      else g.failedCount += 1;
    }
  }

  for (const g of groups) {
    g.allChecked = g.items.length > 0 && g.selectedCount === g.items.length;
    g.indeterminate =
      g.selectedCount > 0 && g.selectedCount < g.items.length;
  }
  return groups;
});

/** 平滑滚动到指定分段卡片（供父组件调用） */
function scrollToSegment(segmentId: number): void {
  collapsedSegments.delete(segmentId);
  const el = segmentRefs[segmentId];
  if (el) {
    el.scrollIntoView({ behavior: "smooth", block: "start" });
  }
}

defineExpose({ scrollToSegment });

function toggleCollapse(segId: number): void {
  if (collapsedSegments.has(segId)) collapsedSegments.delete(segId);
  else collapsedSegments.add(segId);
}

function getVisibleItems(seg: GroupedSegment): IndexedEntry[] {
  const cap = segmentRenderCap[seg.segment] ?? DEFAULT_SEG_CAP;
  return seg.items.slice(0, cap);
}

function expandMoreInSegment(segId: number, step = 200): void {
  const cur = segmentRenderCap[segId] ?? DEFAULT_SEG_CAP;
  segmentRenderCap[segId] = cur + step;
}

function expandAllInSegment(segId: number, total: number): void {
  segmentRenderCap[segId] = total;
}

/** 仅用于展示当前是否全部展开，避免编译提示未使用变量 */
const listScrollAnchor = ref<HTMLElement | null>(null);

/** 根据事件类型返回柔和的颜色徽章类 */
function eventTypeBadgeClass(eventType: string): string {
  const t = eventType.toLowerCase();
  if (t.includes("transfer")) {
    return "bg-emerald-500/12 text-emerald-700 dark:text-emerald-300";
  }
  if (t.includes("subscribe")) {
    return "bg-violet-500/12 text-violet-700 dark:text-violet-300";
  }
  if (t.includes("rss") || t.includes("entries")) {
    return "bg-orange-500/12 text-orange-700 dark:text-orange-300";
  }
  if (t.includes("grafana")) {
    return "bg-rose-500/12 text-rose-700 dark:text-rose-300";
  }
  if (t.includes("diun") || t.includes("dockhand")) {
    return "bg-cyan-500/12 text-cyan-700 dark:text-cyan-300";
  }
  return "bg-slate-500/12 text-slate-700 dark:text-slate-300";
}

/** 根据运行结果分类返回状态徽章类 */
function outcomeBadgeClass(outcome: string): string {
  if (outcome === "ok") {
    return "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 border-emerald-500/30";
  }
  if (outcome === "deduped") {
    return "bg-sky-500/15 text-sky-700 dark:text-sky-300 border-sky-500/30";
  }
  if (outcome === "unmatched") {
    return "bg-amber-500/15 text-amber-700 dark:text-amber-300 border-amber-500/30";
  }
  return "bg-rose-500/15 text-rose-700 dark:text-rose-300 border-rose-500/30";
}
</script>

<template>
  <main class="flex min-w-0 flex-1 flex-col bg-[var(--bg-canvas)]">
    <!-- 顶部悬浮：逐段交互确认 Gemini 流光横幅 -->
    <div
      v-if="segmentWaiting"
      class="mx-3.5 mt-3 rounded-2xl bg-[var(--bg-surface)] p-3.5 aurora-border-active"
    >
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="space-y-0.5 min-w-0">
          <div class="flex items-center gap-2">
            <Sparkles class="h-4 w-4 shrink-0 text-indigo-500 animate-pulse" />
            <span class="text-xs font-bold text-indigo-600 dark:text-indigo-300">
              交互确认 · 第 {{ segmentWaiting.segmentIndex }} / {{ segmentWaiting.totalSegments }} 段
            </span>
            <span class="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 font-mono text-[0.6875rem] text-[var(--text-secondary)]">
              {{ segmentWaiting.count }} 条
            </span>
          </div>
          <div class="truncate text-sm font-semibold text-[var(--text-primary)]">
            是否继续发送「{{ segmentWaiting.comment }}」？
          </div>
          <div class="font-mono text-[0.6875rem] text-[var(--text-muted)]">
            来源：{{ segmentWaiting.sourceName }}:{{ segmentWaiting.startLine }}
          </div>
        </div>

        <div class="flex flex-wrap items-center gap-2 shrink-0">
          <button
            type="button"
            @click="controlRun('confirm', 'y')"
            class="aurora-btn inline-flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
          >
            <Play class="h-3.5 w-3.5" />
            <span>继续本段 (Y)</span>
          </button>

          <button
            type="button"
            @click="controlRun('confirm', 'n')"
            class="inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
          >
            <SkipForward class="h-3.5 w-3.5" />
            <span>跳过本段 (N)</span>
          </button>

          <button
            type="button"
            @click="controlRun('confirm', 'a')"
            class="inline-flex items-center gap-1 rounded-lg border border-emerald-500/40 bg-emerald-500/12 px-3 py-1.5 text-xs font-semibold text-emerald-700 hover:bg-emerald-500/20 dark:text-emerald-300"
          >
            <Zap class="h-3.5 w-3.5" />
            <span>全部自动 (A)</span>
          </button>

          <button
            type="button"
            @click="controlRun('confirm', 'q')"
            class="inline-flex items-center gap-1 rounded-lg border border-rose-500/40 bg-rose-500/12 px-3 py-1.5 text-xs font-semibold text-rose-700 hover:bg-rose-500/20 dark:text-rose-300"
          >
            <Square class="h-3.5 w-3.5" />
            <span>停止 (Q)</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 段间等待倒计时提示条 -->
    <div
      v-else-if="waitCountdownMs > 0"
      class="mx-3.5 mt-3 flex items-center justify-between rounded-xl border border-indigo-500/30 bg-indigo-500/10 px-3.5 py-2 text-xs text-indigo-700 dark:text-indigo-300"
    >
      <div class="flex items-center gap-2">
        <Clock class="h-4 w-4 animate-spin" />
        <span>正在进行段间等待（聚合窗口隔离保护）…</span>
      </div>
      <span class="font-mono font-bold">
        剩余 {{ (waitCountdownMs / 1000).toFixed(1) }}s
      </span>
    </div>

    <!-- 分段卡片滚动容器 -->
    <div
      ref="listScrollAnchor"
      class="min-h-0 flex-1 overflow-y-auto p-3.5 space-y-3"
    >
      <div
        v-for="seg in groupedSegments"
        :key="seg.segment"
        :ref="
          (el) => {
            segmentRefs[seg.segment] = el as HTMLElement | null;
          }
        "
        class="aurora-card-interactive overflow-hidden rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] shadow-xs transition-all"
        :class="{
          'aurora-border-active':
            segmentWaiting?.segmentId === seg.segment ||
            (isBusy && currentSegment === seg.segment),
        }"
      >
        <!-- 分段头部工具条 -->
        <div
          class="flex items-center justify-between gap-2 border-b border-[var(--border-subtle)] bg-[var(--bg-elevated)]/65 px-3 py-2"
        >
          <div class="flex min-w-0 items-center gap-2">
            <input
              type="checkbox"
              :checked="seg.allChecked"
              :indeterminate="seg.indeterminate"
              :disabled="isBusy"
              @change="
                toggleSegmentSelection(
                  seg.segment,
                  ($event.target as HTMLInputElement).checked
                )
              "
              class="h-3.5 w-3.5 shrink-0 cursor-pointer rounded accent-blue-600"
            />

            <button
              type="button"
              @click="toggleCollapse(seg.segment)"
              class="flex min-w-0 items-center gap-1.5 text-left"
            >
              <ChevronRight
                v-if="collapsedSegments.has(seg.segment)"
                class="h-4 w-4 shrink-0 text-[var(--text-muted)]"
              />
              <ChevronDown
                v-else
                class="h-4 w-4 shrink-0 text-[var(--text-muted)]"
              />
              <span class="rounded bg-blue-600/12 px-1.5 py-0.5 font-mono text-[0.6875rem] font-bold text-blue-600 dark:text-blue-300">
                #{{ seg.segment }}
              </span>
              <span class="truncate text-xs font-semibold text-[var(--text-primary)]">
                {{ seg.comment }}
              </span>
            </button>
          </div>

          <div class="flex shrink-0 items-center gap-2">
            <span class="hidden sm:inline font-mono text-[0.6875rem] text-[var(--text-muted)]">
              {{ seg.sourceName }} · {{ seg.items.length }} 条
            </span>

            <!-- 段内快捷统计 -->
            <span
              v-if="seg.okCount + seg.dedupedCount + seg.unmatchedCount + seg.failedCount > 0"
              class="flex items-center gap-1 rounded-full bg-[var(--bg-surface)] px-2 py-0.5 font-mono text-[0.625rem] border border-[var(--border-subtle)]"
            >
              <span v-if="seg.okCount" class="text-emerald-600 dark:text-emerald-400">
                ✓{{ seg.okCount }}
              </span>
              <span v-if="seg.dedupedCount" class="text-sky-600 dark:text-sky-400">
                ↺{{ seg.dedupedCount }}
              </span>
              <span v-if="seg.unmatchedCount" class="text-amber-600 dark:text-amber-400">
                ~{{ seg.unmatchedCount }}
              </span>
              <span v-if="seg.failedCount" class="text-rose-600 dark:text-rose-400">
                ✕{{ seg.failedCount }}
              </span>
            </span>

            <button
              type="button"
              :disabled="isBusy"
              @click="runSingleSegment(seg.segment)"
              title="仅发送这一段中的所有消息"
              class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2 py-1 text-[0.6875rem] font-medium text-[var(--text-primary)] transition hover:border-blue-500/50 hover:text-blue-600 dark:hover:text-blue-300 disabled:opacity-40"
            >
              <Play class="h-3 w-3 text-emerald-500" />
              <span>发本段</span>
            </button>
          </div>
        </div>

        <!-- 分段条目明细行（含大文件按需分页加载保护） -->
        <div
          v-if="!collapsedSegments.has(seg.segment)"
          class="divide-y divide-[var(--border-subtle)]/70"
        >
          <div
            v-for="entry in getVisibleItems(seg)"
            :key="entry.index"
            @click="selectActiveEntry(entry)"
            class="flex cursor-pointer items-center justify-between gap-2 px-3 py-2 text-xs transition"
            :class="
              activeEntry?.index === entry.index
                ? 'aurora-row-active'
                : 'hover:bg-[var(--bg-elevated)]/70'
            "
          >
            <div class="flex min-w-0 flex-1 items-center gap-2">
              <input
                type="checkbox"
                :checked="selectedIndices.has(entry.index)"
                :disabled="isBusy"
                @click.stop
                @change="toggleEntrySelection(entry.index)"
                class="h-3.5 w-3.5 shrink-0 cursor-pointer rounded accent-blue-600"
              />

              <span class="w-9 shrink-0 font-mono text-[0.6875rem] text-[var(--text-muted)]">
                #{{ entry.index }}
              </span>

              <span
                class="shrink-0 rounded-md px-1.5 py-0.5 font-mono text-[0.625rem] font-semibold"
                :class="eventTypeBadgeClass(entry.eventType)"
              >
                {{ entry.eventType }}
              </span>

              <span
                class="min-w-0 truncate text-[var(--text-primary)]"
                :title="entry.label"
              >
                {{ entry.title || entry.label }}
              </span>
            </div>

            <div class="flex shrink-0 items-center gap-2">
              <span class="hidden md:inline font-mono text-[0.625rem] text-[var(--text-muted)]">
                L{{ entry.line }}
              </span>

              <template v-if="resultsByIndex.get(entry.index)">
                <span
                  class="rounded-full border px-2 py-0.5 text-[0.625rem] font-semibold"
                  :class="outcomeBadgeClass(resultsByIndex.get(entry.index)!.outcome)"
                >
                  {{ resultsByIndex.get(entry.index)!.badge }}
                </span>
                <span class="w-12 text-right font-mono text-[0.625rem] text-[var(--text-muted)]">
                  {{ resultsByIndex.get(entry.index)!.ms }}ms
                </span>
              </template>

              <span
                v-else
                class="rounded-full bg-[var(--bg-elevated)] px-2 py-0.5 text-[0.625rem] text-[var(--text-muted)]"
              >
                待发送
              </span>
            </div>
          </div>

          <!-- 超过渲染窗口时展示渐进展开按钮 -->
          <div
            v-if="seg.items.length > getVisibleItems(seg).length"
            class="flex items-center justify-between bg-[var(--bg-elevated)]/40 px-3 py-2 text-[0.6875rem] text-[var(--text-secondary)]"
          >
            <span>
              已显示 {{ getVisibleItems(seg).length }} / {{ seg.items.length }} 条（防卡顿保护已开启）
            </span>
            <div class="flex items-center gap-2">
              <button
                type="button"
                @click="expandMoreInSegment(seg.segment)"
                class="font-semibold text-blue-600 hover:underline dark:text-blue-400"
              >
                加载后续 200 条
              </button>
              <button
                type="button"
                @click="expandAllInSegment(seg.segment, seg.items.length)"
                class="text-[var(--text-muted)] hover:underline"
              >
                显示本段全部
              </button>
            </div>
          </div>
        </div>
      </div>

      <div
        v-if="groupedSegments.length === 0"
        class="flex h-64 flex-col items-center justify-center rounded-2xl border border-dashed border-[var(--border-strong)] bg-[var(--bg-surface)]/50 p-6 text-center text-xs text-[var(--text-muted)]"
      >
        <p class="font-medium text-[var(--text-secondary)]">没有符合当前筛选条件的消息条目</p>
        <p class="mt-1">请在左侧添加日志文件、调整过滤关键词，或使用「临时粘贴 JSON」直接调试。</p>
      </div>
    </div>
  </main>
</template>
