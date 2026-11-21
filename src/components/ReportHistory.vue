<script setup lang="ts">
/**
 * 中心面板：历史测试报告浏览器与异常复测视图（ReportHistory）
 *
 * 功能说明：
 * 1. 自动扫描并按时间倒序列出所有归档历史 JSON 测试报告（含成功 / 幂等跳过 / 未匹配 / 失败统计）；
 * 2. 支持单份报告删除与点击展开查看每条的响应体 / 实际发送体 JSON 语法高亮；
 * 3. 支持仅过滤查看报告中的异常条目，并一键将对应条目勾选回数据源工作区用于复测。
 */
import { computed, ref } from "vue";
import {
  ArrowUpRight,
  Calendar,
  ChevronDown,
  ChevronRight,
  FileJson2,
  PanelLeftClose,
  Pin,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useWorkspaceLayout } from "../composables/useWorkspaceLayout";
import type { ReportSummary } from "../types";
import JsonViewer from "./JsonViewer.vue";
import SplitResizer from "./SplitResizer.vue";

const {
  reportsList,
  inspectedReport,
  isLoadingReport,
  fetchReportsList,
  openReportDetail,
  deleteReport,
  selectAbnormalFromReport,
  pushToast,
} = useAppState();

const {
  leftFloating,
  reportsListFloating,
  effectiveReportsListWidth,
  reportsDrawerOpen,
  activeResizer,
  openFloatingWithIntent,
  openFloatingImmediate,
  cancelOpenIntent,
  keepFloatingOpen,
  scheduleCloseFloating,
  handleFloatingFocusIn,
  handleFloatingFocusOut,
  toggleFloating,
  beginResize,
  updateResize,
  endResize,
  resetPaneWidth,
} = useWorkspaceLayout();

const splitContainerRef = ref<HTMLElement | null>(null);

const onlyReportAbnormal = ref(false);
const inspectedReportPath = ref<string | null>(null);
const expandedRowKey = ref<string | null>(null);

const filteredReportItems = computed(() => {
  const list = inspectedReport.value?.results ?? [];
  if (!onlyReportAbnormal.value) return list;
  return list.filter((r) => r.outcome === "failed" || r.outcome === "unmatched");
});

function formatIsoTime(iso: string): string {
  if (!iso) return "-";
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false });
  } catch {
    return iso;
  }
}

async function handleSelectReport(rep: ReportSummary): Promise<void> {
  inspectedReportPath.value = rep.path;
  expandedRowKey.value = null;
  await openReportDetail(rep.path);
}

async function handleDeleteReport(rep: ReportSummary): Promise<void> {
  await deleteReport(rep.path, rep.finishedAt);
  if (inspectedReportPath.value === rep.path) {
    inspectedReportPath.value = null;
    expandedRowKey.value = null;
  }
}

function toggleExpandItem(key: string): void {
  expandedRowKey.value = expandedRowKey.value === key ? null : key;
}

function handleResetReportsWidth(): void {
  resetPaneWidth("reports");
  pushToast("已恢复默认报告清单宽度", "info", 1500);
}
</script>

<template>
  <section class="flex min-w-0 flex-1 flex-col bg-[var(--bg-canvas)]">
    <!-- 顶部栏 -->
    <div
      class="flex items-center justify-between border-b border-[var(--border-subtle)] bg-[var(--bg-surface)] px-4 py-2.5"
    >
      <div class="flex items-center gap-2">
        <FileJson2 class="h-4 w-4 text-emerald-500" />
        <h2 class="text-sm font-bold text-[var(--text-primary)]">
          历史运行报告中心（共 {{ reportsList.length }} 份）
        </h2>
      </div>

      <div class="flex items-center gap-2">
        <button
          type="button"
          @click="toggleFloating('reports')"
          @mouseenter="reportsListFloating && openFloatingWithIntent('reports')"
          @mouseleave="reportsListFloating && (cancelOpenIntent('reports'), reportsDrawerOpen && scheduleCloseFloating('reports'))"
          :title="
            reportsListFloating
              ? '固定历史报告清单（Ctrl+Shift+\\）'
              : '转为悬浮清单模式（悬停左缘或此按钮展开 · Ctrl+Shift+\\）'
          "
          class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium transition"
          :class="
            reportsListFloating
              ? 'border-indigo-500/45 bg-indigo-500/12 text-indigo-600 dark:text-indigo-300'
              : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
          "
        >
          <Pin v-if="reportsListFloating" class="h-3.5 w-3.5" />
          <PanelLeftClose v-else class="h-3.5 w-3.5" />
          <span>{{ reportsListFloating ? "固定清单" : "悬浮清单" }}</span>
        </button>

        <button
          type="button"
          @click="fetchReportsList()"
          class="aurora-btn-subtle inline-flex items-center gap-1 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 py-1 text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
        >
          <RefreshCw class="h-3.5 w-3.5" />
          <span>刷新列表</span>
        </button>
      </div>
    </div>

    <!-- 主内容左右双拼：报告列表（支持拖宽与悬浮） + 报告详情 -->
    <div ref="splitContainerRef" class="relative flex min-h-0 flex-1 overflow-hidden">
      <!-- 悬浮模式触发：若主左栏未悬浮，则使用紧贴左侧的热区与拉手 -->
      <div
        v-if="reportsListFloating && !leftFloating"
        @mouseenter="openFloatingWithIntent('reports')"
        @mouseleave="cancelOpenIntent('reports'); if (reportsDrawerOpen) scheduleCloseFloating('reports')"
        class="group absolute inset-y-0 left-0 z-20 flex w-4 items-center justify-start"
      >
        <button
          type="button"
          @click="openFloatingImmediate('reports')"
          title="展开历史报告清单（Ctrl+Shift+\\）"
          :class="{ 'opacity-0 pointer-events-none': reportsDrawerOpen }"
          class="aurora-edge-tab flex h-14 w-3.5 items-center justify-center rounded-r-lg border border-l-0 border-[var(--border-strong)] bg-[var(--bg-surface)]/90 text-[var(--text-muted)] shadow-sm backdrop-blur-sm transition-all group-hover:w-4.5 group-hover:text-emerald-500"
        >
          <ChevronRight class="h-3.5 w-3.5" />
        </button>
      </div>

      <!-- 若主左栏也已悬浮，避开最左缘而在内容区左上角放置悬浮触发胶囊 -->
      <button
        v-else-if="reportsListFloating && leftFloating"
        type="button"
        @mouseenter="openFloatingWithIntent('reports')"
        @mouseleave="cancelOpenIntent('reports'); if (reportsDrawerOpen) scheduleCloseFloating('reports')"
        @click="openFloatingImmediate('reports')"
        title="悬停或点击展开历史报告清单（Ctrl+Shift+\\）"
        :class="{ 'opacity-0 pointer-events-none': reportsDrawerOpen }"
        class="aurora-edge-tab absolute left-3 top-2.5 z-20 inline-flex items-center gap-1 rounded-full border border-[var(--border-strong)] bg-[var(--bg-surface)]/90 px-2.5 py-1 text-[0.6875rem] font-semibold text-[var(--text-secondary)] shadow-sm backdrop-blur-sm transition"
      >
        <ChevronRight class="h-3.5 w-3.5 text-emerald-500" />
        <span>报告清单 ({{ reportsList.length }})</span>
      </button>

      <!-- 左侧报告清单持久化容器 -->
      <div
        :style="{ width: `${effectiveReportsListWidth}px` }"
        @mouseenter="reportsListFloating && keepFloatingOpen('reports')"
        @mouseleave="reportsListFloating && scheduleCloseFloating('reports')"
        @focusin="reportsListFloating && handleFloatingFocusIn('reports', $event)"
        @focusout="reportsListFloating && handleFloatingFocusOut('reports', $event)"
        :class="
          reportsListFloating
            ? [
                'floating-drawer absolute inset-y-2 left-2 z-30 flex flex-col overflow-hidden rounded-2xl border border-[var(--border-strong)] bg-[var(--bg-surface)]/95 backdrop-blur-md transition-[transform,opacity] duration-200 ease-out',
                reportsDrawerOpen
                  ? 'translate-x-0 opacity-100 pointer-events-auto'
                  : '-translate-x-[calc(100%+18px)] opacity-0 pointer-events-none',
              ]
            : 'relative flex h-full shrink-0 flex-col border-r border-[var(--border-subtle)] bg-[var(--bg-surface)]/70'
        "
      >
        <div
          v-if="reportsListFloating"
          class="flex shrink-0 items-center justify-between border-b border-[var(--border-subtle)] px-3 py-2 text-xs"
        >
          <span class="font-semibold text-[var(--text-primary)]">
            报告清单 ({{ reportsList.length }})
          </span>
          <button
            type="button"
            @click="toggleFloating('reports')"
            title="固定为常驻分栏"
            class="inline-flex items-center gap-1 rounded-md border border-indigo-500/40 bg-indigo-500/12 px-2 py-0.5 text-[0.625rem] font-semibold text-indigo-600 dark:text-indigo-300"
          >
            <Pin class="h-3 w-3" />
            <span>固定</span>
          </button>
        </div>

        <div class="min-h-0 flex-1 overflow-y-auto p-3 space-y-2">
        <div
          v-for="rep in reportsList"
          :key="rep.path"
          role="button"
          tabindex="0"
          @click="handleSelectReport(rep)"
          @keydown.enter="handleSelectReport(rep)"
          class="aurora-card-interactive group w-full cursor-pointer rounded-xl border p-3 text-left transition"
          :class="
            inspectedReport &&
            (inspectedReportPath === rep.path ||
              (rep.runId && inspectedReport.runId === rep.runId) ||
              inspectedReport.finishedAt === rep.finishedAt)
              ? 'border-emerald-500/60 bg-emerald-500/8'
              : 'border-[var(--border-subtle)] bg-[var(--bg-surface)] hover:bg-[var(--bg-elevated)]'
          "
        >
          <div class="flex items-center justify-between gap-1.5">
            <div class="flex min-w-0 items-center gap-1.5">
              <span class="truncate text-xs font-bold text-[var(--text-primary)]">
                {{ rep.fileName }}
              </span>
              <span
                v-if="rep.fileName.startsWith('single-')"
                class="shrink-0 rounded bg-purple-500/15 px-1.5 py-0.5 text-[0.5625rem] font-semibold text-purple-700 dark:text-purple-300"
              >
                单条
              </span>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <button
                type="button"
                @click.stop="handleDeleteReport(rep)"
                title="删除此条历史报告"
                class="rounded p-0.5 text-[var(--text-muted)] opacity-60 transition hover:bg-rose-500/15 hover:text-rose-500 group-hover:opacity-100"
              >
                <Trash2 class="h-3.5 w-3.5" />
              </button>
              <ArrowUpRight class="h-3.5 w-3.5 text-[var(--text-muted)]" />
            </div>
          </div>

          <div class="mt-1 flex items-center justify-between gap-1 text-[0.625rem] text-[var(--text-muted)] font-mono">
            <span class="inline-flex items-center gap-1">
              <Calendar class="h-3 w-3" />
              <span>{{ formatIsoTime(rep.finishedAt) }}</span>
            </span>
            <span>{{ rep.durationMs }}ms</span>
          </div>

          <div class="mt-1 truncate font-mono text-[0.625rem] text-[var(--text-secondary)]">
            {{ rep.target || "-" }}
          </div>

          <div class="mt-2 flex flex-wrap gap-1 text-[0.625rem] font-medium">
            <span class="rounded bg-emerald-500/15 px-1.5 py-0.5 text-emerald-700 dark:text-emerald-300">
              成功 {{ rep.ok }}
            </span>
            <span
              v-if="rep.deduped > 0"
              class="rounded bg-sky-500/15 px-1.5 py-0.5 text-sky-700 dark:text-sky-300"
            >
              跳过 {{ rep.deduped }}
            </span>
            <span class="rounded bg-amber-500/15 px-1.5 py-0.5 text-amber-700 dark:text-amber-300">
              未匹配 {{ rep.unmatched }}
            </span>
            <span class="rounded bg-rose-500/15 px-1.5 py-0.5 text-rose-700 dark:text-rose-300">
              失败 {{ rep.failed }}
            </span>
          </div>
        </div>

          <div
            v-if="reportsList.length === 0"
            class="py-10 text-center text-xs text-[var(--text-muted)]"
          >
            暂未扫描到历史 JSON 报告文件。完成一次批量回放后，系统将自动按时间戳归档并显示在此处。
          </div>
        </div>

        <SplitResizer
          :floating="reportsListFloating"
          side="right"
          :active="activeResizer === 'reports'"
          @resize-start="beginResize('reports', splitContainerRef?.clientWidth)"
          @resize-move="(dx) => updateResize('reports', dx)"
          @resize-end="endResize"
          @reset="handleResetReportsWidth"
        />
      </div>

      <!-- 右侧报告明细查看器 -->
      <div
        class="flex min-w-0 flex-1 flex-col overflow-hidden p-4"
        @mousedown="reportsListFloating && scheduleCloseFloating('reports')"
      >
        <div
          v-if="isLoadingReport"
          class="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]"
        >
          正在解析报告 JSON 明细…
        </div>

        <template v-else-if="inspectedReport">
          <div
            class="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-3.5"
          >
            <div class="space-y-1 text-xs">
              <div class="font-bold text-[var(--text-primary)]">
                目标：{{ inspectedReport.target }}
              </div>
              <div class="font-mono text-[0.6875rem] text-[var(--text-muted)]">
                完成时间：{{ formatIsoTime(inspectedReport.finishedAt) }} · 总耗时 {{ inspectedReport.durationMs }}ms · 共 {{ inspectedReport.total }} 条
              </div>
            </div>

            <div class="flex items-center gap-3">
              <label class="flex cursor-pointer items-center gap-1.5 text-xs text-[var(--text-secondary)]">
                <input
                  v-model="onlyReportAbnormal"
                  type="checkbox"
                  class="h-3.5 w-3.5 rounded accent-amber-500"
                />
                <span>仅显示异常项</span>
              </label>

              <button
                v-if="inspectedReport.failed + inspectedReport.unmatched > 0"
                type="button"
                @click="selectAbnormalFromReport(inspectedReport)"
                class="aurora-btn inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
              >
                <RotateCcw class="h-3.5 w-3.5" />
                <span>
                  勾选异常项回放 ({{ inspectedReport.failed + inspectedReport.unmatched }})
                </span>
              </button>
            </div>
          </div>

          <div
            class="min-h-0 flex-1 overflow-y-auto rounded-2xl border border-[var(--border-subtle)] bg-[var(--bg-surface)] divide-y divide-[var(--border-subtle)]"
          >
            <div
              v-for="(item, rowIdx) in filteredReportItems"
              :key="`${item.index}-${rowIdx}`"
              class="flex flex-col"
            >
              <button
                type="button"
                @click="toggleExpandItem(`${item.index}-${rowIdx}`)"
                class="flex w-full items-center justify-between gap-3 px-3.5 py-2 text-left text-xs transition hover:bg-[var(--bg-elevated)]/60"
              >
                <div class="flex min-w-0 items-center gap-2">
                  <ChevronDown
                    v-if="expandedRowKey === `${item.index}-${rowIdx}`"
                    class="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]"
                  />
                  <ChevronRight
                    v-else
                    class="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]"
                  />
                  <span class="w-9 shrink-0 font-mono text-[0.6875rem] text-[var(--text-muted)]">
                    #{{ item.index }}
                  </span>
                  <span class="truncate font-medium text-[var(--text-primary)]">
                    {{ item.label }}
                  </span>
                </div>

                <div class="flex shrink-0 items-center gap-2">
                  <span
                    class="rounded-full px-2 py-0.5 text-[0.625rem] font-semibold"
                    :class="
                      item.outcome === 'ok'
                        ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300'
                        : item.outcome === 'deduped'
                          ? 'bg-sky-500/15 text-sky-700 dark:text-sky-300'
                          : item.outcome === 'unmatched'
                            ? 'bg-amber-500/15 text-amber-700 dark:text-amber-300'
                            : 'bg-rose-500/15 text-rose-700 dark:text-rose-300'
                    "
                  >
                    {{ item.badge || `HTTP ${item.status}` }}
                  </span>
                  <span class="w-14 text-right font-mono text-[0.6875rem] text-[var(--text-muted)]">
                    {{ item.ms }}ms
                  </span>
                </div>
              </button>

              <!-- 展开单条响应与请求体 JSON 高亮详情 -->
              <div
                v-if="expandedRowKey === `${item.index}-${rowIdx}`"
                class="border-t border-[var(--border-subtle)] bg-[var(--bg-elevated)]/35 px-4 py-3 space-y-2.5 text-xs"
              >
                <div class="break-all font-mono text-[0.6875rem] text-[var(--text-secondary)]">
                  目标 URL：{{ item.finalUrl || "-" }}
                </div>
                <div
                  v-if="item.error"
                  class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1.5 font-mono text-[0.6875rem] text-rose-700 dark:text-rose-300"
                >
                  错误信息：{{ item.error }}
                </div>
                <div class="grid grid-cols-1 gap-3 xl:grid-cols-2">
                  <div class="space-y-1">
                    <div class="text-[0.6875rem] font-semibold text-[var(--text-secondary)]">
                      响应内容 Response
                    </div>
                    <JsonViewer
                      :code="item.response"
                      :auto-pretty="true"
                      max-height="15rem"
                      empty-text="空响应体"
                    />
                  </div>
                  <div class="space-y-1">
                    <div class="text-[0.6875rem] font-semibold text-[var(--text-secondary)]">
                      实际发送请求体 Sent Body
                    </div>
                    <JsonViewer
                      :code="item.sentBody"
                      :auto-pretty="true"
                      max-height="15rem"
                      empty-text="空请求体"
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>
        </template>

        <div
          v-else
          class="flex flex-1 flex-col items-center justify-center rounded-2xl border border-dashed border-[var(--border-strong)] text-xs text-[var(--text-muted)]"
        >
          在左侧点击任意一份历史报告，即可查看当时全部请求明细（支持展开 JSON 高亮）并一键重跑异常项。
        </div>
      </div>
    </div>
  </section>
</template>
