/**
 * 工作台分栏宽度拖动与 Notion 风格一键悬浮自动隐藏控制器（Workspace Layout Composable）
 *
 * 核心能力：
 * 1. 管理三处可调节分栏宽度（left / right / reports），并在 localStorage 持久化；
 * 2. 结合视口尺寸动态计算防挤压安全上限，避免窄窗下中央内容区被压扁；
 * 3. 实现 Notion 风格「一键悬浮 + 边缘悬停延迟展开 + 移出防抖收起 + 表单输入聚焦防误收」状态机。
 */
import { computed, reactive, ref } from "vue";

export type ResizablePane = "left" | "right" | "reports";

interface PersistedWorkspaceLayout {
  leftWidth: number;
  leftFloating: boolean;
  rightWidth: number;
  rightFloating: boolean;
  reportsListWidth: number;
  reportsListFloating: boolean;
}

interface FloatingRuntimeState {
  hoveredOpen: boolean;
  pointerInside: boolean;
  focusWithin: boolean;
  suppressOpenUntil: number;
  openTimer: number | null;
  closeTimer: number | null;
}

const STORAGE_KEY = "hookreplay.workspace.layout.v1";

export const DEFAULT_PANE_WIDTHS: Record<ResizablePane, number> = {
  left: 288,
  right: 440,
  reports: 320,
};

const PANE_LIMITS: Record<ResizablePane, { min: number; max: number }> = {
  left: { min: 220, max: 520 },
  right: { min: 300, max: 760 },
  reports: { min: 240, max: 560 },
};

const HOVER_OPEN_DELAY_MS = 55;
const HOVER_CLOSE_DELAY_MS = 240;
const TOGGLE_SUPPRESS_MS = 300;
const CENTER_SAFE_MIN_WIDTH = 320;
const REPORTS_DETAIL_SAFE_MIN_WIDTH = 300;

function clampNumber(val: number, min: number, max: number): number {
  if (!Number.isFinite(val)) return min;
  return Math.min(max, Math.max(min, Math.round(val)));
}

function loadInitialLayout(): PersistedWorkspaceLayout {
  const defaults: PersistedWorkspaceLayout = {
    leftWidth: DEFAULT_PANE_WIDTHS.left,
    leftFloating: false,
    rightWidth: DEFAULT_PANE_WIDTHS.right,
    rightFloating: false,
    reportsListWidth: DEFAULT_PANE_WIDTHS.reports,
    reportsListFloating: false,
  };
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") {
    return defaults;
  }
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return defaults;
    const parsed = JSON.parse(raw) as Partial<PersistedWorkspaceLayout>;
    return {
      leftWidth: clampNumber(
        typeof parsed.leftWidth === "number" ? parsed.leftWidth : defaults.leftWidth,
        PANE_LIMITS.left.min,
        PANE_LIMITS.left.max
      ),
      leftFloating: Boolean(parsed.leftFloating),
      rightWidth: clampNumber(
        typeof parsed.rightWidth === "number" ? parsed.rightWidth : defaults.rightWidth,
        PANE_LIMITS.right.min,
        PANE_LIMITS.right.max
      ),
      rightFloating: Boolean(parsed.rightFloating),
      reportsListWidth: clampNumber(
        typeof parsed.reportsListWidth === "number"
          ? parsed.reportsListWidth
          : defaults.reportsListWidth,
        PANE_LIMITS.reports.min,
        PANE_LIMITS.reports.max
      ),
      reportsListFloating: Boolean(parsed.reportsListFloating),
    };
  } catch {
    return defaults;
  }
}

const initial = loadInitialLayout();

const leftWidth = ref<number>(initial.leftWidth);
const leftFloating = ref<boolean>(initial.leftFloating);
const rightWidth = ref<number>(initial.rightWidth);
const rightFloating = ref<boolean>(initial.rightFloating);
const reportsListWidth = ref<number>(initial.reportsListWidth);
const reportsListFloating = ref<boolean>(initial.reportsListFloating);

const viewportWidth = ref<number>(
  typeof window !== "undefined" ? window.innerWidth : 1280
);
const activeResizer = ref<ResizablePane | null>(null);

let resizeStartWidth = DEFAULT_PANE_WIDTHS.left;
let resizeContainerWidth = 0;
let resizeListenerBound = false;

function makeRuntimeState(): FloatingRuntimeState {
  return {
    hoveredOpen: false,
    pointerInside: false,
    focusWithin: false,
    suppressOpenUntil: 0,
    openTimer: null,
    closeTimer: null,
  };
}

const runtime = reactive<Record<ResizablePane, FloatingRuntimeState>>({
  left: makeRuntimeState(),
  right: makeRuntimeState(),
  reports: makeRuntimeState(),
});

function persistLayout(): void {
  if (typeof window === "undefined" || typeof window.localStorage === "undefined") {
    return;
  }
  try {
    const payload: PersistedWorkspaceLayout = {
      leftWidth: leftWidth.value,
      leftFloating: leftFloating.value,
      rightWidth: rightWidth.value,
      rightFloating: rightFloating.value,
      reportsListWidth: reportsListWidth.value,
      reportsListFloating: reportsListFloating.value,
    };
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
  } catch {
    // 忽略无痕模式或存储受限错误
  }
}

function bindViewportResizeListener(): void {
  if (resizeListenerBound || typeof window === "undefined") return;
  viewportWidth.value = window.innerWidth;
  window.addEventListener("resize", () => {
    viewportWidth.value = window.innerWidth;
  });
  resizeListenerBound = true;
}

function clearPaneTimers(pane: ResizablePane): void {
  const st = runtime[pane];
  if (st.openTimer !== null) {
    window.clearTimeout(st.openTimer);
    st.openTimer = null;
  }
  if (st.closeTimer !== null) {
    window.clearTimeout(st.closeTimer);
    st.closeTimer = null;
  }
}

function isFloatingRef(pane: ResizablePane): boolean {
  if (pane === "left") return leftFloating.value;
  if (pane === "right") return rightFloating.value;
  return reportsListFloating.value;
}

function setFloatingRef(pane: ResizablePane, value: boolean): void {
  if (pane === "left") leftFloating.value = value;
  else if (pane === "right") rightFloating.value = value;
  else reportsListFloating.value = value;
}

/** 仅输入框/文本域/下拉框获得焦点时阻止悬浮抽屉收起，避免普通按钮锁住抽屉 */
function isFormInputElement(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    target.isContentEditable
  );
}

/** 根据当前视口与固定侧栏占比，计算左栏安全显示宽度 */
const effectiveLeftWidth = computed<number>(() => {
  const { min, max } = PANE_LIMITS.left;
  if (leftFloating.value) {
    const maxSafe = Math.min(max, Math.max(min, viewportWidth.value - 72));
    return clampNumber(leftWidth.value, min, maxSafe);
  }
  const dockedRight = rightFloating.value
    ? 0
    : clampNumber(rightWidth.value, PANE_LIMITS.right.min, PANE_LIMITS.right.max);
  const maxSafe = Math.min(
    max,
    Math.max(min, viewportWidth.value - dockedRight - CENTER_SAFE_MIN_WIDTH)
  );
  return clampNumber(leftWidth.value, min, maxSafe);
});

/** 根据当前视口与左栏占比，计算右栏安全显示宽度 */
const effectiveRightWidth = computed<number>(() => {
  const { min, max } = PANE_LIMITS.right;
  if (rightFloating.value) {
    const maxSafe = Math.min(max, Math.max(min, viewportWidth.value - 72));
    return clampNumber(rightWidth.value, min, maxSafe);
  }
  const dockedLeft = leftFloating.value ? 0 : effectiveLeftWidth.value;
  const maxSafe = Math.min(
    max,
    Math.max(min, viewportWidth.value - dockedLeft - CENTER_SAFE_MIN_WIDTH)
  );
  return clampNumber(rightWidth.value, min, maxSafe);
});

/** 计算历史报告页左侧报告清单的安全显示宽度 */
const effectiveReportsListWidth = computed<number>(() => {
  const { min, max } = PANE_LIMITS.reports;
  const hostWidth = Math.max(
    min + REPORTS_DETAIL_SAFE_MIN_WIDTH,
    viewportWidth.value - (leftFloating.value ? 0 : effectiveLeftWidth.value)
  );
  if (reportsListFloating.value) {
    const maxSafe = Math.min(max, Math.max(min, hostWidth - 72));
    return clampNumber(reportsListWidth.value, min, maxSafe);
  }
  const maxSafe = Math.min(
    max,
    Math.max(min, hostWidth - REPORTS_DETAIL_SAFE_MIN_WIDTH)
  );
  return clampNumber(reportsListWidth.value, min, maxSafe);
});

const leftDrawerOpen = computed<boolean>(
  () => leftFloating.value && (runtime.left.hoveredOpen || activeResizer.value === "left")
);
const rightDrawerOpen = computed<boolean>(
  () => rightFloating.value && (runtime.right.hoveredOpen || activeResizer.value === "right")
);
const reportsDrawerOpen = computed<boolean>(
  () =>
    reportsListFloating.value &&
    (runtime.reports.hoveredOpen || activeResizer.value === "reports")
);

export function useWorkspaceLayout() {
  bindViewportResizeListener();

  function isDrawerOpen(pane: ResizablePane): boolean {
    if (pane === "left") return leftDrawerOpen.value;
    if (pane === "right") return rightDrawerOpen.value;
    return reportsDrawerOpen.value;
  }

  function openFloatingWithIntent(pane: ResizablePane): void {
    if (!isFloatingRef(pane)) return;
    const st = runtime[pane];
    if (Date.now() < st.suppressOpenUntil) return;
    if (st.closeTimer !== null) {
      window.clearTimeout(st.closeTimer);
      st.closeTimer = null;
    }
    if (st.hoveredOpen || st.openTimer !== null) return;
    st.openTimer = window.setTimeout(() => {
      st.openTimer = null;
      if (isFloatingRef(pane) && Date.now() >= st.suppressOpenUntil) {
        st.hoveredOpen = true;
      }
    }, HOVER_OPEN_DELAY_MS);
  }

  function openFloatingImmediate(pane: ResizablePane): void {
    if (!isFloatingRef(pane)) return;
    clearPaneTimers(pane);
    runtime[pane].suppressOpenUntil = 0;
    runtime[pane].hoveredOpen = true;
  }

  function cancelOpenIntent(pane: ResizablePane): void {
    const st = runtime[pane];
    if (st.openTimer !== null) {
      window.clearTimeout(st.openTimer);
      st.openTimer = null;
    }
  }

  function keepFloatingOpen(pane: ResizablePane): void {
    if (!isFloatingRef(pane)) return;
    const st = runtime[pane];
    if (st.closeTimer !== null) {
      window.clearTimeout(st.closeTimer);
      st.closeTimer = null;
    }
    if (st.openTimer !== null) {
      window.clearTimeout(st.openTimer);
      st.openTimer = null;
    }
    st.pointerInside = true;
    st.hoveredOpen = true;
  }

  function scheduleCloseFloating(pane: ResizablePane): void {
    const st = runtime[pane];
    st.pointerInside = false;
    if (!isFloatingRef(pane)) return;
    if (st.openTimer !== null) {
      window.clearTimeout(st.openTimer);
      st.openTimer = null;
    }
    if (activeResizer.value === pane || st.focusWithin) {
      return;
    }
    if (st.closeTimer !== null) {
      window.clearTimeout(st.closeTimer);
    }
    st.closeTimer = window.setTimeout(() => {
      st.closeTimer = null;
      if (!st.pointerInside && !st.focusWithin && activeResizer.value !== pane) {
        st.hoveredOpen = false;
      }
    }, HOVER_CLOSE_DELAY_MS);
  }

  function handleFloatingFocusIn(pane: ResizablePane, ev: FocusEvent): void {
    if (!isFloatingRef(pane)) return;
    if (!isFormInputElement(ev.target)) return;
    const st = runtime[pane];
    st.focusWithin = true;
    if (st.closeTimer !== null) {
      window.clearTimeout(st.closeTimer);
      st.closeTimer = null;
    }
  }

  function handleFloatingFocusOut(pane: ResizablePane, ev: FocusEvent): void {
    const st = runtime[pane];
    const currentEl = ev.currentTarget as HTMLElement | null;
    const nextEl = ev.relatedTarget as Node | null;
    const stillInsideInput =
      Boolean(nextEl && currentEl && currentEl.contains(nextEl)) &&
      isFormInputElement(nextEl);
    st.focusWithin = stillInsideInput;
    if (
      !stillInsideInput &&
      isFloatingRef(pane) &&
      !st.pointerInside &&
      activeResizer.value !== pane
    ) {
      scheduleCloseFloating(pane);
    }
  }

  /** 切换某侧栏的「悬浮 / 固定」模式，返回切换后的是否为悬浮状态 */
  function toggleFloating(pane: ResizablePane): boolean {
    const nextFloating = !isFloatingRef(pane);
    clearPaneTimers(pane);
    const st = runtime[pane];
    st.hoveredOpen = false;
    st.pointerInside = false;
    st.focusWithin = false;
    st.suppressOpenUntil = nextFloating ? Date.now() + TOGGLE_SUPPRESS_MS : 0;

    if (
      nextFloating &&
      typeof document !== "undefined" &&
      document.activeElement instanceof HTMLElement
    ) {
      document.activeElement.blur();
    }

    setFloatingRef(pane, nextFloating);
    persistLayout();
    return nextFloating;
  }

  /** 立即收起全部已打开的悬浮侧栏 */
  function closeAllFloatingDrawers(): void {
    (["left", "right", "reports"] as ResizablePane[]).forEach((pane) => {
      clearPaneTimers(pane);
      runtime[pane].hoveredOpen = false;
      runtime[pane].pointerInside = false;
      runtime[pane].focusWithin = false;
    });
  }

  /** 开始拖动指定分栏分隔线 */
  function beginResize(pane: ResizablePane, containerWidth?: number): void {
    activeResizer.value = pane;
    resizeContainerWidth = containerWidth ?? 0;
    if (pane === "left") {
      resizeStartWidth = effectiveLeftWidth.value;
    } else if (pane === "right") {
      resizeStartWidth = effectiveRightWidth.value;
    } else {
      resizeStartWidth = effectiveReportsListWidth.value;
    }
    if (typeof document !== "undefined") {
      document.body.classList.add("is-resizing-pane");
    }
  }

  /** 根据拖动位移实时更新分栏宽度 */
  function updateResize(pane: ResizablePane, deltaX: number): void {
    if (activeResizer.value !== pane) return;
    const { min, max } = PANE_LIMITS[pane];
    const signedDelta = pane === "right" ? -deltaX : deltaX;
    const rawTarget = resizeStartWidth + signedDelta;

    if (pane === "left") {
      const totalWidth =
        resizeContainerWidth > 0 ? resizeContainerWidth : viewportWidth.value;
      const maxSafe = leftFloating.value
        ? Math.min(max, Math.max(min, totalWidth - 72))
        : Math.min(
            max,
            Math.max(
              min,
              totalWidth - (rightFloating.value ? 0 : effectiveRightWidth.value) - CENTER_SAFE_MIN_WIDTH
            )
          );
      leftWidth.value = clampNumber(rawTarget, min, maxSafe);
    } else if (pane === "right") {
      const totalWidth =
        resizeContainerWidth > 0 ? resizeContainerWidth : viewportWidth.value;
      const maxSafe = rightFloating.value
        ? Math.min(max, Math.max(min, totalWidth - 72))
        : Math.min(
            max,
            Math.max(
              min,
              totalWidth - (leftFloating.value ? 0 : effectiveLeftWidth.value) - CENTER_SAFE_MIN_WIDTH
            )
          );
      rightWidth.value = clampNumber(rawTarget, min, maxSafe);
    } else {
      const hostWidth =
        resizeContainerWidth > 0
          ? resizeContainerWidth
          : Math.max(
              min + REPORTS_DETAIL_SAFE_MIN_WIDTH,
              viewportWidth.value - (leftFloating.value ? 0 : effectiveLeftWidth.value)
            );
      const maxSafe = reportsListFloating.value
        ? Math.min(max, Math.max(min, hostWidth - 72))
        : Math.min(max, Math.max(min, hostWidth - REPORTS_DETAIL_SAFE_MIN_WIDTH));
      reportsListWidth.value = clampNumber(rawTarget, min, maxSafe);
    }
  }

  /** 结束拖动并持久化宽度 */
  function endResize(): void {
    const pane = activeResizer.value;
    activeResizer.value = null;
    if (typeof document !== "undefined") {
      document.body.classList.remove("is-resizing-pane");
    }
    persistLayout();
    if (pane && isFloatingRef(pane)) {
      const st = runtime[pane];
      if (!st.pointerInside && !st.focusWithin) {
        scheduleCloseFloating(pane);
      }
    }
  }

  /** 双击分隔线恢复指定分栏的默认宽度 */
  function resetPaneWidth(pane: ResizablePane): void {
    if (pane === "left") leftWidth.value = DEFAULT_PANE_WIDTHS.left;
    else if (pane === "right") rightWidth.value = DEFAULT_PANE_WIDTHS.right;
    else reportsListWidth.value = DEFAULT_PANE_WIDTHS.reports;
    persistLayout();
  }

  return {
    leftWidth,
    leftFloating,
    rightWidth,
    rightFloating,
    reportsListWidth,
    reportsListFloating,
    effectiveLeftWidth,
    effectiveRightWidth,
    effectiveReportsListWidth,
    leftDrawerOpen,
    rightDrawerOpen,
    reportsDrawerOpen,
    activeResizer,
    isDrawerOpen,
    openFloatingWithIntent,
    openFloatingImmediate,
    cancelOpenIntent,
    keepFloatingOpen,
    scheduleCloseFloating,
    handleFloatingFocusIn,
    handleFloatingFocusOut,
    toggleFloating,
    closeAllFloatingDrawers,
    beginResize,
    updateResize,
    endResize,
    resetPaneWidth,
  };
}
