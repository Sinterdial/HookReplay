<script setup lang="ts">
/**
 * 右栏：单条 Payload 在线检视、改写编辑、脱敏请求头与响应透视窗（InspectorPane）
 *
 * 设计亮点：
 * 1. 按字节偏移懒加载单条原始 JSON，点击即开，不对超大日志文件产生内存膨胀；
 * 2. 支持直接格式化、压缩、还原并在编辑器内修改字段，一键「单条即时重发」验证边界情况；
 * 3. 分页查看脱敏后的实际请求头（Authorization / X-Hub-Signature-256）、目标 URL 与服务器响应。
 */
import { computed, ref } from "vue";
import {
  Braces,
  Check,
  Copy,
  Eye,
  Minimize2,
  PanelRightClose,
  Pencil,
  Pin,
  RotateCcw,
  Send,
  ShieldCheck,
  Terminal,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useWorkspaceLayout } from "../composables/useWorkspaceLayout";
import JsonViewer from "./JsonViewer.vue";

const {
  activeEntry,
  activeEntryRaw,
  editedEntryRaw,
  isLoadingEntryRaw,
  isReplayingSingle,
  resultsByIndex,
  workingConfig,
  replaySingleActive,
  pushToast,
} = useAppState();

const { rightFloating, toggleFloating } = useWorkspaceLayout();

const activeTab = ref<"payload" | "request" | "response">("payload");
const payloadMode = ref<"preview" | "edit">("preview");
const copiedFlag = ref(false);

/** 当前选中条目的最新执行结果 */
const currentResult = computed(() => {
  if (!activeEntry.value) return undefined;
  return resultsByIndex.get(activeEntry.value.index);
});

/** 校验当前编辑中的 Payload JSON 是否合法 */
const jsonValidation = computed<{ valid: boolean; error: string }>(() => {
  const txt = editedEntryRaw.value.trim();
  if (!txt) return { valid: true, error: "" };
  try {
    JSON.parse(txt);
    return { valid: true, error: "" };
  } catch (err) {
    return { valid: false, error: String(err) };
  }
});

/** 对响应体自动尝试 JSON 缩进美化 */
const formattedResponse = computed<string>(() => {
  const resp = currentResult.value?.response ?? "";
  if (!resp) return "";
  try {
    return JSON.stringify(JSON.parse(resp), null, 2);
  } catch {
    return resp;
  }
});

/** 美化 sentBody */
const formattedSentBody = computed<string>(() => {
  const body = currentResult.value?.sentBody ?? "";
  if (!body) return "";
  try {
    return JSON.stringify(JSON.parse(body), null, 2);
  } catch {
    return body;
  }
});

function formatJson(): void {
  try {
    editedEntryRaw.value = JSON.stringify(JSON.parse(editedEntryRaw.value), null, 2);
  } catch (err) {
    pushToast(`JSON 格式有误，无法美化：${String(err)}`, "warning");
  }
}

function minifyJson(): void {
  try {
    editedEntryRaw.value = JSON.stringify(JSON.parse(editedEntryRaw.value));
  } catch (err) {
    pushToast(`JSON 格式有误，无法压缩：${String(err)}`, "warning");
  }
}

function resetRawJson(): void {
  try {
    editedEntryRaw.value = JSON.stringify(JSON.parse(activeEntryRaw.value), null, 2);
  } catch {
    editedEntryRaw.value = activeEntryRaw.value;
  }
}

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    copiedFlag.value = true;
    window.setTimeout(() => {
      copiedFlag.value = false;
    }, 1500);
  } catch {
    pushToast("复制失败，请检查剪贴板权限", "warning");
  }
}
</script>

<template>
  <aside
    class="flex h-full w-full min-w-0 flex-1 flex-col bg-[var(--bg-surface)]"
    :class="rightFloating ? '' : 'border-l border-[var(--border-subtle)]'"
  >
    <template v-if="activeEntry">
      <!-- 头部：条目基础元信息 -->
      <div class="border-b border-[var(--border-subtle)] px-3.5 py-2.5 space-y-1.5">
        <div class="flex items-center justify-between gap-2">
          <div class="flex min-w-0 items-center gap-1.5">
            <span class="rounded bg-blue-600/12 px-1.5 py-0.5 font-mono text-[0.6875rem] font-bold text-blue-600 dark:text-blue-300">
              #{{ activeEntry.index }}
            </span>
            <span class="truncate text-xs font-bold text-[var(--text-primary)]">
              {{ activeEntry.title || activeEntry.label }}
            </span>
          </div>

          <div class="flex shrink-0 items-center gap-1.5">
            <span
              v-if="currentResult"
              class="shrink-0 rounded-full px-2 py-0.5 text-[0.625rem] font-semibold"
              :class="
                currentResult.outcome === 'ok'
                  ? 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300'
                  : currentResult.outcome === 'deduped'
                    ? 'bg-sky-500/15 text-sky-700 dark:text-sky-300'
                    : currentResult.outcome === 'unmatched'
                      ? 'bg-amber-500/15 text-amber-700 dark:text-amber-300'
                      : 'bg-rose-500/15 text-rose-700 dark:text-rose-300'
              "
            >
              {{ currentResult.badge }} · {{ currentResult.ms }}ms
            </span>

            <button
              type="button"
              @click="toggleFloating('right')"
              :title="
                rightFloating
                  ? '固定为常驻检视栏（Ctrl+Shift+\\）'
                  : '转为悬浮检视栏（鼠标移到右边缘自动展开 · Ctrl+Shift+\\）'
              "
              class="aurora-btn-subtle flex h-6.5 w-6.5 shrink-0 items-center justify-center rounded-md border transition"
              :class="
                rightFloating
                  ? 'border-indigo-500/45 bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
                  : 'border-transparent text-[var(--text-muted)] hover:border-[var(--border-subtle)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]'
              "
            >
              <Pin v-if="rightFloating" class="h-3.5 w-3.5" />
              <PanelRightClose v-else class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <div class="flex items-center justify-between text-[0.6875rem] text-[var(--text-muted)] font-mono">
          <span class="truncate">段 #{{ activeEntry.segment }} · {{ activeEntry.comment }}</span>
          <span class="shrink-0">{{ activeEntry.sourceName }}:L{{ activeEntry.line }}</span>
        </div>
      </div>

      <!-- 子页签切换 -->
      <div class="grid grid-cols-3 border-b border-[var(--border-subtle)] bg-[var(--bg-elevated)]/50 p-1 gap-1">
        <button
          type="button"
          @click="activeTab = 'payload'"
          class="rounded-md py-1 text-xs font-semibold transition"
          :class="
            activeTab === 'payload'
              ? 'aurora-tab-active bg-[var(--bg-surface)] text-[var(--text-primary)] shadow-xs'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
        >
          请求体 Payload
        </button>

        <button
          type="button"
          @click="activeTab = 'request'"
          class="rounded-md py-1 text-xs font-semibold transition"
          :class="
            activeTab === 'request'
              ? 'aurora-tab-active bg-[var(--bg-surface)] text-[var(--text-primary)] shadow-xs'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
        >
          请求头 & URL
        </button>

        <button
          type="button"
          @click="activeTab = 'response'"
          class="rounded-md py-1 text-xs font-semibold transition"
          :class="
            activeTab === 'response'
              ? 'aurora-tab-active bg-[var(--bg-surface)] text-[var(--text-primary)] shadow-xs'
              : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
          "
        >
          响应 Response
        </button>
      </div>

      <!-- Tab 1: 请求体高亮预览与源码在线编辑 -->
      <div v-if="activeTab === 'payload'" class="flex min-h-0 flex-1 flex-col">
        <div class="flex flex-wrap items-center justify-between gap-1.5 border-b border-[var(--border-subtle)] px-3 py-1.5">
          <div class="flex flex-wrap items-center gap-1">
            <!-- 预览 / 编辑切换胶囊 -->
            <div class="mr-1 inline-flex rounded-md bg-[var(--bg-elevated)] p-0.5">
              <button
                type="button"
                @click="payloadMode = 'preview'"
                title="JSON 语法高亮预览（双击内容区可切换编辑）"
                class="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[0.6875rem] font-medium transition"
                :class="
                  payloadMode === 'preview'
                    ? 'bg-[var(--bg-surface)] text-indigo-600 shadow-xs dark:text-indigo-300'
                    : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
                "
              >
                <Eye class="h-3 w-3" />
                <span>高亮预览</span>
              </button>
              <button
                type="button"
                @click="payloadMode = 'edit'"
                title="切换源码编辑模式修改单条 Payload"
                class="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[0.6875rem] font-medium transition"
                :class="
                  payloadMode === 'edit'
                    ? 'bg-[var(--bg-surface)] text-blue-600 shadow-xs dark:text-blue-300'
                    : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'
                "
              >
                <Pencil class="h-3 w-3" />
                <span>源码编辑</span>
              </button>
            </div>

            <button
              type="button"
              @click="formatJson"
              title="格式化缩进"
              class="inline-flex items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
            >
              <Braces class="h-3 w-3" />
              <span>格式化</span>
            </button>
            <button
              type="button"
              @click="minifyJson"
              title="压缩为单行 JSON"
              class="inline-flex items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
            >
              <Minimize2 class="h-3 w-3" />
              <span>压缩</span>
            </button>
            <button
              type="button"
              @click="resetRawJson"
              title="恢复为日志原始文本"
              class="inline-flex items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
            >
              <RotateCcw class="h-3 w-3" />
              <span>还原</span>
            </button>
            <button
              v-if="payloadMode === 'edit'"
              type="button"
              @click="copyText(editedEntryRaw)"
              class="inline-flex items-center gap-1 rounded px-1.5 py-1 text-[0.6875rem] text-[var(--text-secondary)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]"
            >
              <Check v-if="copiedFlag" class="h-3 w-3 text-emerald-500" />
              <Copy v-else class="h-3 w-3" />
              <span>复制</span>
            </button>
          </div>

          <button
            type="button"
            :disabled="isReplayingSingle || !workingConfig.url.trim()"
            @click="replaySingleActive"
            title="以当前编辑器中的文本直接发送一次 HTTP 请求（自动附加认证、测试标记与签名）"
            class="aurora-btn inline-flex items-center gap-1 rounded-lg px-2.5 py-1 text-xs font-semibold text-white disabled:opacity-45"
          >
            <Send class="h-3 w-3" />
            <span>{{ isReplayingSingle ? "发送中…" : "单条重发" }}</span>
          </button>
        </div>

        <div
          v-if="!jsonValidation.valid"
          class="bg-amber-500/12 px-3 py-1 text-[0.6875rem] text-amber-700 dark:text-amber-300"
        >
          JSON 语法提示：{{ jsonValidation.error }}
        </div>

        <div v-if="isLoadingEntryRaw" class="flex flex-1 items-center justify-center text-xs text-[var(--text-muted)]">
          正在从磁盘按偏移读取原始字节…
        </div>

        <div v-else-if="payloadMode === 'preview'" class="flex min-h-0 flex-1 flex-col p-2.5">
          <JsonViewer
            :code="editedEntryRaw"
            empty-text="当前条目为空"
            @dblclick-code="payloadMode = 'edit'"
          />
          <div class="mt-1 flex items-center justify-between px-1 text-[0.625rem] text-[var(--text-muted)]">
            <span>提示：双击代码区域或点击上方「源码编辑」即可改写字段后单条重发</span>
          </div>
        </div>

        <textarea
          v-else
          v-model="editedEntryRaw"
          spellcheck="false"
          class="code-editor-text min-h-0 flex-1 resize-none bg-transparent p-3 text-[var(--text-primary)] outline-none"
        />
      </div>

      <!-- Tab 2: 脱敏请求头、实际 URL 与实际发出请求体 -->
      <div v-else-if="activeTab === 'request'" class="min-h-0 flex-1 overflow-y-auto p-3.5 space-y-3 text-xs">
        <template v-if="currentResult">
          <div class="space-y-1">
            <div class="flex items-center gap-1.5 font-semibold text-[var(--text-secondary)]">
              <ShieldCheck class="h-3.5 w-3.5 text-emerald-500" />
              <span>最终发送 URL（敏感 Query 已脱敏）</span>
            </div>
            <div class="break-all rounded-lg bg-[var(--bg-elevated)] p-2.5 font-mono text-[0.6875rem] text-[var(--text-primary)]">
              {{ workingConfig.method }} {{ currentResult.finalUrl }}
            </div>
          </div>

          <div class="space-y-1.5">
            <div class="font-semibold text-[var(--text-secondary)]">
              请求头明细（已包含鉴权 / HMAC 签名 / 自定义 Header）
            </div>
            <div class="divide-y divide-[var(--border-subtle)] rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60">
              <div
                v-for="(v, k) in currentResult.maskedHeaders"
                :key="k"
                class="flex gap-2 px-2.5 py-1.5 font-mono text-[0.6875rem]"
              >
                <span class="w-36 shrink-0 font-semibold text-blue-600 dark:text-blue-300">{{ k }}</span>
                <span class="min-w-0 flex-1 break-all text-[var(--text-primary)]">{{ v }}</span>
              </div>
            </div>
          </div>

          <div class="space-y-1.5">
            <div class="font-semibold text-[var(--text-secondary)]">
              注入标记与插值后实际发送 Body
            </div>
            <JsonViewer
              :code="formattedSentBody"
              max-height="20rem"
              empty-text="空请求体"
            />
          </div>
        </template>

        <div
          v-else
          class="rounded-xl border border-dashed border-[var(--border-strong)] bg-[var(--bg-elevated)]/40 p-4 space-y-2 text-[var(--text-secondary)]"
        >
          <p class="font-semibold text-[var(--text-primary)]">尚未发送过此条目</p>
          <p>点击「单条重发」或运行批量任务后，此处将显示经过变量插值、认证注入、测试标记写入与 HMAC 签名计算的完整请求详情。</p>
          <div class="pt-1 font-mono text-[0.6875rem] text-[var(--text-muted)] space-y-0.5">
            <div>当前目标：{{ workingConfig.method }} {{ workingConfig.url || "(未填写)" }}</div>
            <div>当前鉴权：{{ workingConfig.auth.type }}</div>
            <div>签名状态：{{ workingConfig.signature.enabled ? workingConfig.signature.algorithm : "未启用" }}</div>
          </div>
        </div>
      </div>

      <!-- Tab 3: 响应体与错误详情 -->
      <div v-else class="flex min-h-0 flex-1 flex-col overflow-hidden p-3.5 space-y-3 text-xs">
        <template v-if="currentResult">
          <div class="flex shrink-0 items-center justify-between rounded-xl bg-[var(--bg-elevated)] p-2.5">
            <div class="flex items-center gap-2">
              <Terminal class="h-4 w-4 text-blue-500" />
              <span class="font-mono font-bold">HTTP {{ currentResult.status }}</span>
              <span class="text-[var(--text-muted)]">{{ currentResult.badge }}</span>
            </div>
            <button
              type="button"
              @click="copyText(currentResult.response || currentResult.error)"
              class="inline-flex items-center gap-1 rounded px-2 py-0.5 text-[0.6875rem] hover:bg-[var(--bg-hover)]"
            >
              <Copy class="h-3 w-3" />
              <span>复制内容</span>
            </button>
          </div>

          <div
            v-if="currentResult.error"
            class="shrink-0 rounded-xl border border-rose-500/35 bg-rose-500/10 p-3 font-mono text-[0.6875rem] text-rose-700 dark:text-rose-300"
          >
            {{ currentResult.error }}
          </div>

          <JsonViewer
            v-if="formattedResponse || !currentResult.error"
            :code="formattedResponse"
            empty-text="服务器返回空响应体"
            class="min-h-0 flex-1"
          />
        </template>

        <div
          v-else
          class="flex h-48 items-center justify-center rounded-xl border border-dashed border-[var(--border-strong)] text-[var(--text-muted)]"
        >
          发送请求后在此查看服务器响应体与耗时明细
        </div>
      </div>
    </template>

    <div v-else class="flex h-full flex-col">
      <div class="flex items-center justify-between border-b border-[var(--border-subtle)] px-3.5 py-2">
        <span class="text-xs font-semibold text-[var(--text-muted)]">单条检视器</span>
        <button
          type="button"
          @click="toggleFloating('right')"
          :title="
            rightFloating
              ? '固定为常驻检视栏（Ctrl+Shift+\\）'
              : '转为悬浮检视栏（鼠标移到右边缘自动展开 · Ctrl+Shift+\\）'
          "
          class="aurora-btn-subtle flex h-6.5 w-6.5 shrink-0 items-center justify-center rounded-md border transition"
          :class="
            rightFloating
              ? 'border-indigo-500/45 bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
              : 'border-transparent text-[var(--text-muted)] hover:border-[var(--border-subtle)] hover:bg-[var(--bg-elevated)] hover:text-[var(--text-primary)]'
          "
        >
          <Pin v-if="rightFloating" class="h-3.5 w-3.5" />
          <PanelRightClose v-else class="h-3.5 w-3.5" />
        </button>
      </div>
      <div
        class="flex flex-1 flex-col items-center justify-center p-6 text-center text-xs text-[var(--text-muted)]"
      >
        <Braces class="mb-2 h-8 w-8 opacity-30" />
        <p class="font-medium text-[var(--text-secondary)]">未选中任何条目</p>
        <p class="mt-1">在中间列表点击任意一条通知记录，即可懒加载完整 JSON、在线修改并单条调试发送。</p>
      </div>
    </div>
  </aside>
</template>
