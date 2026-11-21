<script setup lang="ts">
/**
 * 顶部主控制栏组件（TopBar）
 *
 * 核心交互能力：
 * 1. 配置档案下拉切换 + 原生系统文件对话框打开外部 YAML；
 * 2. 核心目标 URL 编辑区：Method 选择 + URL 输入（带来源指示）+ n8n 一键正式/测试开关 + endpoints 预设下拉 + Ping 连通性探测；
 * 3. 批量控制按钮：发送选中 / 全部运行 / 暂停与继续 / 停止；
 * 4. 跟随系统 / 浅色 / 深色三态主题切换；
 * 5. 底部 Gemini 流光进度条与实时结果计数器。
 */
import { computed } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  ChevronDown,
  FilePlus,
  FolderOpen,
  Monitor,
  Moon,
  Palette,
  Pause,
  Play,
  RefreshCw,
  Send,
  Square,
  Sun,
  Wand2,
  Zap,
} from "lucide-vue-next";
import { useAppState } from "../composables/useAppState";
import { useTheme } from "../composables/useTheme";

const {
  leftTab,
  snapshot,
  workingConfig,
  scanResult,
  selectedIndices,
  runState,
  doneCount,
  totalSelected,
  runStats,
  pingStatus,
  isPinging,
  isBusy,
  n8nUrlMode,
  isStartupShimmer,
  showAppearanceModal,
  showSetupWizard,
  selectYamlProfile,
  mountYamlProfiles,
  toggleN8nTargetUrl,
  testEndpointPing,
  startBatchRun,
  controlRun,
} = useAppState();

const { themeMode, cycleTheme } = useTheme();

/** 顶部进度百分比 */
const progressPercent = computed<number>(() => {
  if (totalSelected.value <= 0) return 0;
  return Math.min(100, Math.round((doneCount.value / totalSelected.value) * 100));
});

/** 当前激活档案下拉值 */
const selectedProfileValue = computed<string>(
  () => snapshot.value?.yamlPath ?? "__gui_only__"
);

/** 当前选中 URL 来源标注 */
const urlSourceTag = computed<string>(() => {
  const src = snapshot.value?.fieldSources["url"];
  if (src === "yaml") return "YAML 优先";
  if (src === "gui") return "GUI 配置";
  return "";
});

/** 处理配置档案下拉变更 */
async function handleProfileChange(ev: Event): Promise<void> {
  const value = (ev.target as HTMLSelectElement).value;
  if (value === "__gui_only__") {
    await selectYamlProfile(null);
  } else {
    await selectYamlProfile(value);
  }
}

/** 弹出系统原生文件选择框挂载一个或多个 YAML/JSONC 档案 */
async function browseYamlFile(): Promise<void> {
  const selected = await open({
    title: "挂载一个或多个 YAML / JSONC 配置文件",
    multiple: true,
    filters: [
      {
        name: "YAML / JSONC 配置",
        extensions: ["yaml", "yml", "json", "jsonc"],
      },
    ],
  });
  if (!selected) return;
  const list = Array.isArray(selected) ? selected : [selected];
  if (list.length > 0) {
    await mountYamlProfiles(list, true);
  }
}

/** 切换到预设目标入口 */
function handleEndpointPresetChange(ev: Event): void {
  const url = (ev.target as HTMLSelectElement).value;
  if (url) {
    workingConfig.value.url = url;
    pingStatus.value = null;
  }
}
</script>

<template>
  <header
    class="shrink-0 border-b border-[var(--border-subtle)] bg-[var(--bg-surface)]/95 backdrop-blur-md transition-colors"
  >
    <div class="flex flex-wrap items-center gap-2.5 px-3.5 py-2.5">
      <!-- 左侧：应用徽标与 YAML 档案切换 -->
      <div class="flex items-center gap-2 min-w-0 shrink-0 max-w-full">
        <div
          class="aurora-ring-soft flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[var(--bg-elevated)] text-blue-600 dark:text-blue-400"
          :class="{ 'aurora-border-active': isBusy || isStartupShimmer }"
          title="HookReplay · Webhook 批量回放与调试工作台"
        >
          <Zap class="h-4 w-4" />
        </div>
        <div class="hidden xl:flex flex-col leading-tight mr-1">
          <span class="text-sm font-bold tracking-tight text-[var(--text-primary)]">
            HookReplay
          </span>
          <span class="text-[0.625rem] text-[var(--text-muted)]">
            Webhook 调试工作台
          </span>
        </div>

        <!-- 配置档案下拉选择 -->
        <div class="relative flex items-center">
          <select
            :value="selectedProfileValue"
            :disabled="isBusy"
            @change="handleProfileChange"
            title="一键切换已挂载的配置档案（YAML 优先级高于 GUI 兜底配置）"
            class="aurora-btn-subtle h-8 max-w-[11rem] appearance-none rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] pl-2.5 pr-7 text-xs font-medium text-[var(--text-primary)] outline-none transition hover:border-[var(--border-strong)] disabled:opacity-50"
          >
            <option value="__gui_only__">仅使用 GUI 独立配置</option>
            <option
              v-for="prof in snapshot?.profiles ?? []"
              :key="prof.path"
              :value="prof.path"
              :disabled="!prof.exists"
              :title="prof.path"
            >
              {{ prof.exists ? prof.name : `[缺失] ${prof.name}` }}
            </option>
          </select>
          <ChevronDown
            class="pointer-events-none absolute right-2 h-3.5 w-3.5 text-[var(--text-muted)]"
          />
        </div>

        <button
          type="button"
          :disabled="isBusy"
          @click="browseYamlFile"
          title="挂载一个或多个 .yaml / .yml / .jsonc 配置文件"
          class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] disabled:opacity-50"
        >
          <FolderOpen class="h-3.5 w-3.5" />
          <span class="hidden sm:inline">挂载YAML</span>
        </button>

        <button
          type="button"
          :disabled="isBusy"
          @click="leftTab = 'config'"
          title="前往配置中心管理和生成内置 YAML 模板"
          class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] disabled:opacity-50"
        >
          <FilePlus class="h-3.5 w-3.5 text-purple-500" />
          <span class="hidden 2xl:inline">模板</span>
        </button>
      </div>

      <!-- 中央：核心目标 URL 输入区与一键切换入口 -->
      <div
        class="aurora-focus flex min-w-[min(100%,24rem)] flex-1 items-center gap-1.5 overflow-hidden rounded-xl border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 transition-all"
      >
        <select
          v-model="workingConfig.method"
          :disabled="isBusy"
          class="h-7 shrink-0 rounded-md bg-[var(--bg-surface)] px-2 text-xs font-bold text-blue-600 dark:text-blue-400 outline-none border border-[var(--border-subtle)]"
        >
          <option value="POST">POST</option>
          <option value="PUT">PUT</option>
          <option value="PATCH">PATCH</option>
          <option value="GET">GET</option>
        </select>

        <input
          v-model="workingConfig.url"
          type="text"
          :disabled="isBusy"
          placeholder="输入目标 Webhook URL，例如 https://n8n.example.com/webhook/demo"
          class="min-w-[6rem] flex-1 bg-transparent px-1.5 font-mono text-xs text-[var(--text-primary)] outline-none placeholder:text-[var(--text-muted)]"
        />

        <!-- 字段来源徽标 -->
        <span
          v-if="urlSourceTag && workingConfig.url === snapshot?.effectiveConfig.url"
          class="hidden 2xl:inline-flex shrink-0 whitespace-nowrap rounded px-1.5 py-0.5 text-[0.625rem] font-semibold"
          :class="
            urlSourceTag === 'YAML 优先'
              ? 'bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
              : 'bg-cyan-500/15 text-cyan-600 dark:text-cyan-300'
          "
        >
          {{ urlSourceTag }}
        </span>

        <!-- n8n 正式 / 测试路径一键互换胶囊开关 -->
        <button
          v-if="n8nUrlMode !== null"
          type="button"
          :disabled="isBusy"
          @click="toggleN8nTargetUrl"
          :title="
            n8nUrlMode === 'prod'
              ? '当前为正式入口 /webhook/，点击一键切换至测试入口 /webhook-test/'
              : '当前为编辑器测试入口 /webhook-test/，点击一键切回正式入口 /webhook/'
          "
          class="aurora-pill inline-flex h-7 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg px-2 text-[0.6875rem] font-semibold transition cursor-pointer disabled:opacity-50"
          :class="
            n8nUrlMode === 'test'
              ? 'bg-amber-500/20 text-amber-700 dark:text-amber-300 border border-amber-500/40'
              : 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 border border-emerald-500/30 hover:bg-emerald-500/25'
          "
        >
          <RefreshCw class="h-3 w-3 shrink-0" />
          <span>{{ n8nUrlMode === "test" ? "测试" : "正式" }}</span>
          <span class="hidden 2xl:inline">{{ n8nUrlMode === "test" ? " /webhook-test/" : " /webhook/" }}</span>
        </button>

        <!-- 预设入口（endpoints）快速切换菜单 -->
        <div v-if="workingConfig.endpoints.length > 0" class="relative shrink-0">
          <select
            :value="''"
            :disabled="isBusy"
            @change="handleEndpointPresetChange"
            title="从配置中的 endpoints 预设列表中一键切换目标网址"
            class="h-7 max-w-[7.5rem] truncate appearance-none rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] pl-2 pr-6 text-[0.6875rem] font-medium text-[var(--text-secondary)] outline-none hover:text-[var(--text-primary)]"
          >
            <option value="" disabled>预设入口…</option>
            <option
              v-for="(ep, idx) in workingConfig.endpoints"
              :key="idx"
              :value="ep.url"
            >
              {{ ep.name }}
            </option>
          </select>
          <ChevronDown
            class="pointer-events-none absolute right-1.5 top-1/2 -translate-y-1/2 h-3 w-3 text-[var(--text-muted)]"
          />
        </div>

        <!-- Ping 连通性测试按钮 -->
        <button
          type="button"
          :disabled="isPinging || !workingConfig.url.trim()"
          @click="testEndpointPing"
          title="发送 HEAD 探测包检测网络、TLS 与目标端点是否可达"
          class="aurora-btn-subtle inline-flex h-7 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-surface)] px-2.5 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)] disabled:opacity-45"
        >
          <Activity class="h-3.5 w-3.5" :class="{ 'animate-pulse text-blue-500': isPinging }" />
          <span>Ping</span>
          <span
            v-if="pingStatus"
            class="ml-0.5 h-2 w-2 rounded-full"
            :class="pingStatus.reachable ? 'bg-emerald-500' : 'bg-rose-500'"
          />
        </button>
      </div>

      <!-- 右侧：执行控制与三态主题开关 -->
      <div class="flex flex-wrap items-center gap-2 shrink-0 max-w-full">
        <template v-if="!isBusy">
          <button
            type="button"
            :disabled="selectedIndices.size === 0 || !workingConfig.url.trim()"
            @click="startBatchRun()"
            class="aurora-btn inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg px-3.5 text-xs font-semibold text-white shadow-sm disabled:cursor-not-allowed disabled:opacity-45"
          >
            <Send class="h-3.5 w-3.5" />
            <span>发送选中 ({{ selectedIndices.size }})</span>
          </button>

          <button
            type="button"
            :disabled="scanResult.entries.length === 0 || !workingConfig.url.trim()"
            @click="startBatchRun(scanResult.entries)"
            title="忽略单条勾选状态，直接发送当前源中的全部条目"
            class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs font-medium text-[var(--text-primary)] transition hover:bg-[var(--bg-hover)] disabled:opacity-45"
          >
            <Play class="h-3.5 w-3.5 text-emerald-500" />
            <span class="hidden sm:inline">全部运行</span>
          </button>
        </template>

        <template v-else>
          <button
            v-if="runState === 'paused'"
            type="button"
            @click="controlRun('resume')"
            class="inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg bg-emerald-600 px-3 text-xs font-semibold text-white hover:bg-emerald-500"
          >
            <Play class="h-3.5 w-3.5" />
            <span>继续</span>
          </button>
          <button
            v-else
            type="button"
            @click="controlRun('pause')"
            class="inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg bg-amber-500 px-3 text-xs font-semibold text-white hover:bg-amber-400"
          >
            <Pause class="h-3.5 w-3.5" />
            <span>暂停</span>
          </button>

          <button
            type="button"
            @click="controlRun('abort')"
            class="inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg bg-rose-600 px-3 text-xs font-semibold text-white hover:bg-rose-500"
          >
            <Square class="h-3.5 w-3.5" />
            <span>停止</span>
          </button>
        </template>

        <!-- 本机字体与流光设置弹窗 -->
        <button
          type="button"
          @click="showAppearanceModal = true"
          title="设置本机字体、界面/代码字号（快捷键 Ctrl + / - / 0）与流光动效强度"
          class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
        >
          <Palette class="h-3.5 w-3.5 text-purple-500" />
          <span class="hidden xl:inline">字体外观</span>
        </button>

        <!-- 打开配置向导 -->
        <button
          type="button"
          @click="showSetupWizard = true"
          title="重新打开初始化配置向导"
          class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
        >
          <Wand2 class="h-3.5 w-3.5 text-indigo-500" />
          <span class="hidden 2xl:inline">向导</span>
        </button>

        <!-- 三态主题切换按钮 -->
        <button
          type="button"
          @click="cycleTheme"
          title="点击切换外观主题：跟随系统 -> 浅色 -> 深色"
          class="aurora-btn-subtle inline-flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-lg border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2.5 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]"
        >
          <Monitor v-if="themeMode === 'system'" class="h-3.5 w-3.5 text-indigo-500" />
          <Sun v-else-if="themeMode === 'light'" class="h-3.5 w-3.5 text-amber-500" />
          <Moon v-else class="h-3.5 w-3.5 text-blue-400" />
          <span class="hidden lg:inline">
            {{ themeMode === "system" ? "跟随系统" : themeMode === "light" ? "浅色" : "深色" }}
          </span>
        </button>
      </div>
    </div>

    <!-- 底部进度条与实时结果汇总栏 -->
    <div
      v-if="isBusy || runStats.totalDone > 0"
      class="border-t border-[var(--border-subtle)] bg-[var(--bg-elevated)]/60 px-3.5 py-1.5"
    >
      <div class="flex flex-wrap items-center justify-between gap-2 text-[0.6875rem]">
        <div class="flex items-center gap-3">
          <span class="font-medium text-[var(--text-primary)]">
            {{
              runState === "waiting_confirm"
                ? "等待逐段确认…"
                : runState === "paused"
                  ? "已暂停"
                  : runState === "running"
                    ? "正在回放…"
                    : runState === "aborted"
                      ? "已手动停止"
                      : "回放完成"
            }}
          </span>
          <span class="font-mono text-[var(--text-muted)]">
            {{ doneCount }} / {{ totalSelected }} ({{ progressPercent }}%)
          </span>
        </div>

        <div class="flex flex-wrap items-center gap-2">
          <span class="rounded-full bg-emerald-500/15 px-2 py-0.5 font-medium text-emerald-700 dark:text-emerald-300">
            成功 {{ runStats.ok }}
          </span>
          <span class="rounded-full bg-sky-500/15 px-2 py-0.5 font-medium text-sky-700 dark:text-sky-300">
            幂等跳过 {{ runStats.deduped }}
          </span>
          <span class="rounded-full bg-amber-500/15 px-2 py-0.5 font-medium text-amber-700 dark:text-amber-300">
            未匹配 {{ runStats.unmatched }}
          </span>
          <span class="rounded-full bg-rose-500/15 px-2 py-0.5 font-medium text-rose-700 dark:text-rose-300">
            失败 {{ runStats.failed }}
          </span>
        </div>
      </div>

      <!-- 进度条轨道 -->
      <div class="mt-1 h-1.5 w-full overflow-hidden rounded-full bg-[var(--border-subtle)]">
        <div
          class="h-full rounded-full transition-all duration-200"
          :class="isBusy ? 'aurora-progress' : 'bg-emerald-500'"
          :style="{ width: `${progressPercent}%` }"
        />
      </div>
    </div>
  </header>
</template>
