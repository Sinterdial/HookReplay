<script setup lang="ts">
/**
 * 外观、本机字体与流光动效设置弹窗（AppearanceModal）
 *
 * 功能：
 * 1. 切换「跟随系统 / 浅色 / 深色」三态主题；
 * 2. 枚举本机已安装字体，支持关键词搜索与「仅看等宽字体」过滤，分别设置界面正文字体与代码等宽字体；
 * 3. 调节全局基础字号（12px–18px）与 Gemini 流光动效强度（丰富 / 轻量 / 关闭）；
 * 4. 实时所见即所得预览中文界面排版与 JSON 语法高亮效果。
 */
import { computed, onMounted, ref, watch } from "vue";
import {
  Check,
  Minus,
  Monitor,
  Moon,
  Palette,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Sparkles,
  Sun,
  Type,
  X,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useTheme } from "../composables/useTheme";
import type { AuroraIntensity, ThemeMode } from "../types";
import { highlightJsonText } from "../utils/jsonHighlight";

const { showAppearanceModal, applySnapshot, pushToast } = useAppState();
const {
  themeMode,
  uiFont,
  monoFont,
  fontSize,
  codeFontSize,
  auroraIntensity,
  systemFonts,
  isLoadingFonts,
  setTheme,
  loadSystemFonts,
  saveAppearanceSettings,
  resetFontsToDefault,
} = useTheme();

const draftTheme = ref<ThemeMode>("system");
const draftUiFont = ref<string>("");
const draftMonoFont = ref<string>("");
const draftFontSize = ref<number>(14);
const draftCodeFontSize = ref<number>(13);
const draftAurora = ref<AuroraIntensity>("rich");

const initialSnapshot = ref<{
  theme: ThemeMode;
  uiFont: string;
  monoFont: string;
  fontSize: number;
  codeFontSize: number;
  auroraIntensity: AuroraIntensity;
} | null>(null);

const uiFontSearch = ref("");
const monoFontSearch = ref("");
const onlyMonospaceForMono = ref(true);
const isSaving = ref(false);

const PREVIEW_JSON = JSON.stringify(
  {
    type: "webhook.event",
    title: "示例通知事件 · Order #1024",
    status: 200,
    deduped: false,
    __test: true,
  },
  null,
  2
);

const previewHtml = computed(() =>
  highlightJsonText(PREVIEW_JSON, { highlightImportantKeys: true }).html
);

const filteredUiFonts = computed(() => {
  const q = uiFontSearch.value.trim().toLowerCase();
  return systemFonts.value.filter((f) =>
    q ? f.name.toLowerCase().includes(q) : true
  );
});

const filteredMonoFonts = computed(() => {
  const q = monoFontSearch.value.trim().toLowerCase();
  return systemFonts.value.filter((f) => {
    if (onlyMonospaceForMono.value && !f.isMonospace) return false;
    if (q && !f.name.toLowerCase().includes(q)) return false;
    return true;
  });
});

function syncDrafts(): void {
  initialSnapshot.value = {
    theme: themeMode.value,
    uiFont: uiFont.value,
    monoFont: monoFont.value,
    fontSize: fontSize.value,
    codeFontSize: codeFontSize.value,
    auroraIntensity: auroraIntensity.value,
  };
  draftTheme.value = themeMode.value;
  draftUiFont.value = uiFont.value;
  draftMonoFont.value = monoFont.value;
  draftFontSize.value = fontSize.value;
  draftCodeFontSize.value = codeFontSize.value;
  draftAurora.value = auroraIntensity.value;
}

watch(showAppearanceModal, (open) => {
  if (open) {
    syncDrafts();
    void loadSystemFonts(false);
  }
});

watch(
  [
    draftTheme,
    draftUiFont,
    draftMonoFont,
    draftFontSize,
    draftCodeFontSize,
    draftAurora,
  ],
  () => {
    if (!showAppearanceModal.value) return;
    themeMode.value = draftTheme.value;
    uiFont.value = draftUiFont.value;
    monoFont.value = draftMonoFont.value;
    fontSize.value = Math.min(22, Math.max(11, Math.round(draftFontSize.value || 14)));
    codeFontSize.value = Math.min(
      24,
      Math.max(10, Math.round(draftCodeFontSize.value || 13))
    );
    auroraIntensity.value = draftAurora.value;
  }
);

onMounted(() => {
  if (showAppearanceModal.value) {
    syncDrafts();
    void loadSystemFonts(false);
  }
});

function closeWithoutSave(): void {
  if (initialSnapshot.value) {
    themeMode.value = initialSnapshot.value.theme;
    uiFont.value = initialSnapshot.value.uiFont;
    monoFont.value = initialSnapshot.value.monoFont;
    fontSize.value = initialSnapshot.value.fontSize;
    codeFontSize.value = initialSnapshot.value.codeFontSize;
    auroraIntensity.value = initialSnapshot.value.auroraIntensity;
  }
  initialSnapshot.value = null;
  showAppearanceModal.value = false;
}

function previewFontStyle(font: string, isMono: boolean): Record<string, string> {
  const fallback = isMono ? "var(--font-mono-default)" : "var(--font-sans-default)";
  const trimmed = font.trim();
  const escaped = trimmed.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return {
    fontFamily: trimmed ? `"${escaped}", ${fallback}` : fallback,
  };
}

function clampDraftUiSize(next: number): void {
  draftFontSize.value = Math.min(22, Math.max(11, Math.round(next)));
}

function clampDraftCodeSize(next: number): void {
  draftCodeFontSize.value = Math.min(24, Math.max(10, Math.round(next)));
}

async function handleSave(): Promise<void> {
  isSaving.value = true;
  try {
    await setTheme(draftTheme.value);
    const snap = await saveAppearanceSettings({
      uiFont: draftUiFont.value,
      monoFont: draftMonoFont.value,
      fontSize: draftFontSize.value,
      codeFontSize: draftCodeFontSize.value,
      auroraIntensity: draftAurora.value,
    });
    if (snap) {
      applySnapshot(snap);
    }
    initialSnapshot.value = null;
    pushToast("外观、字体与字号设置已保存并立即生效", "success");
    showAppearanceModal.value = false;
  } finally {
    isSaving.value = false;
  }
}

async function handleResetFonts(): Promise<void> {
  draftUiFont.value = "";
  draftMonoFont.value = "";
  draftFontSize.value = 14;
  draftCodeFontSize.value = 13;
  const snap = await resetFontsToDefault();
  if (snap) {
    applySnapshot(snap);
  }
  initialSnapshot.value = {
    theme: themeMode.value,
    uiFont: "",
    monoFont: "",
    fontSize: 14,
    codeFontSize: 13,
    auroraIntensity: auroraIntensity.value,
  };
  pushToast("已恢复系统默认字体与标准字号（14px / 13px）", "info");
}
</script>

<template>
  <div
    v-if="showAppearanceModal"
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-4 backdrop-blur-xs"
    @click.self="closeWithoutSave"
  >
    <div
      class="aurora-border-active flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl bg-[var(--bg-surface)] shadow-2xl"
    >
      <!-- 头部 -->
      <div
        class="flex items-center justify-between border-b border-[var(--border-subtle)] px-5 py-3.5"
      >
        <div class="flex items-center gap-2.5">
          <div
            class="aurora-ring-soft flex h-8 w-8 items-center justify-center rounded-lg bg-[var(--bg-elevated)] text-indigo-500"
          >
            <Palette class="h-4 w-4" />
          </div>
          <div>
            <h3 class="text-sm font-bold text-[var(--text-primary)]">
              外观主题、本机字体与流光动效设置
            </h3>
            <p class="text-[0.6875rem] text-[var(--text-muted)]">
              已扫描到本机 {{ systemFonts.length }} 款字体 · 支持主界面实时预览（取消自动还原）与 Ctrl +/-/0 快捷缩放
            </p>
          </div>
        </div>

        <button
          type="button"
          @click="closeWithoutSave"
          class="rounded-md p-1 text-[var(--text-muted)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- 主体滚动区 -->
      <div class="min-h-0 flex-1 overflow-y-auto p-5 space-y-5">
        <!-- 1. 主题模式与流光强度 -->
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div class="space-y-2">
            <div class="text-xs font-semibold text-[var(--text-secondary)]">
              外观主题模式
            </div>
            <div class="grid grid-cols-3 gap-2">
              <button
                v-for="opt in [
                  { id: 'system', label: '跟随系统', icon: Monitor },
                  { id: 'light', label: '浅色模式', icon: Sun },
                  { id: 'dark', label: '深色模式', icon: Moon },
                ]"
                :key="opt.id"
                type="button"
                @click="draftTheme = opt.id as ThemeMode"
                class="aurora-btn-subtle flex items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-medium transition"
                :class="
                  draftTheme === opt.id
                    ? 'border-indigo-500 bg-indigo-500/12 text-indigo-600 dark:text-indigo-300'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-secondary)]'
                "
              >
                <component :is="opt.icon" class="h-3.5 w-3.5" />
                <span>{{ opt.label }}</span>
              </button>
            </div>
          </div>

          <div class="space-y-2">
            <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-secondary)]">
              <Sparkles class="h-3.5 w-3.5 text-purple-500" />
              <span>Gemini 流光动效强度</span>
            </div>
            <div class="grid grid-cols-3 gap-2">
              <button
                v-for="opt in [
                  { id: 'rich', label: '丰富流光' },
                  { id: 'subtle', label: '轻量流光' },
                  { id: 'off', label: '关闭动效' },
                ]"
                :key="opt.id"
                type="button"
                @click="draftAurora = opt.id as AuroraIntensity"
                class="aurora-btn-subtle flex items-center justify-center gap-1 rounded-xl border px-3 py-2 text-xs font-medium transition"
                :class="
                  draftAurora === opt.id
                    ? 'border-purple-500 bg-purple-500/12 text-purple-600 dark:text-purple-300'
                    : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-secondary)]'
                "
              >
                <span>{{ opt.label }}</span>
              </button>
            </div>
          </div>
        </div>

        <!-- 2. 本机正文字体 & 代码等宽字体选择器 -->
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <!-- UI 正文字体 -->
          <div class="flex flex-col rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50 p-3 space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-primary)]">
                <Type class="h-3.5 w-3.5 text-blue-500" />
                <span>界面正文字体 (UI Font)</span>
              </div>
              <button
                type="button"
                @click="loadSystemFonts(true)"
                title="重新扫描本机已安装字体"
                class="inline-flex items-center gap-1 text-[0.6875rem] text-[var(--text-muted)] hover:text-[var(--text-primary)]"
              >
                <RefreshCw class="h-3 w-3" :class="{ 'animate-spin': isLoadingFonts }" />
                <span>刷新</span>
              </button>
            </div>

            <div class="relative flex items-center">
              <Search class="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[var(--text-muted)]" />
              <input
                v-model="uiFontSearch"
                type="text"
                placeholder="搜索本机正文字体（如 Microsoft YaHei / PingFang）…"
                class="h-7 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] pl-8 pr-3 text-xs outline-none focus:border-blue-500"
              />
            </div>

            <div class="h-44 overflow-y-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] divide-y divide-[var(--border-subtle)]/60">
              <button
                type="button"
                @click="draftUiFont = ''"
                class="flex w-full items-center justify-between px-3 py-1.5 text-left text-xs transition hover:bg-[var(--bg-elevated)]"
                :class="draftUiFont === '' ? 'bg-blue-500/12 font-semibold text-blue-600 dark:text-blue-300' : ''"
              >
                <span>系统默认字体栈 (Segoe UI / 微软雅黑)</span>
                <Check v-if="draftUiFont === ''" class="h-3.5 w-3.5" />
              </button>
              <button
                v-for="font in filteredUiFonts"
                :key="'ui-' + font.name"
                type="button"
                @click="draftUiFont = font.name"
                class="flex w-full items-center justify-between px-3 py-1.5 text-left text-xs transition hover:bg-[var(--bg-elevated)]"
                :class="draftUiFont === font.name ? 'bg-blue-500/12 font-semibold text-blue-600 dark:text-blue-300' : ''"
              >
                <span class="truncate pr-2" :style="previewFontStyle(font.name, false)">
                  {{ font.name }}
                </span>
                <div class="flex shrink-0 items-center gap-1.5">
                  <span
                    v-if="font.isMonospace"
                    class="rounded bg-[var(--bg-elevated)] px-1 py-0.5 text-[0.5625rem] text-[var(--text-muted)]"
                  >
                    等宽
                  </span>
                  <Check v-if="draftUiFont === font.name" class="h-3.5 w-3.5 text-blue-500" />
                </div>
              </button>
            </div>
          </div>

          <!-- 代码与 JSON 等宽字体 -->
          <div class="flex flex-col rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50 p-3 space-y-2">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--text-primary)]">
                <Type class="h-3.5 w-3.5 text-emerald-500" />
                <span>JSON / 代码等宽字体 (Mono Font)</span>
              </div>
              <label class="flex cursor-pointer items-center gap-1 text-[0.6875rem] text-[var(--text-secondary)]">
                <input
                  v-model="onlyMonospaceForMono"
                  type="checkbox"
                  class="h-3 w-3 rounded accent-emerald-600"
                />
                <span>仅看等宽</span>
              </label>
            </div>

            <div class="relative flex items-center">
              <Search class="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[var(--text-muted)]" />
              <input
                v-model="monoFontSearch"
                type="text"
                placeholder="搜索等宽字体（如 Cascadia Code / Consolas / Sarasa）…"
                class="h-7 w-full rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] pl-8 pr-3 text-xs outline-none focus:border-emerald-500"
              />
            </div>

            <div class="h-44 overflow-y-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] divide-y divide-[var(--border-subtle)]/60">
              <button
                type="button"
                @click="draftMonoFont = ''"
                class="flex w-full items-center justify-between px-3 py-1.5 text-left text-xs transition hover:bg-[var(--bg-elevated)]"
                :class="draftMonoFont === '' ? 'bg-emerald-500/12 font-semibold text-emerald-600 dark:text-emerald-300' : ''"
              >
                <span class="font-mono">默认等宽栈 (Cascadia / Consolas)</span>
                <Check v-if="draftMonoFont === ''" class="h-3.5 w-3.5" />
              </button>
              <button
                v-for="font in filteredMonoFonts"
                :key="'mono-' + font.name"
                type="button"
                @click="draftMonoFont = font.name"
                class="flex w-full items-center justify-between px-3 py-1.5 text-left text-xs transition hover:bg-[var(--bg-elevated)]"
                :class="draftMonoFont === font.name ? 'bg-emerald-500/12 font-semibold text-emerald-600 dark:text-emerald-300' : ''"
              >
                <span class="truncate pr-2" :style="previewFontStyle(font.name, true)">
                  {{ font.name }}
                </span>
                <Check v-if="draftMonoFont === font.name" class="h-3.5 w-3.5 shrink-0 text-emerald-500" />
              </button>
            </div>
          </div>
        </div>

        <!-- 3. 界面基准字号 & 代码字号双控制 -->
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <!-- 左卡：界面基准字号 -->
          <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/40 p-3.5 space-y-2.5">
            <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
              <div class="flex items-center gap-1.5 font-semibold text-[var(--text-secondary)]">
                <span>界面基准字号：</span>
                <span class="rounded bg-blue-500/15 px-1.5 py-0.5 font-mono text-blue-600 dark:text-blue-300">
                  {{ draftFontSize }}px
                </span>
              </div>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  :disabled="draftFontSize <= 11"
                  @click="clampDraftUiSize(draftFontSize - 1)"
                  title="缩小界面字号"
                  class="flex h-6 w-6 items-center justify-center rounded border border-[var(--border-subtle)] bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
                >
                  <Minus class="h-3 w-3" />
                </button>
                <button
                  type="button"
                  :disabled="draftFontSize >= 22"
                  @click="clampDraftUiSize(draftFontSize + 1)"
                  title="放大界面字号"
                  class="flex h-6 w-6 items-center justify-center rounded border border-[var(--border-subtle)] bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
                >
                  <Plus class="h-3 w-3" />
                </button>
              </div>
            </div>

            <input
              v-model.number="draftFontSize"
              type="range"
              min="11"
              max="22"
              step="1"
              class="w-full accent-indigo-600"
            />

            <div class="flex flex-wrap items-center justify-between gap-1.5">
              <span class="text-[0.625rem] text-[var(--text-muted)]">快捷预设 (11-22px)：</span>
              <div class="flex flex-wrap items-center gap-1">
                <button
                  v-for="sz in [12, 13, 14, 16, 18, 20]"
                  :key="'ui-' + sz"
                  type="button"
                  @click="draftFontSize = sz"
                  class="rounded-md px-2 py-0.5 font-mono text-[0.6875rem] transition"
                  :class="
                    draftFontSize === sz
                      ? 'bg-blue-600 text-white'
                      : 'bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
                  "
                >
                  {{ sz }}
                </button>
              </div>
            </div>
          </div>

          <!-- 右卡：代码 / JSON 独立字号 -->
          <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/40 p-3.5 space-y-2.5">
            <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
              <div class="flex items-center gap-1.5 font-semibold text-[var(--text-secondary)]">
                <span>代码 / JSON 字号：</span>
                <span class="rounded bg-emerald-500/15 px-1.5 py-0.5 font-mono text-emerald-600 dark:text-emerald-300">
                  {{ draftCodeFontSize }}px
                </span>
              </div>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  :disabled="draftCodeFontSize <= 10"
                  @click="clampDraftCodeSize(draftCodeFontSize - 1)"
                  title="缩小代码字号"
                  class="flex h-6 w-6 items-center justify-center rounded border border-[var(--border-subtle)] bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
                >
                  <Minus class="h-3 w-3" />
                </button>
                <button
                  type="button"
                  :disabled="draftCodeFontSize >= 24"
                  @click="clampDraftCodeSize(draftCodeFontSize + 1)"
                  title="放大代码字号"
                  class="flex h-6 w-6 items-center justify-center rounded border border-[var(--border-subtle)] bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] disabled:opacity-40"
                >
                  <Plus class="h-3 w-3" />
                </button>
              </div>
            </div>

            <input
              v-model.number="draftCodeFontSize"
              type="range"
              min="10"
              max="24"
              step="1"
              class="w-full accent-emerald-600"
            />

            <div class="flex flex-wrap items-center justify-between gap-1.5">
              <span class="text-[0.625rem] text-[var(--text-muted)]">快捷预设 (10-24px)：</span>
              <div class="flex flex-wrap items-center gap-1">
                <button
                  v-for="sz in [11, 12, 13, 15, 17, 20]"
                  :key="'code-' + sz"
                  type="button"
                  @click="draftCodeFontSize = sz"
                  class="rounded-md px-2 py-0.5 font-mono text-[0.6875rem] transition"
                  :class="
                    draftCodeFontSize === sz
                      ? 'bg-emerald-600 text-white'
                      : 'bg-[var(--bg-surface)] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]'
                  "
                >
                  {{ sz }}
                </button>
              </div>
            </div>
          </div>
        </div>

        <div class="text-[0.6875rem] text-[var(--text-muted)]">
          提示：全界面采用相对缩放比例，调整时会同步实时应用到左侧说明、列表、顶部控件与 JSON 编辑器；日常可用 <code class="rounded bg-[var(--bg-elevated)] px-1 font-mono">Ctrl +</code> / <code class="rounded bg-[var(--bg-elevated)] px-1 font-mono">Ctrl -</code> / <code class="rounded bg-[var(--bg-elevated)] px-1 font-mono">Ctrl 0</code> 快速缩放。
        </div>

        <!-- 4. 所见即所得实时预览区 -->
        <div class="rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/55 p-3.5 space-y-2">
          <div class="text-[0.6875rem] font-semibold text-[var(--text-muted)]">
            字体与 JSON 高亮实时预览（正文：{{ draftUiFont || "系统默认" }} · 界面 {{ draftFontSize }}px · 等宽：{{ draftMonoFont || "默认等宽" }} · 代码 {{ draftCodeFontSize }}px）
          </div>
          <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
            <div
              class="rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-3 space-y-1"
              :style="{
                ...previewFontStyle(draftUiFont, false),
                fontSize: `${draftFontSize}px`,
              }"
            >
              <div class="font-bold text-[var(--text-primary)]">
                HookReplay 中文排版与混合数字 12345
              </div>
              <p class="text-[0.88em] text-[var(--text-secondary)]">
                支持将本机任意已安装的中文字体与等宽代码字体自由组合，打造最顺眼的测试工作台。
              </p>
            </div>

            <!-- eslint-disable-next-line vue/no-v-html -->
            <pre
              class="m-0 overflow-x-auto rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] p-3 leading-relaxed"
              :style="{
                ...previewFontStyle(draftMonoFont, true),
                fontSize: `${draftCodeFontSize}px`,
              }"
              v-html="previewHtml"
            />
          </div>
        </div>
      </div>

      <!-- 底部动作栏 -->
      <div
        class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--border-subtle)] px-5 py-3"
      >
        <button
          type="button"
          @click="handleResetFonts"
          class="aurora-btn-subtle inline-flex items-center gap-1.5 rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-1.5 text-xs font-medium text-[var(--text-secondary)] hover:text-[var(--text-primary)]"
        >
          <RotateCcw class="h-3.5 w-3.5" />
          <span>恢复默认字体与字号</span>
        </button>

        <div class="flex items-center gap-2">
          <button
            type="button"
            @click="closeWithoutSave"
            class="rounded-lg border border-[var(--border-subtle)] px-3.5 py-1.5 text-xs font-medium text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)]"
          >
            取消
          </button>
          <button
            type="button"
            :disabled="isSaving"
            @click="handleSave"
            class="aurora-btn rounded-lg px-4 py-1.5 text-xs font-semibold text-white disabled:opacity-50"
          >
            {{ isSaving ? "保存中…" : "保存并应用" }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
