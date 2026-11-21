//! 摘要提取与响应断言引擎（Matcher & Assertions）
//!
//! 核心职责：
//! 1. 根据可配置的 `LabelRulesConfig` 从任意 JSON Payload 中提炼业务事件类型与可读标题；
//! 2. 兼容常见自动化工作流、RSS / Diun / Grafana / Wallos / Dockhand 等多种通知结构识别；
//! 3. 按 `AssertionsConfig`（成功状态码、软跳过状态码、响应体 JSON 规则）对响应分类并生成中文徽章。

use crate::config::{AppConfig, AssertionsConfig, LabelRulesConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 提取出的条目展示摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedSummary {
    pub event_type: String,
    pub title: String,
    pub label: String,
}

/// 响应分类评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationOutcome {
    /// 分类结果："ok" | "deduped" | "unmatched" | "failed"
    pub outcome: String,
    /// 前端展示的语义徽章文案（例如 "200 成功" / "200 幂等跳过" / "400 未匹配" / "500 失败"）
    pub badge: String,
    /// 是否计入成功数（包含 ok 与 deduped）
    pub ok: bool,
    /// 是否属于软跳过/未匹配（例如 non-strict 模式下的 400）
    pub unmatched: bool,
    /// 命中规则的说明原因
    pub reason: String,
}

/// 按点路径与数组下标语法读取 JSON 字段（例如 `data.mediainfo.title_year`、`entries[0].title`）
pub fn get_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for raw_part in path.split('.') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }
        // 支持形如 entries[0] 的数组下标访问
        if let Some(open_idx) = part.find('[') {
            let key = &part[..open_idx];
            if !key.is_empty() {
                current = current.get(key)?;
            }
            let rest = &part[open_idx..];
            for bracket in rest.split('[').filter(|s| !s.is_empty()) {
                let idx_str = bracket.trim_end_matches(']');
                let idx: usize = idx_str.parse().ok()?;
                current = current.get(idx)?;
            }
        } else {
            current = current.get(part)?;
        }
    }
    Some(current)
}

/// 将 JSON 基础类型转为可读短字符串
fn value_as_short_string(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// 按配置提取条目摘要（支持自动结构推断与通用 JSON 点路径）
pub fn extract_summary(payload: &Value, rules: &LabelRulesConfig) -> ExtractedSummary {
    let mut event_type = String::new();
    for path in &rules.type_paths {
        if let Some(val) = get_json_path(payload, path).and_then(value_as_short_string) {
            event_type = val;
            break;
        }
    }

    // 兼容 n8n-renotify 原有来源识别规则（在 preset 为 auto 或 n8n-renotify 时启用）
    if event_type.is_empty() && rules.preset != "generic" {
        if payload.get("feed").is_some() {
            event_type = "rss".to_string();
        } else if payload.get("diun_version").is_some() {
            event_type = "diun".to_string();
        } else if payload.get("days_until").is_some() {
            event_type = "wallos".to_string();
        } else if payload.get("alerts").is_some() {
            event_type = "grafana".to_string();
        } else if payload.get("zerobyte_title").is_some() {
            event_type = "zerobyte".to_string();
        } else if payload.get("title").is_some() && payload.get("message").is_some() {
            event_type = "dockhand".to_string();
        }
    }
    if event_type.is_empty() {
        event_type = "unknown".to_string();
    }

    let mut title = String::new();
    for path in &rules.title_paths {
        if let Some(val) = get_json_path(payload, path).and_then(value_as_short_string) {
            title = val;
            break;
        }
    }
    // 兼容 Grafana alerts[0].labels.alertname 与 RSS feed.title
    if title.is_empty() && rules.preset != "generic" {
        for fallback in [
            "alerts[0].labels.alertname",
            "feed.title",
            "zerobyte_title",
            "subscriptionsinfo.name",
            "subscriptioninfo.name",
        ] {
            if let Some(val) = get_json_path(payload, fallback).and_then(value_as_short_string) {
                title = val;
                break;
            }
        }
    }

    let label = if title.is_empty() {
        event_type.clone()
    } else {
        format!("{} | {}", event_type, title)
    };

    ExtractedSummary {
        event_type,
        title,
        label,
    }
}

/// 根据状态码、响应体与配置规则评估响应分类
pub fn evaluate_response(
    status: u16,
    response_body: &str,
    assertions: &AssertionsConfig,
) -> EvaluationOutcome {
    let is_expected = assertions.expect.contains(&status);
    let is_soft_fail = assertions.soft_fail_status.contains(&status);

    // 尝试解析 JSON 响应体用于体断言（如识别 deduped: true）
    if let Ok(json_body) = serde_json::from_str::<Value>(response_body) {
        for rule in &assertions.body_rules {
            if rule.path.trim().is_empty() {
                continue;
            }
            if let Some(actual) = get_json_path(&json_body, &rule.path) {
                if actual == &rule.equals {
                    let tone = rule.tone.to_ascii_lowercase();
                    if tone == "error" {
                        return EvaluationOutcome {
                            outcome: "failed".to_string(),
                            badge: format!(
                                "{} {}",
                                status,
                                if rule.badge.is_empty() { "断言失败" } else { &rule.badge }
                            ),
                            ok: false,
                            unmatched: false,
                            reason: format!("命中响应失败断言：{} == {}", rule.path, rule.equals),
                        };
                    }
                    if tone == "warning" {
                        let treat_fail = assertions.strict;
                        return EvaluationOutcome {
                            outcome: if treat_fail { "failed" } else { "unmatched" }.to_string(),
                            badge: format!(
                                "{} {}",
                                status,
                                if rule.badge.is_empty() { "未匹配" } else { &rule.badge }
                            ),
                            ok: false,
                            unmatched: !treat_fail,
                            reason: format!("命中响应警告断言：{} == {}", rule.path, rule.equals),
                        };
                    }
                    // info / success 语义（例如 200 幂等跳过）
                    let is_deduped = rule.path == "deduped" || rule.badge.contains("跳过");
                    return EvaluationOutcome {
                        outcome: if is_deduped { "deduped" } else { "ok" }.to_string(),
                        badge: format!(
                            "{} {}",
                            status,
                            if rule.badge.is_empty() { "成功" } else { &rule.badge }
                        ),
                        ok: true,
                        unmatched: false,
                        reason: format!("命中响应规则：{} == {}", rule.path, rule.equals),
                    };
                }
            }
        }
    }

    if is_expected {
        return EvaluationOutcome {
            outcome: "ok".to_string(),
            badge: format!("{} 成功", status),
            ok: true,
            unmatched: false,
            reason: String::new(),
        };
    }

    if is_soft_fail {
        if assertions.strict {
            return EvaluationOutcome {
                outcome: "failed".to_string(),
                badge: format!("{} 失败(严格)", status),
                ok: false,
                unmatched: false,
                reason: format!("严格模式下 HTTP {} 视为失败", status),
            };
        }
        return EvaluationOutcome {
            outcome: "unmatched".to_string(),
            badge: format!("{} 未匹配", status),
            ok: false,
            unmatched: true,
            reason: format!("HTTP {}（未匹配/软跳过）", status),
        };
    }

    EvaluationOutcome {
        outcome: "failed".to_string(),
        badge: format!("{} 失败", status),
        ok: false,
        unmatched: false,
        reason: format!("非预期状态码 HTTP {}", status),
    }
}

/// 辅助函数：使用有效 `AppConfig` 快速评估响应
pub fn evaluate_with_config(status: u16, response_body: &str, cfg: &AppConfig) -> EvaluationOutcome {
    evaluate_response(status, response_body, &cfg.assertions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_summary_compat_and_generic() {
        let rules = LabelRulesConfig::default();
        let mp = json!({
            "type": "transfer.complete",
            "data": { "mediainfo": { "title_year": "Sample Media (2024)" } }
        });
        let s1 = extract_summary(&mp, &rules);
        assert_eq!(s1.event_type, "transfer.complete");
        assert_eq!(s1.title, "Sample Media (2024)");
        assert_eq!(s1.label, "transfer.complete | Sample Media (2024)");

        let rss = json!({
            "event_type": "new_entries",
            "feed": { "title": "Demo Feed" },
            "entries": [{ "title": "Sample Feed Entry" }]
        });
        let s2 = extract_summary(&rss, &rules);
        assert_eq!(s2.event_type, "new_entries");
        assert_eq!(s2.title, "Sample Feed Entry");
    }

    #[test]
    fn test_evaluate_response_deduped_and_unmatched() {
        let cfg = AssertionsConfig::default();
        let dedup = evaluate_response(200, r#"{"title":"重复通知已忽略","deduped":true}"#, &cfg);
        assert_eq!(dedup.outcome, "deduped");
        assert!(dedup.ok);
        assert_eq!(dedup.badge, "200 幂等跳过");

        let ok = evaluate_response(200, r#"{"title":"通知已接收","deduped":false}"#, &cfg);
        assert_eq!(ok.outcome, "ok");
        assert!(ok.ok);

        let soft = evaluate_response(400, r#"{"title":"无匹配通知类型"}"#, &cfg);
        assert_eq!(soft.outcome, "unmatched");
        assert!(soft.unmatched);
        assert!(!soft.ok);
    }
}
