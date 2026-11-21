<script setup lang="ts">
/**
 * HookReplay 桌面应用根组件（App.vue）
 *
 * 组装三栏响应式桌面工作台：
 * - 顶部 Gemini 流光饰条（.aurora-top-bar）+ 主控栏（TopBar）；
 * - 左栏（SourceSidebar）：数据源管理、# 注释分段目录树与配置档案快切；
 * - 中栏：在「分段任务流（BatchList）」/「双源配置中心（ConfigPanel）」/「历史报告（ReportHistory）」间无缝切换；
 * - 右栏（InspectorPane）：支持 Payload 语法高亮预览、源码在线编辑与单条即时重发；
 * - 全局模态层：临时调试台（ScratchpadModal）、首次配置向导（SetupWizard）、字体外观设置（AppearanceModal）与 Toast 栈。
 */
import { onMounted, ref, watch } from "vue";
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
import { useAppState } from "./composables/useAppState";
import { useWorkspaceLayout, type ResizablePane } from "./composables/useWorkspaceLayout";
import TopBar from "./components/TopBar.vue";
import SourceSidebar from "./components/SourceSidebar.vue";
import BatchList from "./components/BatchList.vue";
import ConfigPanel from "./components/ConfigPanel.vue";
import ReportHistory from "./components/ReportHistory.vue";
import InspectorPane from "./components/InspectorPane.vue";
import SplitResizer from "./components/SplitResizer.vue";
import ScratchpadModal from "./components/ScratchpadModal.vue";
import SetupWizard from "./components/SetupWizard.vue";
import AppearanceModal from "./components/AppearanceModal.vue";

const {
  leftTab,
  activeEntry,
  isBusy,
  toasts,
  showAppearanceModal,
  showSetupWizard,
  showScratchpadModal,
  pushToast,
  initApp,
} = useAppState();

const {
  leftFloating,
  rightFloating,
  effectiveLeftWidth,
  effectiveRightWidth,
  leftDrawerOpen,
  rightDrawerOpen,
  activeResizer,
  openFloatingWithIntent,
  openFloatingImmediate,
  cancelOpenIntent,
  keepFloatingOpen,
  scheduleCloseFloating,
  handleFloatingFocusIn,
  handleFloatingFocusOut,
  closeAllFloatingDrawers,
  beginResize,
  updateResize,
  endResize,
  resetPaneWidth,
} = useWorkspaceLayout();

const workspaceRef = ref<HTMLElement | null>(null);
const batchListRef = ref<InstanceType<typeof BatchList> | null>(null);

/** 从左侧分段目录树点击后，让中栏列表平滑定位到目标分段 */
function handleJumpSegment(segmentId: number): void {
  batchListRef.value?.scrollToSegment(segmentId);
}

/** 双击分隔线重置分栏默认宽度 */
function handleResetPaneWidth(pane: ResizablePane): void {
  resetPaneWidth(pane);
  pushToast("已恢复默认分栏宽度", "info", 1500);
}

/** 任意模态框打开时立即收起悬浮抽屉，防止层叠遮挡 */
watch([showAppearanceModal, showSetupWizard, showScratchpadModal], ([a, b, c]) => {
  if (a || b || c) {
    closeAllFloatingDrawers();
  }
});

onMounted(() => {
  void initApp();
});
</script>

<template>
  <div class="flex h-screen w-screen flex-col overflow-hidden bg-[var(--bg-canvas)] text-[var(--text-primary)]">
    <!-- 顶部 Gemini 流光装饰条 -->
    <div class="aurora-top-bar shrink-0" :class="{ 'is-busy': isBusy }" />

    <!-- 顶部主控栏 -->
    <TopBar />

    <!-- 主工作台体：左栏（支持拖动调宽与悬浮） + 动态中心工作区 + 右栏（支持拖动调宽与悬浮） -->
    <div ref="workspaceRef" class="relative flex min-h-0 flex-1 overflow-hidden">
      <!-- 左侧悬浮模式边缘感应热区与拉手 -->
      <div
        v-if="leftFloating"
        @mouseenter="openFloatingWithIntent('left')"
        @mouseleave="cancelOpenIntent('left'); if (leftDrawerOpen) scheduleCloseFloating('left')"
        class="group absolute inset-y-0 left-0 z-30 flex w-4 items-center justify-start"
      >
        <button
          type="button"
          @click="openFloatingImmediate('left')"
          title="展开左侧导航栏（Ctrl+\）"
          :class="{ 'opacity-0 pointer-events-none': leftDrawerOpen }"
          class="aurora-edge-tab flex h-14 w-3.5 items-center justify-center rounded-r-lg border border-l-0 border-[var(--border-strong)] bg-[var(--bg-surface)]/90 text-[var(--text-muted)] shadow-sm backdrop-blur-sm transition-all group-hover:w-4.5 group-hover:text-indigo-500"
        >
          <ChevronRight class="h-3.5 w-3.5" />
        </button>
      </div>

      <!-- 左侧导航栏（零卸载切换固定 / 悬浮抽屉，内置右侧拖宽手柄） -->
      <div
        :style="{ width: `${effectiveLeftWidth}px` }"
        @mouseenter="leftFloating && keepFloatingOpen('left')"
        @mouseleave="leftFloating && scheduleCloseFloating('left')"
        @focusin="leftFloating && handleFloatingFocusIn('left', $event)"
        @focusout="leftFloating && handleFloatingFocusOut('left', $event)"
        :class="
          leftFloating
            ? [
                'floating-drawer absolute inset-y-2 left-2 z-40 flex overflow-hidden rounded-2xl border border-[var(--border-strong)] bg-[var(--bg-surface)]/95 backdrop-blur-md transition-[transform,opacity] duration-200 ease-out',
                leftDrawerOpen
                  ? 'translate-x-0 opacity-100 pointer-events-auto'
                  : '-translate-x-[calc(100%+18px)] opacity-0 pointer-events-none',
              ]
            : 'relative flex h-full shrink-0'
        "
      >
        <SourceSidebar class="min-w-0 flex-1" @jump-segment="handleJumpSegment" />
        <SplitResizer
          :floating="leftFloating"
          side="right"
          :active="activeResizer === 'left'"
          @resize-start="beginResize('left', workspaceRef?.clientWidth)"
          @resize-move="(dx) => updateResize('left', dx)"
          @resize-end="endResize"
          @reset="handleResetPaneWidth('left')"
        />
      </div>

      <!-- 中央动态工作区（点击空白/内容区时自动收起悬浮侧栏） -->
      <div
        class="flex min-h-0 min-w-0 flex-1 overflow-hidden"
        @mousedown="closeAllFloatingDrawers()"
      >
        <BatchList v-if="leftTab === 'sources'" ref="batchListRef" />
        <ConfigPanel v-else-if="leftTab === 'config'" />
        <ReportHistory v-else />
      </div>

      <!-- 右侧检视栏（仅在数据源工作区挂载，支持拖动调宽与悬浮抽屉） -->
      <template v-if="leftTab === 'sources'">
        <div
          v-if="rightFloating"
          @mouseenter="openFloatingWithIntent('right')"
          @mouseleave="cancelOpenIntent('right'); if (rightDrawerOpen) scheduleCloseFloating('right')"
          class="group absolute inset-y-0 right-0 z-30 flex w-4 items-center justify-end"
        >
          <button
            type="button"
            @click="openFloatingImmediate('right')"
            :title="
              activeEntry
                ? `展开单条检视器 #${activeEntry.index}（Ctrl+Shift+\\）`
                : '展开单条检视器（Ctrl+Shift+\\）'
            "
            :class="{ 'opacity-0 pointer-events-none': rightDrawerOpen }"
            class="aurora-edge-tab flex h-14 w-3.5 items-center justify-center rounded-l-lg border border-r-0 border-[var(--border-strong)] bg-[var(--bg-surface)]/90 text-[var(--text-muted)] shadow-sm backdrop-blur-sm transition-all group-hover:w-4.5 group-hover:text-indigo-500"
          >
            <ChevronLeft class="h-3.5 w-3.5" />
          </button>
        </div>

        <div
          :style="{ width: `${effectiveRightWidth}px` }"
          @mouseenter="rightFloating && keepFloatingOpen('right')"
          @mouseleave="rightFloating && scheduleCloseFloating('right')"
          @focusin="rightFloating && handleFloatingFocusIn('right', $event)"
          @focusout="rightFloating && handleFloatingFocusOut('right', $event)"
          :class="
            rightFloating
              ? [
                  'floating-drawer absolute inset-y-2 right-2 z-40 flex overflow-hidden rounded-2xl border border-[var(--border-strong)] bg-[var(--bg-surface)]/95 backdrop-blur-md transition-[transform,opacity] duration-200 ease-out',
                  rightDrawerOpen
                    ? 'translate-x-0 opacity-100 pointer-events-auto'
                    : 'translate-x-[calc(100%+18px)] opacity-0 pointer-events-none',
                ]
              : 'relative flex h-full shrink-0'
          "
        >
          <SplitResizer
            :floating="rightFloating"
            side="left"
            :active="activeResizer === 'right'"
            @resize-start="beginResize('right', workspaceRef?.clientWidth)"
            @resize-move="(dx) => updateResize('right', dx)"
            @resize-end="endResize"
            @reset="handleResetPaneWidth('right')"
          />
          <InspectorPane class="min-w-0 flex-1" />
        </div>
      </template>
    </div>

    <!-- 临时粘贴 JSON / JSONL 调试、高亮预览与分类归档弹窗 -->
    <ScratchpadModal />

    <!-- 首次使用配置引导向导 -->
    <SetupWizard />

    <!-- 本机字体、主题与流光动效设置弹窗 -->
    <AppearanceModal />

    <!-- 右下角 Toast 轻提示栈 -->
    <div
      v-if="toasts.length > 0"
      class="pointer-events-none fixed bottom-4 right-4 z-50 flex max-w-md flex-col gap-2"
    >
      <div
        v-for="toast in toasts"
        :key="toast.id"
        class="aurora-toast pointer-events-auto rounded-xl border px-3.5 py-2.5 text-xs font-medium backdrop-blur-md transition-all"
        :class="
          toast.tone === 'success'
            ? 'border-emerald-500/40 bg-emerald-600/95 text-white'
            : toast.tone === 'warning'
              ? 'border-amber-500/40 bg-amber-500/95 text-white'
              : toast.tone === 'error'
                ? 'border-rose-500/40 bg-rose-600/95 text-white'
                : 'border-[var(--border-strong)] bg-[var(--bg-surface)]/95 text-[var(--text-primary)]'
        "
      >
        {{ toast.message }}
      </div>
    </div>
  </div>
</template>
