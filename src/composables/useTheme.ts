/**
 * 外观主题、本机字体与流光动效控制器（Theme & Appearance Composable）
 *
 * 支持：
 * 1. `system` / `light` / `dark` 三态主题同步与持久化；
 * 2. 本机已安装字体枚举（`list_system_fonts`）与正文字体 / 等宽字体 / 字号即时注入 CSS Token；
 * 3. Gemini 流光动效强度档位（`rich` / `subtle` / `off`）实时控制。
 */
import { computed, ref, watchEffect } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type {
  AuroraIntensity,
  ConfigSnapshot,
  SystemFontInfo,
  ThemeMode,
} from "../types";

const themeMode = ref<ThemeMode>("system");
const uiFont = ref<string>("");
const monoFont = ref<string>("");
const fontSize = ref<number>(14);
const codeFontSize = ref<number>(13);
const auroraIntensity = ref<AuroraIntensity>("rich");
const systemFonts = ref<SystemFontInfo[]>([]);
const isLoadingFonts = ref<boolean>(false);

const systemPrefersDark = ref<boolean>(
  typeof window !== "undefined"
    ? window.matchMedia("(prefers-color-scheme: dark)").matches
    : false
);

let mediaListenerBound = false;

/** 判断当前生效是否为深色界面 */
const isDark = computed<boolean>(() => {
  if (themeMode.value === "dark") return true;
  if (themeMode.value === "light") return false;
  return systemPrefersDark.value;
});

/** 将字体族名安全包裹为 CSS font-family 声明 */
function formatFontStack(customFont: string, fallbackVar: string): string {
  const trimmed = customFont.trim();
  if (!trimmed) {
    return `var(${fallbackVar})`;
  }
  const escaped = trimmed.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `"${escaped}", var(${fallbackVar})`;
}

/** 绑定系统级浅色/深色切换监听（全局仅绑定一次） */
function bindSystemThemeListener(): void {
  if (mediaListenerBound || typeof window === "undefined") return;
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  systemPrefersDark.value = mq.matches;
  const handler = (ev: MediaQueryListEvent): void => {
    systemPrefersDark.value = ev.matches;
  };
  mq.addEventListener("change", handler);
  mediaListenerBound = true;
}

/** 实时将主题类名、自定义本机字体、双字号与流光强度同步到 HTML 根节点 */
watchEffect(() => {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  root.classList.toggle("dark", isDark.value);
  root.style.setProperty("--font-sans", formatFontStack(uiFont.value, "--font-sans-default"));
  root.style.setProperty("--font-mono", formatFontStack(monoFont.value, "--font-mono-default"));
  const clampedUiSize = Math.min(22, Math.max(11, Math.round(fontSize.value || 14)));
  const clampedCodeSize = Math.min(24, Math.max(10, Math.round(codeFontSize.value || 13)));
  root.style.setProperty("--app-font-size", `${clampedUiSize}px`);
  root.style.setProperty("--code-font-size", `${clampedCodeSize}px`);
  root.setAttribute("data-aurora", auroraIntensity.value);
});

export function useTheme() {
  bindSystemThemeListener();

  /** 从后端配置快照初始化主题（兼容旧调用） */
  function hydrateTheme(raw?: string | null): void {
    if (raw === "light" || raw === "dark" || raw === "system") {
      themeMode.value = raw;
    } else {
      themeMode.value = "system";
    }
  }

  /** 从后端配置快照完整同步主题、本机字体、双字号与流光动效配置 */
  function hydrateAppearance(
    snap: Pick<
      ConfigSnapshot,
      "theme" | "uiFont" | "monoFont" | "fontSize" | "codeFontSize" | "auroraIntensity"
    >
  ): void {
    hydrateTheme(snap.theme);
    uiFont.value = snap.uiFont ?? "";
    monoFont.value = snap.monoFont ?? "";
    fontSize.value = snap.fontSize >= 11 && snap.fontSize <= 22 ? snap.fontSize : 14;
    codeFontSize.value =
      typeof snap.codeFontSize === "number" &&
      snap.codeFontSize >= 10 &&
      snap.codeFontSize <= 24
        ? snap.codeFontSize
        : 13;
    if (
      snap.auroraIntensity === "rich" ||
      snap.auroraIntensity === "subtle" ||
      snap.auroraIntensity === "off"
    ) {
      auroraIntensity.value = snap.auroraIntensity;
    } else {
      auroraIntensity.value = "rich";
    }
  }

  /** 按 system -> light -> dark 指定模式切换主题并持久化 */
  async function setTheme(nextMode: ThemeMode): Promise<void> {
    themeMode.value = nextMode;
    try {
      await invoke("set_theme_mode", { theme: nextMode });
    } catch {
      // 在纯浏览器预览模式下忽略 Tauri Invoke 异常
    }
  }

  /** 一键循环切换三态主题 */
  async function cycleTheme(): Promise<void> {
    const order: ThemeMode[] = ["system", "light", "dark"];
    const nextIdx = (order.indexOf(themeMode.value) + 1) % order.length;
    await setTheme(order[nextIdx]);
  }

  /** 拉取本机已安装字体列表（带缓存，可传 force=true 强制刷新） */
  async function loadSystemFonts(force = false): Promise<SystemFontInfo[]> {
    if (!force && systemFonts.value.length > 0) {
      return systemFonts.value;
    }
    isLoadingFonts.value = true;
    try {
      const list = await invoke<SystemFontInfo[]>("list_system_fonts");
      systemFonts.value = list;
      return list;
    } catch {
      return systemFonts.value;
    } finally {
      isLoadingFonts.value = false;
    }
  }

  /** 保存字体、双字号与流光动效设置到后端 */
  async function saveAppearanceSettings(opts: {
    uiFont: string;
    monoFont: string;
    fontSize: number;
    codeFontSize?: number;
    auroraIntensity: AuroraIntensity;
  }): Promise<ConfigSnapshot | null> {
    uiFont.value = opts.uiFont.trim();
    monoFont.value = opts.monoFont.trim();
    fontSize.value = Math.min(22, Math.max(11, Math.round(opts.fontSize)));
    codeFontSize.value = Math.min(
      24,
      Math.max(10, Math.round(opts.codeFontSize ?? codeFontSize.value ?? 13))
    );
    auroraIntensity.value = opts.auroraIntensity;
    try {
      return await invoke<ConfigSnapshot>("set_appearance_settings", {
        uiFont: uiFont.value,
        monoFont: monoFont.value,
        fontSize: fontSize.value,
        codeFontSize: codeFontSize.value,
        auroraIntensity: auroraIntensity.value,
      });
    } catch {
      return null;
    }
  }

  /** 恢复默认字体与标准字号 */
  async function resetFontsToDefault(): Promise<ConfigSnapshot | null> {
    return saveAppearanceSettings({
      uiFont: "",
      monoFont: "",
      fontSize: 14,
      codeFontSize: 13,
      auroraIntensity: auroraIntensity.value,
    });
  }

  /** 快捷键步进调节界面与代码字号（Ctrl + / -）并持久化 */
  async function stepFontSizes(delta: number): Promise<ConfigSnapshot | null> {
    const nextUi = Math.min(22, Math.max(11, Math.round(fontSize.value + delta)));
    const nextCode = Math.min(24, Math.max(10, Math.round(codeFontSize.value + delta)));
    return saveAppearanceSettings({
      uiFont: uiFont.value,
      monoFont: monoFont.value,
      fontSize: nextUi,
      codeFontSize: nextCode,
      auroraIntensity: auroraIntensity.value,
    });
  }

  /** 快捷键重置界面与代码字号为默认 14px / 13px（Ctrl + 0） */
  async function resetFontSizesOnly(): Promise<ConfigSnapshot | null> {
    return saveAppearanceSettings({
      uiFont: uiFont.value,
      monoFont: monoFont.value,
      fontSize: 14,
      codeFontSize: 13,
      auroraIntensity: auroraIntensity.value,
    });
  }

  return {
    themeMode,
    isDark,
    uiFont,
    monoFont,
    fontSize,
    codeFontSize,
    auroraIntensity,
    systemFonts,
    isLoadingFonts,
    hydrateTheme,
    hydrateAppearance,
    setTheme,
    cycleTheme,
    loadSystemFonts,
    saveAppearanceSettings,
    resetFontsToDefault,
    stepFontSizes,
    resetFontSizesOnly,
  };
}
