<script setup lang="ts">
/**
 * 通用 JSON / JSONL 语法高亮与关键词突显检视器（JsonViewer）
 *
 * 特性：
 * 1. 基于 `highlightJsonText` 实现 XSS 安全的词法着色（区分键名/重点字段/值类型/注释）；
 * 2. 内置关键词搜索高亮与上一个/下一个命中定位滚动；
 * 3. 支持「突显关键字段」开关、自动换行开关、行号对齐与大文本卡顿保护。
 */
import { computed, nextTick, ref, watch } from "vue";
import {
  Check,
  ChevronDown,
  ChevronUp,
  Copy,
  Search,
  Sparkles,
  WrapText,
  X,
} from "lucide-vue-next";
import { highlightJsonText, tryPrettyJson } from "../utils/jsonHighlight";

const props = withDefaults(
  defineProps<{
    code: string;
    emptyText?: string;
    maxHeight?: string;
    showToolbar?: boolean;
    showLineNumbers?: boolean;
    autoPretty?: boolean;
  }>(),
  {
    emptyText: "暂无内容",
    maxHeight: "",
    showToolbar: true,
    showLineNumbers: true,
    autoPretty: false,
  }
);

const emit = defineEmits<{
  (e: "dblclick-code"): void;
}>();

const searchKeyword = ref("");
const activeHitIndex = ref(0);
const highlightImportantKeys = ref(true);
const wrapLines = ref(true);
const copied = ref(false);
const forceFullPlain = ref(false);
const scrollBoxRef = ref<HTMLElement | null>(null);

const LARGE_HIGHLIGHT_LIMIT = 300_000;

const displayCode = computed<string>(() => {
  if (!props.code) return "";
  if (props.autoPretty) {
    const pretty = tryPrettyJson(props.code);
    return pretty.valid ? pretty.text : props.code;
  }
  return props.code;
});

const isOversized = computed<boolean>(
  () => displayCode.value.length > LARGE_HIGHLIGHT_LIMIT
);

const highlighted = computed(() => {
  const source =
    isOversized.value && !forceFullPlain.value
      ? displayCode.value.slice(0, LARGE_HIGHLIGHT_LIMIT)
      : displayCode.value;

  return highlightJsonText(source, {
    highlightImportantKeys: highlightImportantKeys.value,
    searchKeyword: searchKeyword.value,
    activeHitIndex: activeHitIndex.value,
  });
});

const highlightedLines = computed<string[]>(() => {
  if (!highlighted.value.html) return [];
  return highlighted.value.html.split("\n");
});

const useRowLayout = computed<boolean>(
  () => props.showLineNumbers && highlightedLines.value.length <= 2500
);

watch(
  () => [searchKeyword.value, props.code],
  () => {
    activeHitIndex.value = 0;
  }
);

function scrollToCurrentHit(): void {
  void nextTick(() => {
    const container = scrollBoxRef.value;
    if (!container) return;
    const hitEl = container.querySelector<HTMLElement>(
      `mark[data-hit-index="${activeHitIndex.value}"]`
    );
    if (hitEl) {
      hitEl.scrollIntoView({ block: "center", behavior: "smooth" });
    }
  });
}

function jumpHit(delta: number): void {
  const total = highlighted.value.matchCount;
  if (total <= 0) return;
  activeHitIndex.value = (activeHitIndex.value + delta + total) % total;
  scrollToCurrentHit();
}

async function copyCode(): Promise<void> {
  if (!displayCode.value) return;
  try {
    await navigator.clipboard.writeText(displayCode.value);
    copied.value = true;
    window.setTimeout(() => {
      copied.value = false;
    }, 1500);
  } catch {
    // 忽略剪贴板权限异常
  }
}
</script>

<template>
  <div class="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/65">
    <!-- 轻量工具栏：搜索高亮 + 关键字段突显 + 换行 + 复制 -->
    <div
      v-if="showToolbar"
      class="flex flex-wrap items-center justify-between gap-1.5 border-b border-[var(--border-subtle)] bg-[var(--bg-surface)]/80 px-2.5 py-1.5"
    >
      <div class="flex min-w-[140px] flex-1 items-center gap-1">
        <div class="relative flex flex-1 items-center max-w-[230px]">
          <Search class="pointer-events-none absolute left-2 h-3 w-3 text-[var(--text-muted)]" />
          <input
            v-model="searchKeyword"
            type="text"
            placeholder="高亮搜索关键词…"
            @keydown.enter.prevent="jumpHit(1)"
            class="h-6 w-full rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] pl-6 pr-5 text-[0.6875rem] text-[var(--text-primary)] outline-none focus:border-indigo-500"
          />
          <button
            v-if="searchKeyword"
            type="button"
            @click="searchKeyword = ''"
            class="absolute right-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)]"
          >
            <X class="h-3 w-3" />
          </button>
        </div>

        <template v-if="searchKeyword.trim()">
          <span class="font-mono text-[0.625rem] text-[var(--text-muted)]">
            {{
              highlighted.matchCount > 0
                ? `${activeHitIndex + 1}/${highlighted.matchCount}`
                : "0/0"
            }}
          </span>
          <button
            type="button"
            :disabled="highlighted.matchCount === 0"
            @click="jumpHit(-1)"
            title="上一个匹配"
            class="rounded p-0.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] disabled:opacity-35"
          >
            <ChevronUp class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            :disabled="highlighted.matchCount === 0"
            @click="jumpHit(1)"
            title="下一个匹配"
            class="rounded p-0.5 text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] disabled:opacity-35"
          >
            <ChevronDown class="h-3.5 w-3.5" />
          </button>
        </template>
      </div>

      <div class="flex items-center gap-1">
        <button
          type="button"
          @click="highlightImportantKeys = !highlightImportantKeys"
          title="突显 type / title / status / deduped / __test 等重点字段"
          class="aurora-pill inline-flex h-6 items-center gap-1 rounded-md border px-1.5 text-[0.625rem] font-medium transition"
          :class="
            highlightImportantKeys
              ? 'border-violet-500/40 bg-violet-500/15 text-violet-700 dark:text-violet-300'
              : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-muted)]'
          "
        >
          <Sparkles class="h-3 w-3" />
          <span>突显关键词</span>
        </button>

        <button
          type="button"
          @click="wrapLines = !wrapLines"
          title="切换长行自动换行"
          class="inline-flex h-6 items-center gap-1 rounded-md border px-1.5 text-[0.625rem] font-medium transition"
          :class="
            wrapLines
              ? 'border-blue-500/40 bg-blue-500/12 text-blue-700 dark:text-blue-300'
              : 'border-[var(--border-subtle)] bg-[var(--bg-elevated)] text-[var(--text-muted)]'
          "
        >
          <WrapText class="h-3 w-3" />
          <span>换行</span>
        </button>

        <button
          type="button"
          :disabled="!displayCode"
          @click="copyCode"
          title="复制内容"
          class="inline-flex h-6 items-center gap-1 rounded-md border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-1.5 text-[0.625rem] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] disabled:opacity-40"
        >
          <Check v-if="copied" class="h-3 w-3 text-emerald-500" />
          <Copy v-else class="h-3 w-3" />
          <span>复制</span>
        </button>
      </div>
    </div>

    <!-- 超大文本防卡顿提示条 -->
    <div
      v-if="isOversized && !forceFullPlain"
      class="flex items-center justify-between border-b border-amber-500/25 bg-amber-500/10 px-2.5 py-1 text-[0.625rem] text-amber-700 dark:text-amber-300"
    >
      <span>内容超过 300KB，已仅高亮前 300KB 以保证流畅度</span>
      <button
        type="button"
        @click="forceFullPlain = true"
        class="font-semibold underline"
      >
        强制渲染全文
      </button>
    </div>

    <!-- 高亮代码滚动主体 -->
    <div
      ref="scrollBoxRef"
      @dblclick="emit('dblclick-code')"
      :style="maxHeight ? { maxHeight } : undefined"
      class="code-editor-text min-h-0 flex-1 overflow-auto p-2.5 text-[var(--text-primary)]"
    >
      <div
        v-if="!displayCode"
        class="flex h-full min-h-[80px] items-center justify-center text-xs text-[var(--text-muted)]"
      >
        {{ emptyText }}
      </div>

      <!-- 行号对齐分栏布局（支持自动换行时行号依然精准对齐） -->
      <div v-else-if="useRowLayout" class="min-w-full">
        <div
          v-for="(lineHtml, idx) in highlightedLines"
          :key="idx"
          class="flex items-start gap-2.5 rounded px-1 hover:bg-[var(--bg-hover)]/35"
        >
          <span
            class="w-7 shrink-0 select-none text-right text-[0.85em] text-[var(--text-muted)]/75"
          >
            {{ idx + 1 }}
          </span>
          <!-- eslint-disable-next-line vue/no-v-html -->
          <span
            class="min-w-0 flex-1"
            :class="wrapLines ? 'whitespace-pre-wrap break-all' : 'whitespace-pre'"
            v-html="lineHtml || ' '"
          />
        </div>
      </div>

      <!-- 超多行高性能整块高亮 -->
      <!-- eslint-disable-next-line vue/no-v-html -->
      <pre
        v-else
        class="code-editor-text m-0"
        :class="wrapLines ? 'whitespace-pre-wrap break-all' : 'whitespace-pre'"
        v-html="highlighted.html"
      />
    </div>
  </div>
</template>
