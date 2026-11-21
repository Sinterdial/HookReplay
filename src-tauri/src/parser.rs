//! 日志流式扫描与分段偏移索引引擎（Parser）
//!
//! 核心职责：
//! 1. 收集并扫描配置 `inputs` 中的单个文件、多个文件或目录（支持递归与扩展名过滤）；
//! 2. 使用流式逐行读取（BufReader）构建分段（# / // 注释边界）与每条记录的字节偏移索引；
//! 3. 不将 15MB+ 大文件的完整内容常驻传给前端，仅在点击右侧 Inspector 时按偏移精确 Seek 读取；
//! 4. 支持前端「临时粘贴 JSON / JSONL」调试台解析。

use crate::config::{normalize_path_string, resolve_path, AppConfig};
use crate::matcher::extract_summary;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// 单条通知消息的轻量索引元数据（供前端列表高效渲染）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexedEntry {
    /// 全局序号（从 1 开始）
    pub index: usize,
    /// 所属分段编号（从 1 开始）
    pub segment: usize,
    /// 分段注释标题
    pub comment: String,
    /// 来源文件绝对路径（临时粘贴时为 "<scratchpad>"）
    pub source: String,
    /// 来源短文件名
    pub source_name: String,
    /// 在源文件中的行号（从 1 开始）
    pub line: usize,
    /// 该条记录在文件中的字节起始偏移（用于大文件 O(1) 懒加载读取）
    pub byte_offset: u64,
    /// 该条记录的字节长度
    pub byte_len: usize,
    /// 若来自临时粘贴或整文件 JSON 数组，可直接携带内联文本避免磁盘回读
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_raw: Option<String>,
    /// 提取出的事件类型（如 transfer.complete / new_entries）
    pub event_type: String,
    /// 提取出的可读标题
    pub title: String,
    /// 组合摘要标签
    pub label: String,
}

/// 分段摘要信息（用于左栏分段目录树与中栏分组卡片头）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentSummary {
    pub segment: usize,
    pub comment: String,
    pub source_name: String,
    pub start_line: usize,
    pub count: usize,
}

/// 解析失败行的记录
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseErrorItem {
    pub source: String,
    pub line: usize,
    pub error: String,
}

/// 扫描并索引后返回给前端的完整数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceScanResult {
    pub files: Vec<String>,
    pub missing_paths: Vec<String>,
    pub entries: Vec<IndexedEntry>,
    pub segments: Vec<SegmentSummary>,
    pub parse_errors: Vec<ParseErrorItem>,
    pub comments_skipped: usize,
    pub total_before_filter: usize,
    /// 出现过的所有事件类型去重列表（便于前端快速筛选胶囊）
    pub available_types: Vec<String>,
}

/// 收集配置 `inputs` 中所有待读取的文件路径
pub fn collect_input_files(cfg: &AppConfig, base_dir: &Path) -> (Vec<PathBuf>, Vec<String>) {
    let allowed_exts: HashSet<String> = cfg
        .extensions
        .iter()
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let mut files: Vec<PathBuf> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    fn walk_dir(dir: &Path, depth: usize, recursive: bool, allowed_exts: &HashSet<String>, out: &mut Vec<PathBuf>) {
        if depth > 6 {
            return;
        }
        let Ok(read_dir) = fs::read_dir(dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let full = entry.path();
            if full.is_dir() {
                if recursive {
                    walk_dir(&full, depth + 1, recursive, allowed_exts, out);
                }
                continue;
            }
            if !full.is_file() {
                continue;
            }
            if !allowed_exts.is_empty() {
                let ext = full
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !allowed_exts.contains(&ext) {
                    continue;
                }
            }
            out.push(full);
        }
    }

    for raw_input in &cfg.inputs {
        let trimmed = raw_input.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resolved = resolve_path(trimmed, base_dir);
        if !resolved.exists() {
            missing.push(resolved.to_string_lossy().to_string());
            continue;
        }
        if resolved.is_dir() {
            walk_dir(&resolved, 0, cfg.recursive, &allowed_exts, &mut files);
        } else {
            files.push(resolved);
        }
    }

    files.sort();
    files.dedup();
    (files, missing)
}

/// 内部中间条目结构，在过滤与重排编号前暂存
struct RawParsedItem {
    segment: usize,
    comment: String,
    source: String,
    source_name: String,
    line: usize,
    byte_offset: u64,
    byte_len: usize,
    inline_raw: Option<String>,
    raw_for_filter: String,
    event_type: String,
    title: String,
    label: String,
}

/// 去除单行注释前缀并提取干净的分段注释文本
fn extract_comment_text(line: &str, prefixes: &[String]) -> String {
    let mut val = line;
    for prefix in prefixes {
        if !prefix.is_empty() && val.starts_with(prefix) {
            val = &val[prefix.len()..];
            break;
        }
    }
    val.trim().to_string()
}

/// 判断一行是否以配置的注释前缀开头
fn is_comment_line(line: &str, prefixes: &[String]) -> bool {
    prefixes.iter().any(|p| !p.is_empty() && line.starts_with(p))
}

/// 流式扫描单个文件（auto 模式：先快速嗅探是否整文件单 JSON/数组，否则走零拷贝流式 JSONL 解析）
fn scan_single_file(
    file: &Path,
    cfg: &AppConfig,
    segment_base: usize,
    out_items: &mut Vec<RawParsedItem>,
    out_errors: &mut Vec<ParseErrorItem>,
) -> Result<(usize, usize), String> {
    let source_str = file.to_string_lossy().to_string();
    let source_name = file
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let format = cfg.format.trim().to_ascii_lowercase();
    let meta = fs::metadata(file).map_err(|e| format!("读取文件元数据失败（{}）：{}", source_str, e))?;
    let file_len = meta.len();

    // 仅当显式指定 json，或 auto 模式下文件小于 4MB 且首非空字符为 `[` 或尝试整包解析成功时，按整体 JSON 处理
    let try_whole_json = format == "json" || (format != "jsonl" && file_len <= 4 * 1024 * 1024);
    if try_whole_json {
        if let Ok(text) = fs::read_to_string(file) {
            let clean = text.trim_start_matches('\u{FEFF}').trim();
            if clean.is_empty() {
                return Ok((0, 0));
            }
            if let Ok(parsed) = serde_json::from_str::<Value>(clean) {
                let seg = segment_base + 1;
                match parsed {
                    Value::Array(arr) => {
                        for (idx, item) in arr.into_iter().enumerate() {
                            let raw_json = serde_json::to_string(&item).unwrap_or_default();
                            let summary = extract_summary(&item, &cfg.label_rules);
                            out_items.push(RawParsedItem {
                                segment: seg,
                                comment: source_name.clone(),
                                source: source_str.clone(),
                                source_name: source_name.clone(),
                                line: idx + 1,
                                byte_offset: 0,
                                byte_len: 0,
                                inline_raw: Some(raw_json.clone()),
                                raw_for_filter: raw_json,
                                event_type: summary.event_type,
                                title: summary.title,
                                label: summary.label,
                            });
                        }
                    }
                    other => {
                        let raw_json = serde_json::to_string(&other).unwrap_or_default();
                        let summary = extract_summary(&other, &cfg.label_rules);
                        out_items.push(RawParsedItem {
                            segment: seg,
                            comment: source_name.clone(),
                            source: source_str.clone(),
                            source_name: source_name.clone(),
                            line: 1,
                            byte_offset: 0,
                            byte_len: 0,
                            inline_raw: Some(raw_json.clone()),
                            raw_for_filter: raw_json,
                            event_type: summary.event_type,
                            title: summary.title,
                            label: summary.label,
                        });
                    }
                }
                return Ok((0, 1));
            } else if format == "json" {
                out_errors.push(ParseErrorItem {
                    source: source_str,
                    line: 1,
                    error: "整文件 JSON 解析失败".to_string(),
                });
                return Ok((0, 0));
            }
        }
    }

    // 流式逐行解析 JSONL，精确记录每行字节偏移以支持超大文件零负担回看
    let f = File::open(file).map_err(|e| format!("打开文件失败（{}）：{}", source_str, e))?;
    let mut reader = BufReader::with_capacity(64 * 1024, f);
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut current_offset: u64 = 0;
    let mut line_no: usize = 0;
    let mut local_segment: usize = 0;
    let mut last_was_comment = false;
    let mut segment_label = String::new();
    let mut comments_skipped: usize = 0;

    loop {
        buf.clear();
        let bytes_read = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("读取文件流失败（{}）：{}", source_str, e))?;
        if bytes_read == 0 {
            break;
        }
        line_no += 1;
        let line_start_offset = current_offset;
        current_offset += bytes_read as u64;

        let mut slice: &[u8] = &buf;
        // 剥离首行 UTF-8 BOM
        if line_no == 1 && slice.starts_with(&[0xEF, 0xBB, 0xBF]) {
            slice = &slice[3..];
        }
        let line_str = String::from_utf8_lossy(slice);
        let trimmed = line_str.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_comment_line(trimmed, &cfg.comment_prefixes) {
            comments_skipped += 1;
            if !last_was_comment {
                local_segment += 1;
                segment_label.clear();
            }
            last_was_comment = true;
            let text = extract_comment_text(trimmed, &cfg.comment_prefixes);
            if !text.is_empty() {
                if segment_label.is_empty() {
                    segment_label = text;
                } else {
                    segment_label.push_str(" / ");
                    segment_label.push_str(&text);
                }
            }
            continue;
        }
        last_was_comment = false;

        match serde_json::from_str::<Value>(trimmed) {
            Ok(parsed_val) => {
                // 若文件开头没有注释行直接出现数据，确保首个分段编号至少为 1
                let seg_num = segment_base + if local_segment == 0 { 1 } else { local_segment };
                if local_segment == 0 {
                    local_segment = 1;
                }
                let summary = extract_summary(&parsed_val, &cfg.label_rules);
                out_items.push(RawParsedItem {
                    segment: seg_num,
                    comment: if segment_label.is_empty() {
                        source_name.clone()
                    } else {
                        segment_label.clone()
                    },
                    source: source_str.clone(),
                    source_name: source_name.clone(),
                    line: line_no,
                    byte_offset: line_start_offset,
                    byte_len: bytes_read,
                    inline_raw: None,
                    raw_for_filter: trimmed.to_string(),
                    event_type: summary.event_type,
                    title: summary.title,
                    label: summary.label,
                });
            }
            Err(err) => {
                out_errors.push(ParseErrorItem {
                    source: source_str.clone(),
                    line: line_no,
                    error: err.to_string(),
                });
            }
        }
    }

    Ok((comments_skipped, local_segment.max(1)))
}

/// 执行过滤规则（types / include / exclude / reverse / limit）并构建前端可用摘要结构
fn finalize_scan_result(
    files: Vec<String>,
    missing_paths: Vec<String>,
    raw_items: Vec<RawParsedItem>,
    parse_errors: Vec<ParseErrorItem>,
    comments_skipped: usize,
    cfg: &AppConfig,
) -> Result<SourceScanResult, String> {
    let total_before_filter = raw_items.len();

    let include_re = if cfg.include.trim().is_empty() {
        None
    } else {
        Some(
            Regex::new(&cfg.include)
                .map_err(|e| format!("包含正则（include）语法错误：{}", e))?,
        )
    };
    let exclude_re = if cfg.exclude.trim().is_empty() {
        None
    } else {
        Some(
            Regex::new(&cfg.exclude)
                .map_err(|e| format!("排除正则（exclude）语法错误：{}", e))?,
        )
    };

    let wanted_types: HashSet<String> = cfg
        .types
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut available_type_set = HashSet::new();
    let mut filtered: Vec<RawParsedItem> = Vec::with_capacity(raw_items.len());

    for item in raw_items {
        available_type_set.insert(item.event_type.clone());
        if !wanted_types.is_empty() && !wanted_types.contains(&item.event_type) {
            continue;
        }
        if let Some(ref re) = include_re {
            if !re.is_match(&item.raw_for_filter) {
                continue;
            }
        }
        if let Some(ref re) = exclude_re {
            if re.is_match(&item.raw_for_filter) {
                continue;
            }
        }
        filtered.push(item);
    }

    if cfg.reverse {
        filtered.reverse();
    }
    if cfg.limit > 0 && filtered.len() > cfg.limit {
        filtered.truncate(cfg.limit);
    }

    let mut seg_map = BTreeMap::<usize, SegmentSummary>::new();
    let mut entries = Vec::with_capacity(filtered.len());

    for (idx, item) in filtered.into_iter().enumerate() {
        let seg_entry = seg_map.entry(item.segment).or_insert_with(|| SegmentSummary {
            segment: item.segment,
            comment: item.comment.clone(),
            source_name: item.source_name.clone(),
            start_line: item.line,
            count: 0,
        });
        seg_entry.count += 1;

        entries.push(IndexedEntry {
            index: idx + 1,
            segment: item.segment,
            comment: item.comment,
            source: item.source,
            source_name: item.source_name,
            line: item.line,
            byte_offset: item.byte_offset,
            byte_len: item.byte_len,
            inline_raw: item.inline_raw,
            event_type: item.event_type,
            title: item.title,
            label: item.label,
        });
    }

    let mut available_types: Vec<String> = available_type_set.into_iter().collect();
    available_types.sort();

    Ok(SourceScanResult {
        files,
        missing_paths,
        entries,
        segments: seg_map.into_values().collect(),
        parse_errors,
        comments_skipped,
        total_before_filter,
        available_types,
    })
}

/// 扫描当前配置下的全部输入日志并返回索引
pub fn scan_sources(cfg: &AppConfig, base_dir: &Path) -> Result<SourceScanResult, String> {
    let (files, missing_paths) = collect_input_files(cfg, base_dir);
    let mut raw_items = Vec::new();
    let mut parse_errors = Vec::new();
    let mut total_comments_skipped = 0;
    let mut segment_base = 0;

    for file in &files {
        match scan_single_file(file, cfg, segment_base, &mut raw_items, &mut parse_errors) {
            Ok((skipped, seg_count)) => {
                total_comments_skipped += skipped;
                segment_base += seg_count.max(1);
            }
            Err(err) => {
                parse_errors.push(ParseErrorItem {
                    source: file.to_string_lossy().to_string(),
                    line: 0,
                    error: err,
                });
            }
        }
    }

    let file_strings = files
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    finalize_scan_result(
        file_strings,
        missing_paths,
        raw_items,
        parse_errors,
        total_comments_skipped,
        cfg,
    )
}

/// 解析前端临时粘贴台（Scratchpad）输入文本（支持单 JSON、JSON 数组或带注释 JSONL）
pub fn parse_scratchpad(raw_text: &str, cfg: &AppConfig) -> Result<SourceScanResult, String> {
    let clean = raw_text.trim_start_matches('\u{FEFF}').trim();
    if clean.is_empty() {
        return Err("粘贴内容为空".to_string());
    }
    let mut raw_items = Vec::new();
    let mut parse_errors = Vec::new();
    let mut comments_skipped = 0;

    // 尝试整体解析
    if clean.starts_with('{') || clean.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Value>(clean) {
            match parsed {
                Value::Array(arr) => {
                    for (idx, item) in arr.into_iter().enumerate() {
                        let raw_json = serde_json::to_string(&item).unwrap_or_default();
                        let summary = extract_summary(&item, &cfg.label_rules);
                        raw_items.push(RawParsedItem {
                            segment: 1,
                            comment: "临时调试台 (JSON数组)".to_string(),
                            source: "<scratchpad>".to_string(),
                            source_name: "临时调试".to_string(),
                            line: idx + 1,
                            byte_offset: 0,
                            byte_len: 0,
                            inline_raw: Some(raw_json.clone()),
                            raw_for_filter: raw_json,
                            event_type: summary.event_type,
                            title: summary.title,
                            label: summary.label,
                        });
                    }
                }
                other => {
                    let raw_json = serde_json::to_string(&other).unwrap_or_default();
                    let summary = extract_summary(&other, &cfg.label_rules);
                    raw_items.push(RawParsedItem {
                        segment: 1,
                        comment: "临时调试台".to_string(),
                        source: "<scratchpad>".to_string(),
                        source_name: "临时调试".to_string(),
                        line: 1,
                        byte_offset: 0,
                        byte_len: 0,
                        inline_raw: Some(raw_json.clone()),
                        raw_for_filter: raw_json,
                        event_type: summary.event_type,
                        title: summary.title,
                        label: summary.label,
                    });
                }
            }
            return finalize_scan_result(
                vec!["<scratchpad>".to_string()],
                Vec::new(),
                raw_items,
                parse_errors,
                0,
                cfg,
            );
        }
    }

    // 否则按 JSONL 多行逐条解析
    let mut seg = 0usize;
    let mut last_was_comment = false;
    let mut comment_buf = String::new();
    for (idx, line) in clean.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_comment_line(trimmed, &cfg.comment_prefixes) {
            comments_skipped += 1;
            if !last_was_comment {
                seg += 1;
                comment_buf.clear();
            }
            last_was_comment = true;
            let txt = extract_comment_text(trimmed, &cfg.comment_prefixes);
            if !txt.is_empty() {
                if !comment_buf.is_empty() {
                    comment_buf.push_str(" / ");
                }
                comment_buf.push_str(&txt);
            }
            continue;
        }
        last_was_comment = false;
        let active_seg = if seg == 0 {
            seg = 1;
            1
        } else {
            seg
        };
        match serde_json::from_str::<Value>(trimmed) {
            Ok(v) => {
                let summary = extract_summary(&v, &cfg.label_rules);
                raw_items.push(RawParsedItem {
                    segment: active_seg,
                    comment: if comment_buf.is_empty() {
                        "临时调试台".to_string()
                    } else {
                        comment_buf.clone()
                    },
                    source: "<scratchpad>".to_string(),
                    source_name: "临时调试".to_string(),
                    line: idx + 1,
                    byte_offset: 0,
                    byte_len: 0,
                    inline_raw: Some(trimmed.to_string()),
                    raw_for_filter: trimmed.to_string(),
                    event_type: summary.event_type,
                    title: summary.title,
                    label: summary.label,
                });
            }
            Err(err) => {
                parse_errors.push(ParseErrorItem {
                    source: "<scratchpad>".to_string(),
                    line: idx + 1,
                    error: err.to_string(),
                });
            }
        }
    }

    finalize_scan_result(
        vec!["<scratchpad>".to_string()],
        Vec::new(),
        raw_items,
        parse_errors,
        comments_skipped,
        cfg,
    )
}

/// 按偏移精确读取某一条记录的原始 JSON 字符串（支持大文件 O(1) 懒加载）
pub fn read_entry_raw(entry: &IndexedEntry) -> Result<String, String> {
    if let Some(ref inline) = entry.inline_raw {
        return Ok(inline.clone());
    }
    let mut file = File::open(&entry.source)
        .map_err(|e| format!("打开文件读取详情失败（{}）：{}", entry.source, e))?;
    file.seek(SeekFrom::Start(entry.byte_offset))
        .map_err(|e| format!("定位文件字节偏移失败：{}", e))?;
    let mut buf = vec![0u8; entry.byte_len];
    file.read_exact(&mut buf)
        .map_err(|e| format!("读取条目原始字节失败：{}", e))?;
    let text = String::from_utf8_lossy(&buf);
    Ok(text.trim_start_matches('\u{FEFF}').trim().to_string())
}

/// 临时消息持久化保存请求参数
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistScratchpadRequest {
    /// 原始粘贴文本（支持单条格式化 JSON、JSON 数组或带注释 JSONL）
    pub raw_text: String,
    /// 目标模式："file"（写入指定文件） | "directory"（写入目录内指定/新建文件）
    pub target_mode: String,
    /// 目标文件或文件夹路径
    pub target_path: String,
    /// 当 target_mode 为 directory 时的文件名（留空则按分类自动命名）
    pub new_file_name: Option<String>,
    /// 所属分类名称（写入为注释分段标题，例如 `# RSS消息`）
    pub category: String,
    /// 目标文件中若已存在同名分类，是否直接归类插入到该分段尾部
    pub insert_into_existing_category: bool,
    /// 分段注释前缀（默认 `#`）
    pub comment_prefix: String,
}

/// 临时消息持久化保存结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistScratchpadResult {
    pub saved_path: String,
    pub written_count: usize,
    pub category: String,
    pub inserted_into_existing: bool,
}

/// 将临时粘贴文本中的合法 JSON 条目提取并归一化为单行 JSONL 字符串列表
pub fn extract_normalized_json_lines(
    raw_text: &str,
    comment_prefixes: &[String],
) -> Result<Vec<String>, String> {
    let mut cfg = AppConfig::default();
    if !comment_prefixes.is_empty() {
        cfg.comment_prefixes = comment_prefixes.to_vec();
    }
    let parsed = parse_scratchpad(raw_text, &cfg)?;
    if let Some(first_err) = parsed.parse_errors.first() {
        return Err(format!(
            "第 {} 行 JSON 格式有误，无法保存：{}",
            first_err.line, first_err.error
        ));
    }
    if parsed.entries.is_empty() {
        return Err("未解析出任何有效的 JSON 消息条目".to_string());
    }

    let mut lines = Vec::with_capacity(parsed.entries.len());
    for entry in parsed.entries {
        let raw = entry.inline_raw.unwrap_or_default();
        let val: Value = serde_json::from_str(&raw)
            .map_err(|e| format!("第 {} 条 JSON 校验失败：{}", entry.line, e))?;
        let compact = serde_json::to_string(&val)
            .map_err(|e| format!("序列化压缩 JSON 失败：{}", e))?;
        lines.push(compact);
    }
    Ok(lines)
}

/// 扫描指定日志文件已有的注释分类列表（保持出现顺序并去重）
pub fn inspect_log_categories(
    file_path: &Path,
    comment_prefixes: &[String],
) -> Result<Vec<String>, String> {
    if !file_path.is_file() {
        return Ok(Vec::new());
    }
    let prefixes: Vec<String> = if comment_prefixes.is_empty() {
        vec!["#".to_string(), "//".to_string()]
    } else {
        comment_prefixes.to_vec()
    };

    let f = File::open(file_path)
        .map_err(|e| format!("读取日志文件分类失败（{}）：{}", file_path.display(), e))?;
    let mut reader = BufReader::with_capacity(64 * 1024, f);
    let mut buf = Vec::with_capacity(1024);
    let mut is_first = true;
    let mut last_was_comment = false;
    let mut cur_label = String::new();
    let mut categories = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    let flush_label = |label: &mut String, out: &mut Vec<String>, seen: &mut HashSet<String>| {
        let trimmed = label.trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
        label.clear();
    };

    loop {
        buf.clear();
        let n = reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| format!("读取文件流失败：{}", e))?;
        if n == 0 {
            break;
        }
        let mut slice: &[u8] = &buf;
        if is_first {
            is_first = false;
            if slice.starts_with(&[0xEF, 0xBB, 0xBF]) {
                slice = &slice[3..];
            }
        }
        let line = String::from_utf8_lossy(slice);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if last_was_comment {
                flush_label(&mut cur_label, &mut categories, &mut seen);
                last_was_comment = false;
            }
            continue;
        }
        if is_comment_line(trimmed, &prefixes) {
            last_was_comment = true;
            let text = extract_comment_text(trimmed, &prefixes);
            if !text.is_empty() {
                if cur_label.is_empty() {
                    cur_label = text;
                } else {
                    cur_label.push_str(" / ");
                    cur_label.push_str(&text);
                }
            }
        } else if last_was_comment {
            flush_label(&mut cur_label, &mut categories, &mut seen);
            last_was_comment = false;
        }
    }
    if last_was_comment {
        flush_label(&mut cur_label, &mut categories, &mut seen);
    }

    Ok(categories)
}

/// 列出指定测试目录下可追加写入的日志文件列表
pub fn list_dir_log_files(dir_path: &Path, extensions: &[String]) -> Result<Vec<String>, String> {
    if !dir_path.is_dir() {
        return Ok(Vec::new());
    }
    let allowed: HashSet<String> = if extensions.is_empty() {
        ["txt", "log", "json", "jsonl"]
            .into_iter()
            .map(String::from)
            .collect()
    } else {
        extensions
            .iter()
            .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    };

    let mut files = Vec::<PathBuf>::new();
    let entries = fs::read_dir(dir_path)
        .map_err(|e| format!("读取目录失败（{}）：{}", dir_path.display(), e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if allowed.is_empty() || allowed.contains(&ext) {
            files.push(p);
        }
    }
    files.sort();
    Ok(files
        .into_iter()
        .map(|p| normalize_path_string(&p.to_string_lossy()))
        .collect())
}

/// 清洗用于新建文件名的字符串（剔除 Windows 非法路径字符）
fn sanitize_filename(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if trimmed.is_empty() {
        "custom_cases".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 将临时粘贴的 JSON / JSONL 归类持久化追加到指定测试文件或测试目录
pub fn persist_scratchpad_messages(
    req: &PersistScratchpadRequest,
    comment_prefixes: &[String],
    base_dir: &Path,
) -> Result<PersistScratchpadResult, String> {
    let prefixes: Vec<String> = if comment_prefixes.is_empty() {
        vec!["#".to_string(), "//".to_string()]
    } else {
        comment_prefixes.to_vec()
    };
    let json_lines = extract_normalized_json_lines(&req.raw_text, &prefixes)?;

    let target_raw = req.target_path.trim();
    if target_raw.is_empty() {
        return Err("请先指定保存的目标测试文件或文件夹路径".to_string());
    }

    let resolved_target = resolve_path(target_raw, base_dir);
    let final_file = if req.target_mode.eq_ignore_ascii_case("directory") {
        fs::create_dir_all(&resolved_target).map_err(|e| {
            format!(
                "创建目标测试目录失败（{}）：{}",
                resolved_target.display(),
                e
            )
        })?;
        let fname_raw = req
            .new_file_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                let base_name = if req.category.trim().is_empty() {
                    "custom_cases".to_string()
                } else {
                    sanitize_filename(&req.category)
                };
                format!("{}.txt", base_name)
            });
        let fname_path = Path::new(&fname_raw);
        let safe_name = if fname_path.extension().is_some() {
            sanitize_filename(fname_raw.trim_end_matches(|c| c == '\\' || c == '/'))
        } else {
            format!("{}.txt", sanitize_filename(&fname_raw))
        };
        resolved_target.join(safe_name)
    } else {
        if let Some(parent) = resolved_target.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("创建目标文件目录失败（{}）：{}", parent.display(), e))?;
            }
        }
        resolved_target
    };

    let comment_prefix = if req.comment_prefix.trim().is_empty() {
        "#".to_string()
    } else {
        req.comment_prefix.trim().to_string()
    };
    let category = req.category.trim().to_string();
    let written_count = json_lines.len();
    let mut inserted_into_existing = false;

    if final_file.is_file() {
        let existing_raw = fs::read_to_string(&final_file)
            .map_err(|e| format!("读取目标测试文件失败（{}）：{}", final_file.display(), e))?;
        let line_sep = if existing_raw.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let clean_existing = existing_raw.trim_start_matches('\u{FEFF}');
        let has_bom = existing_raw.starts_with('\u{FEFF}');

        // 尝试按分类插入已有分段末尾
        if req.insert_into_existing_category && !category.is_empty() {
            let mut lines: Vec<String> = clean_existing
                .lines()
                .map(|l| l.trim_end_matches('\r').to_string())
                .collect();

            // 定位首个匹配该分类名称的注释分段区块
            let mut idx = 0usize;
            let mut matched_seg_end_header: Option<usize> = None;
            while idx < lines.len() {
                let t = lines[idx].trim();
                if is_comment_line(t, &prefixes) {
                    let block_start = idx;
                    let mut single_texts = Vec::new();
                    while idx < lines.len() && is_comment_line(lines[idx].trim(), &prefixes) {
                        let txt = extract_comment_text(lines[idx].trim(), &prefixes);
                        if !txt.is_empty() {
                            single_texts.push(txt);
                        }
                        idx += 1;
                    }
                    let combined = single_texts.join(" / ");
                    if combined == category || single_texts.iter().any(|s| s == &category) {
                        matched_seg_end_header = Some(idx);
                        break;
                    }
                    let _ = block_start;
                } else {
                    idx += 1;
                }
            }

            if let Some(seg_body_start) = matched_seg_end_header {
                let mut scan = seg_body_start;
                let mut last_non_empty = seg_body_start;
                while scan < lines.len() {
                    let t = lines[scan].trim();
                    if is_comment_line(t, &prefixes) {
                        break;
                    }
                    if !t.is_empty() {
                        last_non_empty = scan + 1;
                    }
                    scan += 1;
                }
                for (offset, item) in json_lines.iter().enumerate() {
                    lines.insert(last_non_empty + offset, item.clone());
                }
                let mut out = String::new();
                if has_bom {
                    out.push('\u{FEFF}');
                }
                out.push_str(&lines.join(line_sep));
                out.push_str(line_sep);
                fs::write(&final_file, out).map_err(|e| {
                    format!("写入归类消息到文件失败（{}）：{}", final_file.display(), e)
                })?;
                inserted_into_existing = true;
            }
        }

        if !inserted_into_existing {
            let mut out = existing_raw;
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(line_sep);
            }
            if !category.is_empty() {
                if !out.trim().is_empty() && !out.ends_with("\n\n") && !out.ends_with("\r\n\r\n") {
                    out.push_str(line_sep);
                }
                out.push_str(&format!("{} {}{}", comment_prefix, category, line_sep));
            }
            for item in &json_lines {
                out.push_str(item);
                out.push_str(line_sep);
            }
            fs::write(&final_file, out).map_err(|e| {
                format!("追加写入测试文件失败（{}）：{}", final_file.display(), e)
            })?;
        }
    } else {
        let mut out = String::new();
        if !category.is_empty() {
            out.push_str(&format!("{} {}\n", comment_prefix, category));
        }
        for item in &json_lines {
            out.push_str(item);
            out.push('\n');
        }
        fs::write(&final_file, out)
            .map_err(|e| format!("创建测试文件失败（{}）：{}", final_file.display(), e))?;
    }

    let saved_path = normalize_path_string(&final_file.to_string_lossy());
    Ok(PersistScratchpadResult {
        saved_path,
        written_count,
        category,
        inserted_into_existing,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_segmented_jsonl_and_lazy_read() {
        let temp_dir = std::env::temp_dir().join(format!(
            "hookreplay_scan_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let sample = temp_dir.join("sample_segmented.txt");

        let mut raw_content = String::new();
        for seg in 1..=8 {
            raw_content.push_str(&format!("# 批次 {}：中文标题验证\n", seg));
            for item in 1..=8 {
                raw_content.push_str(&format!(
                    r#"{{"type":"transfer.complete","data":{{"mediainfo":{{"title_year":"示例作品 {} (2024)"}}}}}}"#,
                    seg * 10 + item
                ));
                raw_content.push('\n');
            }
            raw_content.push('\n');
        }
        fs::write(&sample, raw_content).unwrap();

        let mut cfg = AppConfig::default();
        cfg.inputs = vec![sample.to_string_lossy().to_string()];
        let res = scan_sources(&cfg, &temp_dir).unwrap();
        assert_eq!(res.entries.len(), 64, "应能扫描出全部 64 条分段测试条目");
        assert_eq!(res.segments.len(), 8, "应正确按 # 注释拆出 8 个分段");

        // 验证通过字节偏移精确回读中间条目（含中文）无乱码且为合法 JSON
        let target = &res.entries[res.entries.len() / 2];
        let raw = read_entry_raw(target).unwrap();
        let parsed: Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed.is_object());
        assert!(raw.contains("示例作品"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_persist_scratchpad_with_category_and_pretty_json() {
        let temp_dir = std::env::temp_dir().join(format!(
            "hookreplay_parser_test_{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let target_file = temp_dir.join("cases.txt");
        fs::write(
            &target_file,
            "# RSS消息\n{\"event\":\"a\"}\n\n# 告警消息\n{\"event\":\"b\"}\n",
        )
        .unwrap();

        let prefixes = vec!["#".to_string(), "//".to_string()];
        let cats = inspect_log_categories(&target_file, &prefixes).unwrap();
        assert_eq!(cats, vec!["RSS消息".to_string(), "告警消息".to_string()]);

        // 1. 多行美化 JSON 归类插入到已有 `# RSS消息` 末尾
        let req1 = PersistScratchpadRequest {
            raw_text: "{\n  \"event\": \"rss_new\",\n  \"title\": \"测试条目\"\n}".to_string(),
            target_mode: "file".to_string(),
            target_path: target_file.to_string_lossy().to_string(),
            new_file_name: None,
            category: "RSS消息".to_string(),
            insert_into_existing_category: true,
            comment_prefix: "#".to_string(),
        };
        let res1 = persist_scratchpad_messages(&req1, &prefixes, &temp_dir).unwrap();
        assert!(res1.inserted_into_existing);
        assert_eq!(res1.written_count, 1);

        let content_after = fs::read_to_string(&target_file).unwrap();
        let rss_pos = content_after.find("rss_new").unwrap();
        let alert_pos = content_after.find("# 告警消息").unwrap();
        assert!(rss_pos < alert_pos, "新条目应插入到 RSS消息 段内且位于下一分段之前");

        // 2. 目录模式下新建文件并能被 scan_sources 正确回读
        let req2 = PersistScratchpadRequest {
            raw_text: "[{\"type\":\"ping\",\"id\":1},{\"type\":\"pong\",\"id\":2}]".to_string(),
            target_mode: "directory".to_string(),
            target_path: temp_dir.to_string_lossy().to_string(),
            new_file_name: Some("new_batch.txt".to_string()),
            category: "新增测试分类".to_string(),
            insert_into_existing_category: false,
            comment_prefix: "#".to_string(),
        };
        let res2 = persist_scratchpad_messages(&req2, &prefixes, &temp_dir).unwrap();
        assert_eq!(res2.written_count, 2);
        assert!(!res2.inserted_into_existing);

        let mut cfg = AppConfig::default();
        cfg.inputs = vec![res2.saved_path.clone()];
        let scan = scan_sources(&cfg, &temp_dir).unwrap();
        assert_eq!(scan.entries.len(), 2);
        assert_eq!(scan.segments[0].comment, "新增测试分类");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
