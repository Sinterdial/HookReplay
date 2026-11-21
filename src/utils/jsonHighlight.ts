/**
 * 轻量级无依赖 JSON / JSONL 词法高亮与关键词突显引擎（XSS 安全）
 *
 * 特性：
 * 1. 全量 HTML 转义，杜绝任意 Payload 内容注入风险；
 * 2. 自动识别对象键名、重要 Webhook 关键词、字符串、数字、布尔、null、标点及 `#`/`//` 注释行；
 * 3. 在词法 Token 文本阶段进行搜索词匹配标记，不破坏 HTML 标签结构，并精确统计命中次数。
 */

export interface HighlightOptions {
  /** 是否突显 Webhook 核心关键字段（type / title / status / deduped / __test 等） */
  highlightImportantKeys?: boolean;
  /** 搜索高亮关键词（不区分大小写） */
  searchKeyword?: string;
  /** 当前高亮定位的命中序号（0-based） */
  activeHitIndex?: number;
}

export interface HighlightResult {
  /** 高亮后的安全 HTML 字符串 */
  html: string;
  /** 搜索关键词命中总次数 */
  matchCount: number;
  /** 总行数 */
  lineCount: number;
}

/** 重点突显的 Webhook / 通知常见关键字段集合（小写匹配） */
const IMPORTANT_KEYS = new Set<string>([
  "type",
  "event_type",
  "event",
  "action",
  "title",
  "title_year",
  "name",
  "message",
  "content",
  "url",
  "status",
  "error",
  "deduped",
  "__test",
  "season",
  "episode",
  "alerts",
  "feed",
  "entries",
  "mediainfo",
  "subscribe_info",
  "category",
  "scene",
  "subject",
  "alertname",
  "idempotency_key",
  "ok",
]);

/** HTML 特殊字符转义 */
function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** 构造在纯文本内部切分搜索词并注入 <mark> 的安全输出 */
function renderTextWithSearch(
  rawText: string,
  keywordLower: string,
  hitCounter: { count: number },
  activeHitIndex: number
): string {
  if (!keywordLower) {
    return escapeHtml(rawText);
  }
  const lower = rawText.toLowerCase();
  const kwLen = keywordLower.length;
  let cursor = 0;
  let out = "";

  while (cursor < rawText.length) {
    const pos = lower.indexOf(keywordLower, cursor);
    if (pos === -1) {
      out += escapeHtml(rawText.slice(cursor));
      break;
    }
    if (pos > cursor) {
      out += escapeHtml(rawText.slice(cursor, pos));
    }
    const idx = hitCounter.count;
    hitCounter.count += 1;
    const isCurrent = idx === activeHitIndex;
    const cls = isCurrent
      ? "json-search-hit json-search-hit-current"
      : "json-search-hit";
    out += `<mark class="${cls}" data-hit-index="${idx}">${escapeHtml(
      rawText.slice(pos, pos + kwLen)
    )}</mark>`;
    cursor = pos + kwLen;
  }
  return out;
}

/** 扫描并高亮单行（兼容美化后 JSON 跨行片段、紧凑 JSONL 或注释行） */
function highlightLine(
  line: string,
  opts: {
    highlightImportantKeys: boolean;
    keywordLower: string;
    hitCounter: { count: number };
    activeHitIndex: number;
  }
): string {
  const trimmed = line.trimStart();
  if (trimmed.startsWith("#") || trimmed.startsWith("//")) {
    return `<span class="json-comment">${renderTextWithSearch(
      line,
      opts.keywordLower,
      opts.hitCounter,
      opts.activeHitIndex
    )}</span>`;
  }

  let out = "";
  let i = 0;
  const n = line.length;

  while (i < n) {
    const ch = line[i];

    // 空白字符原样保留
    if (ch === " " || ch === "\t" || ch === "\r") {
      let j = i + 1;
      while (j < n && (line[j] === " " || line[j] === "\t" || line[j] === "\r")) {
        j++;
      }
      out += line.slice(i, j);
      i = j;
      continue;
    }

    // 字符串字面量（区分键名与普通值）
    if (ch === '"') {
      let j = i + 1;
      let escaped = false;
      while (j < n) {
        const c = line[j];
        if (escaped) {
          escaped = false;
        } else if (c === "\\") {
          escaped = true;
        } else if (c === '"') {
          j++;
          break;
        }
        j++;
      }
      const token = line.slice(i, j);
      // 向后跳过空白判断是否为对象键名（紧跟冒号）
      let k = j;
      while (k < n && (line[k] === " " || line[k] === "\t")) {
        k++;
      }
      const isKey = line[k] === ":";

      if (isKey) {
        const rawKeyName = token.length >= 2 ? token.slice(1, -1) : token;
        const isImportant =
          opts.highlightImportantKeys &&
          IMPORTANT_KEYS.has(rawKeyName.toLowerCase());
        const cls = isImportant ? "json-key-important" : "json-key";
        out += `<span class="${cls}">${renderTextWithSearch(
          token,
          opts.keywordLower,
          opts.hitCounter,
          opts.activeHitIndex
        )}</span>`;
      } else {
        out += `<span class="json-string">${renderTextWithSearch(
          token,
          opts.keywordLower,
          opts.hitCounter,
          opts.activeHitIndex
        )}</span>`;
      }
      i = j;
      continue;
    }

    // 数字字面量
    if (ch === "-" || (ch >= "0" && ch <= "9")) {
      const rest = line.slice(i);
      const numMatch = /^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/.exec(rest);
      if (numMatch) {
        const token = numMatch[0];
        out += `<span class="json-number">${renderTextWithSearch(
          token,
          opts.keywordLower,
          opts.hitCounter,
          opts.activeHitIndex
        )}</span>`;
        i += token.length;
        continue;
      }
    }

    // 布尔与 null 关键字
    if (line.startsWith("true", i) && !/[a-zA-Z0-9_]/.test(line[i + 4] ?? "")) {
      out += `<span class="json-boolean">${renderTextWithSearch(
        "true",
        opts.keywordLower,
        opts.hitCounter,
        opts.activeHitIndex
      )}</span>`;
      i += 4;
      continue;
    }
    if (line.startsWith("false", i) && !/[a-zA-Z0-9_]/.test(line[i + 5] ?? "")) {
      out += `<span class="json-boolean">${renderTextWithSearch(
        "false",
        opts.keywordLower,
        opts.hitCounter,
        opts.activeHitIndex
      )}</span>`;
      i += 5;
      continue;
    }
    if (line.startsWith("null", i) && !/[a-zA-Z0-9_]/.test(line[i + 4] ?? "")) {
      out += `<span class="json-null">${renderTextWithSearch(
        "null",
        opts.keywordLower,
        opts.hitCounter,
        opts.activeHitIndex
      )}</span>`;
      i += 4;
      continue;
    }

    // JSON 结构标点
    if ("{}[]:,".includes(ch)) {
      out += `<span class="json-punct">${ch}</span>`;
      i++;
      continue;
    }

    // 其他普通字符（容错非合法 JSON 片段）
    let j = i + 1;
    while (
      j < n &&
      !'"{}[]:, \t\r0123456789-tfn'.includes(line[j])
    ) {
      j++;
    }
    out += renderTextWithSearch(
      line.slice(i, j),
      opts.keywordLower,
      opts.hitCounter,
      opts.activeHitIndex
    );
    i = j;
  }

  return out;
}

/** 对整段 JSON / JSONL 文本生成带行分隔的高亮 HTML 与搜索计数 */
export function highlightJsonText(
  text: string,
  options: HighlightOptions = {}
): HighlightResult {
  const highlightImportantKeys = options.highlightImportantKeys ?? true;
  const keywordLower = (options.searchKeyword ?? "").trim().toLowerCase();
  const activeHitIndex = options.activeHitIndex ?? 0;
  const hitCounter = { count: 0 };

  if (!text) {
    return { html: "", matchCount: 0, lineCount: 0 };
  }

  const lines = text.split("\n");
  const highlightedLines = lines.map((line) =>
    highlightLine(line, {
      highlightImportantKeys,
      keywordLower,
      hitCounter,
      activeHitIndex,
    })
  );

  return {
    html: highlightedLines.join("\n"),
    matchCount: hitCounter.count,
    lineCount: lines.length,
  };
}

/** 尝试将任意文本尝试格式化为缩进 JSON（支持单 JSON、JSON 数组） */
export function tryPrettyJson(raw: string): { text: string; valid: boolean } {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { text: "", valid: false };
  }
  try {
    return { text: JSON.stringify(JSON.parse(trimmed), null, 2), valid: true };
  } catch {
    return { text: raw, valid: false };
  }
}
