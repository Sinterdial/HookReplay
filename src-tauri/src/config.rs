//! 配置管理模块（Config Engine）
//!
//! 核心职责：
//! 1. 完整建模并兼容原有 `notify-test.config.yaml` 的全部 28 个字段与 V2 通用扩展字段；
//! 2. 实现「YAML 配置文件 > GUI 兜底配置 > 系统默认值」的逐字段优先级合并算法；
//! 3. 支持环境变量与动态占位符插值（`{{VAR}}`、`{{$timestamp}}`、`{{$isoTime}}`、`{{$uuid}}`）；
//! 4. 管理系统配置目录（`%APPDATA%/com.hookreplay.studio/gui-config.json`）与多档案（Profiles）。

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// 单个预设目标入口（Endpoint Preset），用于在顶栏一键切换不同环境地址
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EndpointPreset {
    /// 入口展示名称（例如：正式网关 / 编辑器测试入口 / 局域网直连）
    pub name: String,
    /// 目标 Webhook / HTTP 完整 URL
    pub url: String,
}

/// 认证配置（完全兼容原版 none / basic / bearer / header / query 五种模式）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AuthConfig {
    /// 认证类型：none / basic / bearer / header / query
    #[serde(rename = "type")]
    pub auth_type: String,
    /// Basic 认证用户名
    pub username: String,
    /// Basic 认证密码
    pub password: String,
    /// Bearer / Header / Query 认证令牌
    pub token: String,
    /// Header 认证时的自定义请求头名称（默认 Authorization）
    pub header_name: String,
    /// Query 认证时的 URL 参数名称（默认 token）
    pub query_name: String,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: "none".to_string(),
            username: String::new(),
            password: String::new(),
            token: String::new(),
            header_name: "Authorization".to_string(),
            query_name: "token".to_string(),
        }
    }
}

/// Webhook 请求签名器配置（V2 扩展：用于 GitHub / Gitea / 自建验签服务）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct SignatureConfig {
    /// 是否启用 HMAC 签名计算
    pub enabled: bool,
    /// 签名算法：hmac-sha256 / hmac-sha1
    pub algorithm: String,
    /// 写入签名的请求头名称（默认 X-Hub-Signature-256）
    pub header_name: String,
    /// 签名值前缀（例如 sha256=）
    pub prefix: String,
    /// HMAC 密钥（建议写 {{WEBHOOK_SECRET}} 从环境变量读取）
    pub secret: String,
}

impl Default for SignatureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: "hmac-sha256".to_string(),
            header_name: "X-Hub-Signature-256".to_string(),
            prefix: "sha256=".to_string(),
            secret: String::new(),
        }
    }
}

/// 精细化测试标记注入配置（V2 扩展：可替代单一的 markTest 布尔开关）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TestMarkConfig {
    /// 注入位置：body（JSON体） / header（请求头） / query（URL参数） / none（不注入）
    pub mode: String,
    /// 键名或点分路径（例如 "__test"、"meta.is_test"、"X-Test-Mode"）
    pub field: String,
    /// 注入值（支持布尔、数字或字符串，默认为 true）
    pub value: serde_json::Value,
}

impl Default for TestMarkConfig {
    fn default() -> Self {
        Self {
            mode: "body".to_string(),
            field: "__test".to_string(),
            value: serde_json::Value::Bool(true),
        }
    }
}

/// 条目摘要提取规则（V2 扩展：让工具不局限于单一 n8n 工作流的 JSON 结构）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct LabelRulesConfig {
    /// 提取预设模式：auto（自动兼顾 n8n-renotify 与通用结构）/ n8n-renotify / generic
    pub preset: String,
    /// 用于提取事件类型的 JSON 路径列表（按优先级从左到右查找）
    pub type_paths: Vec<String>,
    /// 用于提取展示标题的 JSON 路径列表（按优先级从左到右查找）
    pub title_paths: Vec<String>,
}

impl Default for LabelRulesConfig {
    fn default() -> Self {
        Self {
            preset: "auto".to_string(),
            type_paths: vec![
                "type".to_string(),
                "event_type".to_string(),
                "event".to_string(),
                "action".to_string(),
                "status".to_string(),
            ],
            title_paths: vec![
                "data.mediainfo.title_year".to_string(),
                "data.title".to_string(),
                "data.subscribe_info.name".to_string(),
                "entries[0].title".to_string(),
                "entry.title".to_string(),
                "title".to_string(),
                "name".to_string(),
                "subject".to_string(),
            ],
        }
    }
}

/// 响应体 JSON 细粒度断言规则（例如识别 n8n 去重返回的 deduped: true）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct BodyRule {
    /// 响应体中的 JSON 点路径（例如 "deduped" 或 "ok"）
    pub path: String,
    /// 期望匹配的值
    pub equals: serde_json::Value,
    /// 命中后在前端显示的中文徽章文本（例如 "幂等跳过"）
    pub badge: String,
    /// 视觉语义：success / info / warning / error
    pub tone: String,
}

impl Default for BodyRule {
    fn default() -> Self {
        Self {
            path: String::new(),
            equals: serde_json::Value::Bool(true),
            badge: String::new(),
            tone: "info".to_string(),
        }
    }
}

/// 响应判定与断言配置
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AssertionsConfig {
    /// 视为请求成功的 HTTP 状态码列表（默认 [200]）
    pub expect: Vec<u16>,
    /// 视为「未匹配 / 软跳过」的状态码列表（默认 [400]，strict 为 true 时升级为失败）
    pub soft_fail_status: Vec<u16>,
    /// 是否严格模式：开启后 soft_fail_status 也算作失败
    pub strict: bool,
    /// 响应体 JSON 规则列表
    pub body_rules: Vec<BodyRule>,
}

impl Default for AssertionsConfig {
    fn default() -> Self {
        Self {
            expect: vec![200],
            soft_fail_status: vec![400],
            strict: false,
            body_rules: vec![BodyRule {
                path: "deduped".to_string(),
                equals: serde_json::Value::Bool(true),
                badge: "幂等跳过".to_string(),
                tone: "info".to_string(),
            }],
        }
    }
}

/// 应用核心配置结构体（100% 覆盖原有 notify-test.config.yaml 全部字段 + V2 扩展字段）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    /// 档案名称（可选）
    pub name: String,
    /// 档案描述（可选）
    pub description: String,
    /// 目标 Webhook / HTTP 地址（支持 {{VAR}} 插值）
    pub url: String,
    /// HTTP 请求方法（POST / PUT / PATCH / GET）
    pub method: String,
    /// 预设目标入口列表（用于顶栏一键切换）
    pub endpoints: Vec<EndpointPreset>,
    /// 认证配置
    pub auth: AuthConfig,
    /// 自定义请求头
    pub headers: BTreeMap<String, String>,
    /// Webhook HMAC 签名配置
    pub signature: SignatureConfig,
    /// 单次请求超时时间（毫秒）
    pub timeout_ms: u64,
    /// 失败重试次数（仅针对网络错误与 5xx）
    pub retries: u32,
    /// 重试等待间隔（毫秒）
    pub retry_delay_ms: u64,
    /// 同分段内并发请求数（入库聚合测试建议保持 1）
    pub concurrency: usize,
    /// 同分段内每条请求的间隔（毫秒）
    pub delay_ms: u64,
    /// 分段与分段之间的等待时间（毫秒）
    pub segment_wait_ms: u64,
    /// 逐段交互确认开关（true 时每段开始前暂停询问 y/n/a/q）
    pub interactive: bool,
    /// 兼容旧版：是否在 JSON 根注入 __test: true
    pub mark_test: bool,
    /// V2 精细化测试标记配置（若配置则优先生效）
    pub test_mark: Option<TestMarkConfig>,
    /// 是否跳过自签名 HTTPS 证书校验
    pub insecure: bool,
    /// 输入日志文件路径或目录列表
    pub inputs: Vec<String>,
    /// 扫描目录时是否递归子目录
    pub recursive: bool,
    /// 扫描目录时允许的扩展名
    pub extensions: Vec<String>,
    /// 数据解析格式：auto / jsonl / json
    pub format: String,
    /// 注释行前缀（兼作分段切分标记）
    pub comment_prefixes: Vec<String>,
    /// 仅发送指定类型列表（为空表示不限制）
    pub types: Vec<String>,
    /// 仅发送匹配该正则的条目
    pub include: String,
    /// 排除匹配该正则的条目
    pub exclude: String,
    /// 最大发送条数（0 表示不限制）
    pub limit: usize,
    /// 是否倒序发送
    pub reverse: bool,
    /// 兼容旧版顶层 expect（与 assertions.expect 联动）
    pub expect: Vec<u16>,
    /// 兼容旧版顶层 strict（与 assertions.strict 联动）
    pub strict: bool,
    /// V2 通用摘要提取规则
    pub label_rules: LabelRulesConfig,
    /// V2 响应判定与断言配置
    pub assertions: AssertionsConfig,
    /// JSON 报告输出路径（留空不写）
    pub report: String,
    /// 单条响应落盘目录（留空不保存）
    pub save_response_dir: String,
    /// 静默模式开关（保留兼容 CLI 配置）
    pub quiet: bool,
    /// 局部环境变量映射（用于 {{VAR}} 模板插值）
    pub variables: BTreeMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "默认配置档案".to_string(),
            description: String::new(),
            url: String::new(),
            method: "POST".to_string(),
            endpoints: Vec::new(),
            auth: AuthConfig::default(),
            headers: BTreeMap::new(),
            signature: SignatureConfig::default(),
            timeout_ms: 15_000,
            retries: 0,
            retry_delay_ms: 800,
            concurrency: 1,
            delay_ms: 100,
            segment_wait_ms: 0,
            interactive: true,
            mark_test: true,
            test_mark: None,
            insecure: false,
            inputs: Vec::new(),
            recursive: false,
            extensions: vec!["txt".to_string(), "log".to_string(), "json".to_string()],
            format: "auto".to_string(),
            comment_prefixes: vec!["#".to_string(), "//".to_string()],
            types: Vec::new(),
            include: String::new(),
            exclude: String::new(),
            limit: 0,
            reverse: false,
            expect: vec![200],
            strict: false,
            label_rules: LabelRulesConfig::default(),
            assertions: AssertionsConfig::default(),
            report: String::new(),
            save_response_dir: String::new(),
            quiet: false,
            variables: BTreeMap::new(),
        }
    }
}

/// 用户手动挂载的配置文件条目（支持自定义展示别名）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct MountedProfile {
    /// 配置文件绝对路径
    pub path: String,
    /// 用户自定义显示名称（留空则自动使用文件名）
    pub alias: String,
}

impl Default for MountedProfile {
    fn default() -> Self {
        Self {
            path: String::new(),
            alias: String::new(),
        }
    }
}

/// 内置配置模板描述信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigTemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub suggested_file_name: String,
    pub content: String,
}

/// 发现的 YAML 配置档案信息
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub name: String,
    pub path: String,
    #[serde(default = "default_true")]
    pub exists: bool,
    #[serde(default)]
    pub source_kind: String,
}

fn default_true() -> bool {
    true
}

fn default_font_size() -> u8 {
    14
}

fn default_code_font_size() -> u8 {
    13
}

fn default_aurora_intensity() -> String {
    "rich".to_string()
}

/// GUI 持久化状态（保存在系统用户目录 gui-config.json 中）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GuiStateFile {
    /// 界面主题：system（跟随系统） / light / dark
    pub theme: String,
    /// 当前绑定的 YAML 文件路径（None 表示纯 GUI 模式）
    pub active_yaml_path: Option<String>,
    /// 兼容旧版：用户曾手动打开或发现的额外 YAML 路径列表
    pub custom_yaml_paths: Vec<String>,
    /// 用户手动挂载的配置文件列表（支持自定义别名与一键切换）
    pub mounted_profiles: Vec<MountedProfile>,
    /// 用户自定义扫描配置目录列表（自动收录目录下 yaml/yml/json/jsonc）
    pub custom_profile_dirs: Vec<String>,
    /// 界面正文字体（空串表示系统默认）
    pub ui_font: String,
    /// 代码与 JSON 等宽字体（空串表示系统默认等宽字体）
    pub mono_font: String,
    /// 全局界面基础字号（11..=22，默认 14）
    pub font_size: u8,
    /// 代码与 JSON 预览字号（10..=24，默认 13）
    pub code_font_size: u8,
    /// 流光动效强度：rich（丰富） / subtle（轻量） / off（关闭）
    pub aurora_intensity: String,
    /// 是否已完成首次配置引导向导
    pub onboarding_completed: bool,
    /// GUI 图形界面保存的兜底配置（当 YAML 对应项为空或未绑定 YAML 时生效）
    pub gui_config: AppConfig,
}

impl Default for GuiStateFile {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            active_yaml_path: None,
            custom_yaml_paths: Vec::new(),
            mounted_profiles: Vec::new(),
            custom_profile_dirs: Vec::new(),
            ui_font: String::new(),
            mono_font: String::new(),
            font_size: default_font_size(),
            code_font_size: default_code_font_size(),
            aurora_intensity: default_aurora_intensity(),
            onboarding_completed: false,
            gui_config: AppConfig::default(),
        }
    }
}

/// 返回给前端的完整配置快照，含合并结果与逐字段来源标注
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSnapshot {
    /// 最终生效的配置（已按 YAML > GUI > 默认值 完成合并）
    pub effective_config: AppConfig,
    /// GUI 兜底配置
    pub gui_config: AppConfig,
    /// 当前主题设置（system / light / dark）
    pub theme: String,
    /// 界面正文字体
    pub ui_font: String,
    /// 等宽字体
    pub mono_font: String,
    /// 全局基础字号
    pub font_size: u8,
    /// 代码 / JSON 预览字号
    pub code_font_size: u8,
    /// 流光动效强度（rich / subtle / off）
    pub aurora_intensity: String,
    /// 是否已完成首次配置向导
    pub onboarding_completed: bool,
    /// 用户自定义配置目录列表
    pub custom_profile_dirs: Vec<String>,
    /// 当前绑定的 YAML 文件路径
    pub yaml_path: Option<String>,
    /// 当前 YAML 文件所在基准目录（用于相对路径预览与提示）
    pub base_dir: String,
    /// 当前 YAML 原始文本
    pub raw_yaml: String,
    /// 逐字段来源映射：值为 "yaml" / "gui" / "default"
    pub field_sources: BTreeMap<String, String>,
    /// 可供切换的配置档案列表
    pub profiles: Vec<ProfileSummary>,
    /// 若 YAML 解析出错则返回可读中文错误信息
    pub yaml_error: Option<String>,
}

/// 去掉 JSONC 文件里的 `//` 与 `/* */` 注释（字符串字面量内部不受影响）
pub fn strip_json_comments(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while i < chars.len() {
        let ch = chars[i];
        let next = chars.get(i + 1).copied();

        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                out.push(ch);
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if ch == '*' && next == Some('/') {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            i += 1;
            continue;
        }
        if ch == '/' && next == Some('/') {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if ch == '/' && next == Some('*') {
            in_block_comment = true;
            i += 2;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    out
}

/// 将 YAML / JSONC 文本解析为通用 `serde_yaml::Value`
pub fn parse_config_to_yaml_value(text: &str, file_hint: Option<&str>) -> Result<YamlValue, String> {
    let clean = text.trim_start_matches('\u{FEFF}');
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        return Ok(YamlValue::Mapping(Default::default()));
    }
    let ext = file_hint
        .and_then(|p| Path::new(p).extension())
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "json" || ext == "jsonc" || trimmed.starts_with('{') || trimmed.starts_with('[') {
        let stripped = strip_json_comments(clean);
        let json_val: serde_json::Value = serde_json::from_str(&stripped)
            .map_err(|e| format!("JSON/JSONC 解析失败：{}", e))?;
        let yaml_val = serde_yaml::to_value(json_val)
            .map_err(|e| format!("JSON 转 YAML 结构失败：{}", e))?;
        return Ok(yaml_val);
    }

    serde_yaml::from_str::<YamlValue>(clean).map_err(|e| format!("YAML 解析失败：{}", e))
}

/// 判断一个 `YamlValue` 是否属于“空值”（未写 / null / 空串 / 空数组 / 空映射）
///
/// 关键设计说明：
/// 布尔值 `false` 和数值 `0` 不算空值！只要 YAML 显式写了 `interactive: false` 或 `delayMs: 0`，
/// 依然会优先覆盖 GUI 配置。
pub fn is_yaml_empty(val: &YamlValue) -> bool {
    match val {
        YamlValue::Null => true,
        YamlValue::String(s) => s.trim().is_empty(),
        YamlValue::Sequence(seq) => seq.is_empty(),
        YamlValue::Mapping(map) => map.is_empty(),
        YamlValue::Bool(_) | YamlValue::Number(_) => false,
        YamlValue::Tagged(t) => is_yaml_empty(&t.value),
    }
}

/// 对字符串形式的 `auth` 做兼容性归一化（支持旧版 `auth: basic` 简写）
fn normalize_yaml_mapping(val: &mut YamlValue) {
    if let YamlValue::Mapping(map) = val {
        let auth_key = YamlValue::String("auth".to_string());
        if let Some(YamlValue::String(auth_type)) = map.get(&auth_key).cloned() {
            let mut inner = serde_yaml::Mapping::new();
            inner.insert(
                YamlValue::String("type".to_string()),
                YamlValue::String(auth_type),
            );
            map.insert(auth_key, YamlValue::Mapping(inner));
        }
    }
}

/// 递归合并通用 YAML 映射，返回合并后对象并记录每个字段的来源（"yaml" / "gui" / "default"）
///
/// 当 `yaml_val` 中存在且非空时取 `yaml_val` 并标注 `"yaml"`；
/// 否则回退到 `gui_val`（若 `gui_val` 也为空则由调用方再判定 `"default"`）。
fn merge_yaml_node(
    prefix: &str,
    yaml_node: Option<&YamlValue>,
    gui_node: &YamlValue,
    default_node: &YamlValue,
    sources: &mut BTreeMap<String, String>,
) -> YamlValue {
    // 若默认值是 Mapping 且非空（排除自由键值的 headers / variables 映射），则逐字段递归合并
    let is_structured_map = matches!(default_node, YamlValue::Mapping(m) if !m.is_empty())
        && prefix != "headers"
        && prefix != "variables";

    if is_structured_map {
        let def_map = default_node.as_mapping().unwrap();
        let gui_map = gui_node.as_mapping();
        let yml_map = yaml_node.and_then(|v| v.as_mapping());
        let mut merged = serde_yaml::Mapping::new();

        for (k, def_v) in def_map {
            let key_str = k.as_str().unwrap_or_default();
            let path = if prefix.is_empty() {
                key_str.to_string()
            } else {
                format!("{}.{}", prefix, key_str)
            };
            let g_v = gui_map.and_then(|m| m.get(k)).unwrap_or(def_v);
            let y_v = yml_map.and_then(|m| m.get(k));
            let res = merge_yaml_node(&path, y_v, g_v, def_v, sources);
            merged.insert(k.clone(), res);
        }
        return YamlValue::Mapping(merged);
    }

    if let Some(yv) = yaml_node {
        if !is_yaml_empty(yv) {
            sources.insert(prefix.to_string(), "yaml".to_string());
            return yv.clone();
        }
    }

    if !is_yaml_empty(gui_node) && gui_node != default_node {
        sources.insert(prefix.to_string(), "gui".to_string());
        return gui_node.clone();
    }

    // GUI 与默认值相同或为空时，标注对应来源
    if !is_yaml_empty(gui_node) {
        sources.insert(prefix.to_string(), "default".to_string());
        return gui_node.clone();
    }

    sources.insert(prefix.to_string(), "default".to_string());
    default_node.clone()
}

/// 将 `AppConfig`、可选 `YamlValue` 合并为最终生效配置并计算字段来源
pub fn merge_configs(
    yaml_opt: Option<&YamlValue>,
    gui_cfg: &AppConfig,
) -> Result<(AppConfig, BTreeMap<String, String>), String> {
    let default_cfg = AppConfig::default();
    let mut def_val = serde_yaml::to_value(&default_cfg)
        .map_err(|e| format!("序列化默认配置失败：{}", e))?;
    let mut gui_val = serde_yaml::to_value(gui_cfg)
        .map_err(|e| format!("序列化 GUI 配置失败：{}", e))?;

    let mut yaml_clean = yaml_opt.cloned();
    if let Some(ref mut yv) = yaml_clean {
        normalize_yaml_mapping(yv);
    }
    normalize_yaml_mapping(&mut def_val);
    normalize_yaml_mapping(&mut gui_val);

    let mut sources = BTreeMap::new();
    let merged_val = merge_yaml_node(
        "",
        yaml_clean.as_ref(),
        &gui_val,
        &def_val,
        &mut sources,
    );

    let mut cfg: AppConfig = serde_yaml::from_value(merged_val)
        .map_err(|e| format!("配置字段类型不匹配：{}", e))?;

    // 处理嵌套可选对象 testMark（在 Default 中为 None，需单独判断 YAML 是否显式配置）
    if let Some(yml_map) = yaml_clean.as_ref().and_then(|v| v.as_mapping()) {
        let tm_key = YamlValue::String("testMark".to_string());
        if let Some(tm_val) = yml_map.get(&tm_key) {
            if !is_yaml_empty(tm_val) {
                if let Ok(parsed_tm) = serde_yaml::from_value::<TestMarkConfig>(tm_val.clone()) {
                    cfg.test_mark = Some(parsed_tm);
                    sources.insert("testMark".to_string(), "yaml".to_string());
                }
            }
        }
    }
    if sources.get("testMark").map(|s| s.as_str()) != Some("yaml") {
        if gui_cfg.test_mark.is_some() {
            cfg.test_mark = gui_cfg.test_mark.clone();
            sources.insert("testMark".to_string(), "gui".to_string());
        }
    }

    // 兼容旧版顶层 `expect` 与 `strict`：若 YAML 显式写了顶层 expect/strict 而没写 assertions.*，同步到 assertions
    if let Some(yml_map) = yaml_clean.as_ref().and_then(|v| v.as_mapping()) {
        let has_assertions_expect = yml_map
            .get(&YamlValue::String("assertions".to_string()))
            .and_then(|a| a.as_mapping())
            .and_then(|m| m.get(&YamlValue::String("expect".to_string())))
            .map(|v| !is_yaml_empty(v))
            .unwrap_or(false);
        let has_top_expect = yml_map
            .get(&YamlValue::String("expect".to_string()))
            .map(|v| !is_yaml_empty(v))
            .unwrap_or(false);

        if has_top_expect && !has_assertions_expect {
            cfg.assertions.expect = cfg.expect.clone();
            sources.insert("assertions.expect".to_string(), "yaml".to_string());
        } else if has_assertions_expect {
            cfg.expect = cfg.assertions.expect.clone();
        }

        let has_assertions_strict = yml_map
            .get(&YamlValue::String("assertions".to_string()))
            .and_then(|a| a.as_mapping())
            .and_then(|m| m.get(&YamlValue::String("strict".to_string())))
            .map(|v| !is_yaml_empty(v))
            .unwrap_or(false);
        let has_top_strict = yml_map
            .get(&YamlValue::String("strict".to_string()))
            .map(|v| !is_yaml_empty(v))
            .unwrap_or(false);

        if has_top_strict && !has_assertions_strict {
            cfg.assertions.strict = cfg.strict;
            sources.insert("assertions.strict".to_string(), "yaml".to_string());
        } else if has_assertions_strict {
            cfg.strict = cfg.assertions.strict;
        }
    }

    // 智能鉴权类型补全：仅填用户名密码时自动按 basic 处理；仅填 token 时自动按 bearer 处理
    if cfg.auth.auth_type.trim().is_empty() || cfg.auth.auth_type.eq_ignore_ascii_case("none") {
        if !cfg.auth.username.trim().is_empty() || !cfg.auth.password.trim().is_empty() {
            cfg.auth.auth_type = "basic".to_string();
        } else if !cfg.auth.token.trim().is_empty() {
            cfg.auth.auth_type = "bearer".to_string();
        }
    }

    // 数值安全下限清洗
    cfg.concurrency = cfg.concurrency.max(1);
    cfg.timeout_ms = cfg.timeout_ms.max(500);
    if cfg.assertions.expect.is_empty() {
        cfg.assertions.expect = vec![200];
    }

    Ok((cfg, sources))
}

/// 环境变量与内置动态占位符插值
///
/// 支持：
/// - `{{$timestamp}}`：当前秒级 Unix 时间戳
/// - `{{$timestampMs}}`：当前毫秒级 Unix 时间戳
/// - `{{$isoTime}}`：当前 UTC ISO-8601 时间字符串
/// - `{{$uuid}}`：每次调用生成随机 UUID v4
/// - `{{VAR_NAME}}`：优先查找 `variables` 映射，再回退系统环境变量 `std::env::var`
pub fn interpolate_str(input: &str, variables: &BTreeMap<String, String>) -> String {
    if !input.contains("{{") {
        return input.to_string();
    }
    let re = Regex::new(r"\{\{\s*([^{}\s]+)\s*\}\}").unwrap();
    re.replace_all(input, |caps: &regex::Captures| {
        let key = &caps[1];
        match key {
            "$timestamp" => Utc::now().timestamp().to_string(),
            "$timestampMs" => Utc::now().timestamp_millis().to_string(),
            "$isoTime" => Utc::now().to_rfc3339(),
            "$uuid" => Uuid::new_v4().to_string(),
            _ => {
                if let Some(v) = variables.get(key) {
                    v.clone()
                } else if let Ok(env_val) = std::env::var(key) {
                    env_val
                } else {
                    caps[0].to_string()
                }
            }
        }
    })
    .to_string()
}

/// 将可能是相对路径的路径按基准目录解析为绝对路径
pub fn resolve_path(raw: &str, base_dir: &Path) -> PathBuf {
    let p = Path::new(raw.trim());
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base_dir.join(p)
    }
}

/// 自动探测当前目录或程序同目录下的默认 YAML 配置文件
pub fn detect_default_yaml_path() -> Option<PathBuf> {
    let file_names = [
        "hook-replay.config.yaml",
        "hook-replay.config.yml",
        "notify-test.config.yaml",
        "notify-test.config.yml",
    ];
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        for name in &file_names {
            candidates.push(cwd.join(name));
            candidates.push(cwd.join("configs").join(name));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in &file_names {
                candidates.push(dir.join(name));
                candidates.push(dir.join("configs").join(name));
            }
        }
    }

    candidates.into_iter().find(|p| p.is_file())
}

/// 规范化路径展示字符串（去掉 Windows `\\?\` 长路径前缀并统一路径分隔符）
pub fn normalize_path_string(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let p = Path::new(trimmed);
    if p.exists() {
        if let Ok(canon) = fs::canonicalize(p) {
            return canon
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_string();
        }
    }
    trimmed.trim_start_matches(r"\\?\").to_string()
}

/// 生成用于路径去重比较的归一化键值（Windows 下忽略大小写与正反斜杠差异）
pub fn normalize_path_key(raw: &str) -> String {
    let norm = normalize_path_string(raw);
    #[cfg(target_os = "windows")]
    {
        norm.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        norm
    }
}

/// 判断文件扩展名是否属于支持的配置文件格式
fn is_supported_config_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "yaml" | "yml" | "json" | "jsonc")
}

/// 扫描已挂载档案、自定义配置目录与默认 `profiles/*.yaml` 档案列表
pub fn discover_profiles(
    active_yaml: Option<&str>,
    mounted: &[MountedProfile],
    custom_dirs: &[String],
    custom_paths: &[String],
) -> Vec<ProfileSummary> {
    fn add_profile(
        seen_keys: &mut std::collections::HashSet<String>,
        result: &mut Vec<ProfileSummary>,
        p: &Path,
        preferred_name: Option<String>,
        include_missing: bool,
        source_kind: &str,
    ) {
        let raw_str = p.to_string_lossy().trim().to_string();
        if raw_str.is_empty() {
            return;
        }
        let exists = p.is_file();
        if !exists && !include_missing {
            return;
        }
        let norm_path = normalize_path_string(&raw_str);
        let key = normalize_path_key(&norm_path);
        if !seen_keys.insert(key) {
            return;
        }
        let fallback_name = Path::new(&norm_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("未命名档案")
            .to_string();
        let name = preferred_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_name);
        result.push(ProfileSummary {
            name,
            path: norm_path,
            exists,
            source_kind: source_kind.to_string(),
        });
    }

    fn scan_dir(
        seen_keys: &mut std::collections::HashSet<String>,
        result: &mut Vec<ProfileSummary>,
        dir: &Path,
        kind: &str,
    ) {
        if !dir.is_dir() {
            return;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            let mut candidates: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && is_supported_config_ext(p))
                .collect();
            candidates.sort();
            for fp in candidates {
                add_profile(seen_keys, result, &fp, None, false, kind);
            }
        }
    }

    let mut seen_keys = std::collections::HashSet::<String>::new();
    let mut result = Vec::<ProfileSummary>::new();

    // 1. 优先展示用户手动挂载的配置文件（包含可编辑别名与失效状态标注）
    for mp in mounted {
        let alias = if mp.alias.trim().is_empty() {
            None
        } else {
            Some(mp.alias.trim().to_string())
        };
        add_profile(
            &mut seen_keys,
            &mut result,
            Path::new(&mp.path),
            alias,
            true,
            "mounted",
        );
    }

    // 2. 兼容旧版 custom_yaml_paths 中尚未迁移的路径
    for cp in custom_paths {
        add_profile(
            &mut seen_keys,
            &mut result,
            Path::new(cp),
            None,
            false,
            "mounted",
        );
    }

    // 3. 扫描用户自定义挂载的配置目录
    for dir_raw in custom_dirs {
        let trimmed = dir_raw.trim();
        if !trimmed.is_empty() {
            scan_dir(&mut seen_keys, &mut result, Path::new(trimmed), "dir");
        }
    }

    // 4. 当前激活的 YAML 及其同级 profiles/ 子目录
    if let Some(active) = active_yaml {
        let p = Path::new(active);
        let kind = if p.is_file() { "auto" } else { "mounted" };
        add_profile(&mut seen_keys, &mut result, p, None, true, kind);
        if let Some(dir) = p.parent() {
            scan_dir(&mut seen_keys, &mut result, &dir.join("profiles"), "dir");
        }
    }

    // 5. 自动探测默认配置档案及其同级 profiles/ 子目录
    if let Some(def) = detect_default_yaml_path() {
        add_profile(
            &mut seen_keys,
            &mut result,
            &def,
            Some("默认配置 (notify-test.config)".to_string()),
            false,
            "auto",
        );
        if let Some(dir) = def.parent() {
            scan_dir(&mut seen_keys, &mut result, &dir.join("profiles"), "dir");
        }
    }

    result
}

/// 返回内置的 3 套 YAML 配置模板（编译内嵌，便携单文件模式也可随时生成）
pub fn builtin_config_templates() -> Vec<ConfigTemplateInfo> {
    vec![
        ConfigTemplateInfo {
            id: "n8n-webhook".to_string(),
            name: "n8n Webhook 完整模板".to_string(),
            description: "含 /webhook/ 与 /webhook-test/ 预设入口、Basic 认证、幂等跳过断言及历史报告配置".to_string(),
            suggested_file_name: "n8n-webhook.config.yaml".to_string(),
            content: include_str!("../../templates/n8n-webhook.template.yaml").to_string(),
        },
        ConfigTemplateInfo {
            id: "generic-webhook".to_string(),
            name: "通用 Webhook / REST API 模板".to_string(),
            description: "支持 Bearer/Header 鉴权、HMAC-SHA256 签名、V2 testMark 注入、自定义摘要路径与变量插值".to_string(),
            suggested_file_name: "generic-webhook.config.yaml".to_string(),
            content: include_str!("../../templates/generic-webhook.template.yaml").to_string(),
        },
        ConfigTemplateInfo {
            id: "minimal".to_string(),
            name: "精简快速配置模板".to_string(),
            description: "仅保留目标地址、请求方法、认证、数据源与交互开关，其余项自动回退 GUI 默认值".to_string(),
            suggested_file_name: "hook-replay.minimal.yaml".to_string(),
            content: include_str!("../../templates/minimal.template.yaml").to_string(),
        },
    ]
}

/// 根据内置模板 ID 在磁盘生成 YAML 配置文件
pub fn create_template_file(template_id: &str, target_path: &str) -> Result<PathBuf, String> {
    let trimmed = target_path.trim();
    if trimmed.is_empty() {
        return Err("目标配置文件路径不能为空".to_string());
    }
    let templates = builtin_config_templates();
    let tpl = templates
        .into_iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| format!("未找到指定配置模板：{}", template_id))?;

    let path = PathBuf::from(trimmed);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目标目录失败：{}", e))?;
        }
    }
    fs::write(&path, tpl.content).map_err(|e| format!("写入模板配置文件失败：{}", e))?;
    Ok(PathBuf::from(normalize_path_string(&path.to_string_lossy())))
}

/// 将旧版 `custom_yaml_paths` 迁移合并至 `mounted_profiles`，并清洗字号/动效等参数边界
pub fn migrate_gui_state(state: &mut GuiStateFile) {
    let mut seen_keys: std::collections::HashSet<String> = state
        .mounted_profiles
        .iter()
        .map(|m| normalize_path_key(&m.path))
        .collect();

    for mp in &mut state.mounted_profiles {
        mp.path = normalize_path_string(&mp.path);
    }

    for legacy in &state.custom_yaml_paths {
        let norm = normalize_path_string(legacy);
        if norm.is_empty() {
            continue;
        }
        let key = normalize_path_key(&norm);
        if seen_keys.insert(key) {
            state.mounted_profiles.push(MountedProfile {
                path: norm,
                alias: String::new(),
            });
        }
    }

    if state.font_size == 0 {
        state.font_size = default_font_size();
    } else {
        state.font_size = state.font_size.clamp(11, 22);
    }

    if state.code_font_size == 0 {
        state.code_font_size = default_code_font_size();
    } else {
        state.code_font_size = state.code_font_size.clamp(10, 24);
    }

    if !matches!(state.aurora_intensity.as_str(), "rich" | "subtle" | "off") {
        state.aurora_intensity = default_aurora_intensity();
    }
}

/// 将配置文件路径挂载（或更新别名）到状态列表中，返回规范化路径
pub fn upsert_mounted_profile(
    state: &mut GuiStateFile,
    raw_path: &str,
    alias: Option<&str>,
) -> String {
    let norm = normalize_path_string(raw_path);
    let key = normalize_path_key(&norm);
    if let Some(existing) = state
        .mounted_profiles
        .iter_mut()
        .find(|m| normalize_path_key(&m.path) == key)
    {
        existing.path = norm.clone();
        if let Some(a) = alias {
            existing.alias = a.trim().to_string();
        }
    } else {
        state.mounted_profiles.push(MountedProfile {
            path: norm.clone(),
            alias: alias.map(|s| s.trim().to_string()).unwrap_or_default(),
        });
    }
    if !state
        .custom_yaml_paths
        .iter()
        .any(|p| normalize_path_key(p) == key)
    {
        state.custom_yaml_paths.push(norm.clone());
    }
    norm
}

/// 取消挂载指定配置文件路径（若当前正激活该文件，同步切回纯 GUI 模式）
pub fn remove_mounted_profile(state: &mut GuiStateFile, raw_path: &str) {
    let key = normalize_path_key(raw_path);
    state
        .mounted_profiles
        .retain(|m| normalize_path_key(&m.path) != key);
    state
        .custom_yaml_paths
        .retain(|p| normalize_path_key(p) != key);
    if let Some(ref active) = state.active_yaml_path {
        if normalize_path_key(active) == key {
            state.active_yaml_path = None;
        }
    }
}

/// 计算配置生效时的基准目录（YAML 文件所在目录 > 探测到的默认配置目录 > 当前工作目录）
pub fn resolve_base_dir(yaml_path: Option<&str>) -> PathBuf {
    if let Some(yp) = yaml_path {
        if let Some(parent) = Path::new(yp).parent() {
            if parent.is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    if let Some(def) = detect_default_yaml_path() {
        if let Some(parent) = def.parent() {
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// 读取 GUI 状态文件（若尚不存在则自动探测并绑定默认 YAML 档案，同时执行平滑迁移）
pub fn load_gui_state(state_file: &Path) -> GuiStateFile {
    if state_file.is_file() {
        if let Ok(raw) = fs::read_to_string(state_file) {
            if let Ok(mut parsed) = serde_json::from_str::<GuiStateFile>(&raw) {
                migrate_gui_state(&mut parsed);
                return parsed;
            }
        }
    }
    let mut state = GuiStateFile::default();
    if let Some(def) = detect_default_yaml_path() {
        let clean = normalize_path_string(&def.to_string_lossy());
        upsert_mounted_profile(&mut state, &clean, Some("默认配置 (notify-test.config)"));
        state.active_yaml_path = Some(clean);
    }
    migrate_gui_state(&mut state);
    state
}

/// 持久化 GUI 状态文件
pub fn save_gui_state(state_file: &Path, state: &GuiStateFile) -> Result<(), String> {
    if let Some(parent) = state_file.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{}", e))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|e| format!("序列化 GUI 状态失败：{}", e))?;
    fs::write(state_file, format!("{}\n", text))
        .map_err(|e| format!("写入 GUI 状态失败：{}", e))
}

/// 构建返回给前端的完整配置快照
pub fn build_config_snapshot(state: &GuiStateFile) -> ConfigSnapshot {
    let mut raw_yaml = String::new();
    let mut yaml_error = None;
    let mut yaml_val_opt = None;

    if let Some(ref yp) = state.active_yaml_path {
        let p = Path::new(yp);
        if p.is_file() {
            match fs::read_to_string(p) {
                Ok(content) => {
                    raw_yaml = content.clone();
                    match parse_config_to_yaml_value(&content, Some(yp)) {
                        Ok(val) => yaml_val_opt = Some(val),
                        Err(err) => yaml_error = Some(err),
                    }
                }
                Err(err) => yaml_error = Some(format!("读取 YAML 文件失败：{}", err)),
            }
        } else {
            yaml_error = Some(format!("指定的配置文件不存在：{}", yp));
        }
    }

    let (effective_config, field_sources) = match merge_configs(yaml_val_opt.as_ref(), &state.gui_config) {
        Ok(res) => res,
        Err(err) => {
            yaml_error = Some(err);
            merge_configs(None, &state.gui_config).unwrap_or_else(|_| (AppConfig::default(), BTreeMap::new()))
        }
    };

    let base_dir = resolve_base_dir(state.active_yaml_path.as_deref())
        .to_string_lossy()
        .to_string();
    let profiles = discover_profiles(
        state.active_yaml_path.as_deref(),
        &state.mounted_profiles,
        &state.custom_profile_dirs,
        &state.custom_yaml_paths,
    );

    ConfigSnapshot {
        effective_config,
        gui_config: state.gui_config.clone(),
        theme: state.theme.clone(),
        ui_font: state.ui_font.clone(),
        mono_font: state.mono_font.clone(),
        font_size: if state.font_size == 0 {
            default_font_size()
        } else {
            state.font_size.clamp(11, 22)
        },
        code_font_size: if state.code_font_size == 0 {
            default_code_font_size()
        } else {
            state.code_font_size.clamp(10, 24)
        },
        aurora_intensity: if state.aurora_intensity.is_empty() {
            "rich".to_string()
        } else {
            state.aurora_intensity.clone()
        },
        onboarding_completed: state.onboarding_completed,
        custom_profile_dirs: state.custom_profile_dirs.clone(),
        yaml_path: state.active_yaml_path.clone(),
        base_dir,
        raw_yaml,
        field_sources,
        profiles,
        yaml_error,
    }
}

/// 一键切换 n8n URL 的正式路径 (`/webhook/`) 与测试路径 (`/webhook-test/`)
pub fn toggle_n8n_webhook_url(url: &str) -> String {
    if url.contains("/webhook-test/") {
        url.replacen("/webhook-test/", "/webhook/", 1)
    } else if url.contains("/webhook/") {
        url.replacen("/webhook/", "/webhook-test/", 1)
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_precedence_with_empty_fallback_and_explicit_false_zero() {
        let yaml_text = r#"
url: "https://yaml.example.com/webhook/demo"
method: ""
interactive: false
delayMs: 0
auth:
  type: basic
  username: yaml_user
  password: ""
types: []
"#;
        let yaml_val = parse_config_to_yaml_value(yaml_text, Some("test.yaml")).unwrap();

        let mut gui_cfg = AppConfig::default();
        gui_cfg.url = "https://gui.example.com".to_string();
        gui_cfg.method = "PUT".to_string();
        gui_cfg.interactive = true;
        gui_cfg.delay_ms = 500;
        gui_cfg.auth.username = "gui_user".to_string();
        gui_cfg.auth.password = "gui_secret".to_string();
        gui_cfg.types = vec!["transfer.complete".to_string()];

        let (merged, sources) = merge_configs(Some(&yaml_val), &gui_cfg).unwrap();

        // YAML 非空项优先于 GUI
        assert_eq!(merged.url, "https://yaml.example.com/webhook/demo");
        assert_eq!(sources.get("url").map(String::as_str), Some("yaml"));
        assert_eq!(merged.auth.username, "yaml_user");
        assert_eq!(sources.get("auth.username").map(String::as_str), Some("yaml"));

        // YAML 显式写了 false 和 0，必须优先于 GUI
        assert!(!merged.interactive);
        assert_eq!(sources.get("interactive").map(String::as_str), Some("yaml"));
        assert_eq!(merged.delay_ms, 0);
        assert_eq!(sources.get("delayMs").map(String::as_str), Some("yaml"));

        // YAML 写为空字符串/空数组时，自动回退使用 GUI 设置
        assert_eq!(merged.method, "PUT");
        assert_eq!(sources.get("method").map(String::as_str), Some("gui"));
        assert_eq!(merged.auth.password, "gui_secret");
        assert_eq!(sources.get("auth.password").map(String::as_str), Some("gui"));
        assert_eq!(merged.types, vec!["transfer.complete".to_string()]);
        assert_eq!(sources.get("types").map(String::as_str), Some("gui"));
    }

    #[test]
    fn test_toggle_n8n_url_and_interpolation() {
        let prod = "https://n8n.example.com:8443/webhook/demo";
        let test = toggle_n8n_webhook_url(prod);
        assert_eq!(test, "https://n8n.example.com:8443/webhook-test/demo");
        assert_eq!(toggle_n8n_webhook_url(&test), prod);

        let mut vars = BTreeMap::new();
        vars.insert("HOST".to_string(), "127.0.0.1:5678".to_string());
        let out = interpolate_str("http://{{HOST}}/webhook/{{$uuid}}", &vars);
        assert!(out.starts_with("http://127.0.0.1:5678/webhook/"));
        assert!(!out.contains("{{$uuid}}"));
    }

    #[test]
    fn test_templates_mounting_and_legacy_migration() {
        let templates = builtin_config_templates();
        assert_eq!(templates.len(), 3);
        for tpl in &templates {
            let parsed = parse_config_to_yaml_value(&tpl.content, Some(&tpl.suggested_file_name))
                .expect("内置模板必须能正确解析为 YAML");
            let (_cfg, _) = merge_configs(Some(&parsed), &AppConfig::default())
                .expect("内置模板必须能顺利合并为 AppConfig");
        }

        let temp_dir = std::env::temp_dir().join(format!("hookreplay_cfg_test_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let file_a = create_template_file(
            "minimal",
            &temp_dir.join("alpha.yaml").to_string_lossy(),
        )
        .unwrap();
        let file_b = create_template_file(
            "generic-webhook",
            &temp_dir.join("sub").join("beta.yaml").to_string_lossy(),
        )
        .unwrap();

        // 验证旧版 gui-config.json 反序列化与平滑迁移
        let legacy_json = serde_json::json!({
            "theme": "dark",
            "activeYamlPath": file_a.to_string_lossy(),
            "customYamlPaths": [file_a.to_string_lossy(), file_b.to_string_lossy()]
        });
        let legacy_file = temp_dir.join("gui-config.json");
        fs::write(&legacy_file, serde_json::to_string(&legacy_json).unwrap()).unwrap();

        let mut loaded = load_gui_state(&legacy_file);
        assert_eq!(loaded.theme, "dark");
        assert_eq!(loaded.font_size, 14);
        assert_eq!(loaded.code_font_size, 13);
        assert_eq!(loaded.aurora_intensity, "rich");
        assert!(!loaded.onboarding_completed);
        assert_eq!(loaded.mounted_profiles.len(), 2);

        // 挂载带自定义别名且对同路径去重
        let upper_a = file_a.to_string_lossy().to_uppercase();
        upsert_mounted_profile(&mut loaded, &upper_a, Some("生产配置别名"));
        assert_eq!(loaded.mounted_profiles.len(), 2);

        // 自定义目录扫描 + 移除测试
        loaded.custom_profile_dirs.push(temp_dir.join("sub").to_string_lossy().to_string());
        let profiles = discover_profiles(
            loaded.active_yaml_path.as_deref(),
            &loaded.mounted_profiles,
            &loaded.custom_profile_dirs,
            &loaded.custom_yaml_paths,
        );
        assert!(profiles.iter().any(|p| p.name == "生产配置别名" && p.exists));

        remove_mounted_profile(&mut loaded, &file_a.to_string_lossy());
        assert!(loaded.active_yaml_path.is_none());
        assert_eq!(loaded.mounted_profiles.len(), 1);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
