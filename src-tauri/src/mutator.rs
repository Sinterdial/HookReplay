//! 请求构造、测试标记注入、身份认证、HMAC 签名与敏感信息脱敏模块（Mutator）
//!
//! 核心职责：
//! 1. 根据 `AppConfig` 对原始 Payload 与目标 URL 执行 `{{VAR}}` 及动态占位符插值；
//! 2. 支持多种方式注入测试标记（`testMark`: Body JSON 点路径 / Header / Query / None，兼容 `markTest`）；
//! 3. 支持 5 种鉴权模式（none / basic / bearer / header / query）与自定义请求头合并；
//! 4. 支持可选 HMAC-SHA256 / HMAC-SHA1 请求体签名（适配 GitHub / Gitea / 自建验签网关）；
//! 5. 输出一份同时包含「真实请求参数」与「前端展示用脱敏视图」的 `PreparedRequest`。

use crate::config::{interpolate_str, AppConfig, TestMarkConfig};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use serde_json::{Map, Value};
use sha1::Sha1;
use sha2::Sha256;
use std::collections::BTreeMap;

/// 准备就绪的 HTTP 请求（含真实发送数据与前端安全展示用的脱敏副本）
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    /// 大写 HTTP 方法名（POST / PUT / PATCH / GET 等）
    pub method: String,
    /// 真实发送的目标 URL（含 Query 认证或测试参数）
    pub url: String,
    /// 脱敏后的目标 URL（隐藏 Query 密钥，供 UI 与报告展示）
    pub masked_url: String,
    /// 真实发送的请求头映射
    pub headers: BTreeMap<String, String>,
    /// 脱敏后的请求头映射（隐藏 Authorization / Token / Secret 等敏感值）
    pub masked_headers: BTreeMap<String, String>,
    /// 最终序列化后的请求体文本（已做变量插值与测试标记注入）
    pub body: String,
}

/// 决策最终生效的测试标记配置（`testMark` 优先；未配置时兼容旧版 `markTest` 开关）
pub fn effective_test_mark(cfg: &AppConfig) -> TestMarkConfig {
    if let Some(ref tm) = cfg.test_mark {
        return tm.clone();
    }
    if cfg.mark_test {
        TestMarkConfig {
            mode: "body".to_string(),
            field: "__test".to_string(),
            value: Value::Bool(true),
        }
    } else {
        TestMarkConfig {
            mode: "none".to_string(),
            field: String::new(),
            value: Value::Null,
        }
    }
}

/// 以大小写不敏感的方式设置 HTTP 请求头（避免出现重复的大小写异构键）
fn set_header_ci(map: &mut BTreeMap<String, String>, key: &str, val: &str) {
    let existing: Vec<String> = map
        .keys()
        .filter(|k| k.eq_ignore_ascii_case(key))
        .cloned()
        .collect();
    for old_k in existing {
        map.remove(&old_k);
    }
    map.insert(key.to_string(), val.to_string());
}

/// 对 URL 查询参数的键值做 RFC 3986 基础百分号编码
fn percent_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let ch = *byte;
        let is_unreserved = ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(ch as char);
        } else {
            out.push_str(&format!("%{:02X}", ch));
        }
    }
    out
}

/// 安全向 URL 追加 Query 参数（兼容已有 `?` 与 `#fragment`）
fn append_query_param(url: &str, key: &str, value: &str, encode_value: bool) -> String {
    let (base_and_query, fragment) = match url.find('#') {
        Some(idx) => (&url[..idx], Some(&url[idx..])),
        None => (url, None),
    };
    let sep = if base_and_query.contains('?') {
        if base_and_query.ends_with('?') || base_and_query.ends_with('&') {
            ""
        } else {
            "&"
        }
    } else {
        "?"
    };
    let k_enc = percent_encode_component(key);
    let v_enc = if encode_value {
        percent_encode_component(value)
    } else {
        value.to_string()
    };

    let mut result = format!("{}{}{}={}", base_and_query, sep, k_enc, v_enc);
    if let Some(frag) = fragment {
        result.push_str(frag);
    }
    result
}

/// 按点分路径向 JSON Object 写入字段（中间对象不存在时自动创建）
fn set_json_dot_path(root: &mut Value, dot_path: &str, val: Value) {
    let parts: Vec<&str> = dot_path
        .split('.')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        return;
    }

    let mut current = root;
    for (idx, part) in parts.iter().enumerate() {
        let is_last = idx + 1 == parts.len();
        if !current.is_object() {
            return;
        }
        let obj = current.as_object_mut().unwrap();
        if is_last {
            obj.insert((*part).to_string(), val);
            return;
        }
        if !obj.contains_key(*part) || !obj[*part].is_object() {
            obj.insert((*part).to_string(), Value::Object(Map::new()));
        }
        current = obj.get_mut(*part).unwrap();
    }
}

/// 将 `serde_json::Value` 转为适合放入 Header 或 Query 的标量字符串
fn json_value_to_scalar_str(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// 对敏感值生成遮罩字符串（保留少量前缀方便排查是否传值）
fn mask_secret_value(raw: &str) -> String {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() <= 6 {
        "******".to_string()
    } else {
        let prefix: String = chars.iter().take(4).collect();
        format!("{}******", prefix)
    }
}

/// 判断请求头名称是否属于常见敏感鉴权头
fn is_sensitive_header_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization"
            | "proxy-authorization"
            | "token"
            | "api-key"
            | "x-api-key"
            | "cookie"
            | "set-cookie"
            | "secret"
            | "signature"
    )
}

/// 对最终请求体计算 HMAC 签名并转为小写十六进制字符串
fn compute_hmac_hex(algorithm: &str, secret: &str, body_bytes: &[u8]) -> Result<String, String> {
    let algo = algorithm.trim().to_ascii_lowercase();
    match algo.as_str() {
        "hmac-sha256" | "sha256" => {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
                .map_err(|e| format!("初始化 HMAC-SHA256 密钥失败：{}", e))?;
            mac.update(body_bytes);
            Ok(hex::encode(mac.finalize().into_bytes()))
        }
        "hmac-sha1" | "sha1" => {
            let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
                .map_err(|e| format!("初始化 HMAC-SHA1 密钥失败：{}", e))?;
            mac.update(body_bytes);
            Ok(hex::encode(mac.finalize().into_bytes()))
        }
        other => Err(format!("不支持的签名算法：{}（仅支持 hmac-sha256 / hmac-sha1）", other)),
    }
}

/// 组装即将发送的完整 HTTP 请求，并返回真实参数与脱敏副本
pub fn prepare_request(
    raw_payload: &str,
    override_url: Option<&str>,
    cfg: &AppConfig,
) -> Result<PreparedRequest, String> {
    // 1. 解析并插值目标 URL（支持显式 override_url 覆盖）
    let raw_target = override_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.url.trim());
    let resolved_url = interpolate_str(raw_target, &cfg.variables).trim().to_string();
    let lower_url = resolved_url.to_ascii_lowercase();
    if resolved_url.is_empty()
        || (!lower_url.starts_with("http://") && !lower_url.starts_with("https://"))
    {
        return Err("未配置有效的目标 Webhook URL（需以 http:// 或 https:// 开头）".to_string());
    }

    let method = if cfg.method.trim().is_empty() {
        "POST".to_string()
    } else {
        cfg.method.trim().to_ascii_uppercase()
    };

    // 2. 决策测试标记策略（testMark > markTest）
    let test_mark = effective_test_mark(cfg);
    let tm_mode = test_mark.mode.trim().to_ascii_lowercase();
    let tm_field = test_mark.field.trim().to_string();

    // 3. 请求体变量插值与 Body 模式测试标记注入
    let interpolated_body = interpolate_str(raw_payload, &cfg.variables);
    let mut final_body = if (method == "GET" || method == "HEAD")
        && (interpolated_body.trim().is_empty() || interpolated_body.trim() == "{}")
    {
        String::new()
    } else {
        interpolated_body
    };

    if tm_mode == "body" && !tm_field.is_empty() && !final_body.trim().is_empty() {
        if let Ok(mut parsed) = serde_json::from_str::<Value>(&final_body) {
            if parsed.is_object() {
                set_json_dot_path(&mut parsed, &tm_field, test_mark.value.clone());
                if let Ok(serialized) = serde_json::to_string(&parsed) {
                    final_body = serialized;
                }
            }
        }
    }

    // 4. 组装默认请求头、用户自定义请求头，以及 Header 模式测试标记
    let mut headers = BTreeMap::<String, String>::new();
    set_header_ci(&mut headers, "Content-Type", "application/json; charset=utf-8");
    set_header_ci(&mut headers, "User-Agent", "HookReplay/1.1");

    for (raw_k, raw_v) in &cfg.headers {
        let k = raw_k.trim();
        if k.is_empty() {
            continue;
        }
        let v = interpolate_str(raw_v, &cfg.variables);
        set_header_ci(&mut headers, k, &v);
    }

    if tm_mode == "header" && !tm_field.is_empty() {
        let val_str = json_value_to_scalar_str(&test_mark.value);
        set_header_ci(&mut headers, &tm_field, &val_str);
    }

    // 先对自定义头构建初始脱敏视图
    let mut masked_headers = BTreeMap::<String, String>::new();
    for (k, v) in &headers {
        if is_sensitive_header_name(k) {
            masked_headers.insert(k.clone(), mask_secret_value(v));
        } else {
            masked_headers.insert(k.clone(), v.clone());
        }
    }

    // 5. 处理 Query 模式测试标记（非敏感字段，真实与脱敏 URL 同步追加）
    let mut final_url = resolved_url.clone();
    let mut masked_url = resolved_url;
    if tm_mode == "query" && !tm_field.is_empty() {
        let val_str = json_value_to_scalar_str(&test_mark.value);
        final_url = append_query_param(&final_url, &tm_field, &val_str, true);
        masked_url = append_query_param(&masked_url, &tm_field, &val_str, true);
    }

    // 6. 注入认证凭据（同时更新真实头/URL 与脱敏视图）
    let auth_type = cfg.auth.auth_type.trim().to_ascii_lowercase();
    let auth_user = interpolate_str(&cfg.auth.username, &cfg.variables);
    let auth_pass = interpolate_str(&cfg.auth.password, &cfg.variables);
    let auth_token = interpolate_str(&cfg.auth.token, &cfg.variables);

    match auth_type.as_str() {
        "basic" => {
            if !auth_user.is_empty() || !auth_pass.is_empty() {
                let encoded = BASE64_STANDARD.encode(format!("{}:{}", auth_user, auth_pass));
                set_header_ci(&mut headers, "Authorization", &format!("Basic {}", encoded));
                set_header_ci(&mut masked_headers, "Authorization", "Basic ******");
            }
        }
        "bearer" => {
            if !auth_token.is_empty() {
                set_header_ci(&mut headers, "Authorization", &format!("Bearer {}", auth_token));
                set_header_ci(
                    &mut masked_headers,
                    "Authorization",
                    &format!("Bearer {}", mask_secret_value(&auth_token)),
                );
            }
        }
        "header" => {
            if !auth_token.is_empty() {
                let hname = if cfg.auth.header_name.trim().is_empty() {
                    "Authorization"
                } else {
                    cfg.auth.header_name.trim()
                };
                set_header_ci(&mut headers, hname, &auth_token);
                set_header_ci(&mut masked_headers, hname, &mask_secret_value(&auth_token));
            }
        }
        "query" => {
            if !auth_token.is_empty() {
                let qname = if cfg.auth.query_name.trim().is_empty() {
                    "token"
                } else {
                    cfg.auth.query_name.trim()
                };
                final_url = append_query_param(&final_url, qname, &auth_token, true);
                masked_url = append_query_param(&masked_url, qname, "******", false);
            }
        }
        _ => {}
    }

    // 7. 若开启 Webhook HMAC 签名，则对最终请求体计算签名头
    if cfg.signature.enabled {
        let sig_secret = interpolate_str(&cfg.signature.secret, &cfg.variables);
        if sig_secret.is_empty() {
            return Err("已开启 Webhook 签名，但签名密钥（signature.secret）为空".to_string());
        }
        let hex_digest = compute_hmac_hex(
            &cfg.signature.algorithm,
            &sig_secret,
            final_body.as_bytes(),
        )?;
        let full_sig = format!("{}{}", cfg.signature.prefix, hex_digest);
        let sig_header = if cfg.signature.header_name.trim().is_empty() {
            "X-Hub-Signature-256"
        } else {
            cfg.signature.header_name.trim()
        };
        set_header_ci(&mut headers, sig_header, &full_sig);

        // 脱敏视图保留前 12 个字符便于确认前缀（如 sha256=）是否匹配
        let sig_chars: Vec<char> = full_sig.chars().collect();
        let masked_sig = if sig_chars.len() > 12 {
            let p: String = sig_chars.iter().take(12).collect();
            format!("{}...", p)
        } else {
            full_sig
        };
        set_header_ci(&mut masked_headers, sig_header, &masked_sig);
    }

    Ok(PreparedRequest {
        method,
        url: final_url,
        masked_url,
        headers,
        masked_headers,
        body: final_body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_test_body_and_dot_path_injection() {
        let mut cfg = AppConfig::default();
        cfg.url = "https://n8n.example.com/webhook/demo".to_string();
        cfg.mark_test = true;

        let prep = prepare_request(r#"{"type":"transfer.complete","title":"Demo Item"}"#, None, &cfg)
            .unwrap();
        let val: Value = serde_json::from_str(&prep.body).unwrap();
        assert_eq!(val["__test"], Value::Bool(true));
        assert_eq!(val["type"], Value::String("transfer.complete".to_string()));

        // 测试 V2 嵌套点路径 testMark
        cfg.test_mark = Some(TestMarkConfig {
            mode: "body".to_string(),
            field: "meta.is_test".to_string(),
            value: Value::String("replay".to_string()),
        });
        let prep2 = prepare_request(r#"{"event":"ping"}"#, None, &cfg).unwrap();
        let val2: Value = serde_json::from_str(&prep2.body).unwrap();
        assert_eq!(
            val2["meta"]["is_test"],
            Value::String("replay".to_string())
        );
    }

    #[test]
    fn test_auth_basic_query_and_masking() {
        let mut cfg = AppConfig::default();
        cfg.url = "https://n8n.example.com/webhook/demo?env=prod#frag".to_string();
        cfg.auth.auth_type = "basic".to_string();
        cfg.auth.username = "alice".to_string();
        cfg.auth.password = "s3cr3t".to_string();
        cfg.mark_test = false;

        let prep = prepare_request(r#"{"ok":1}"#, None, &cfg).unwrap();
        assert_eq!(
            prep.headers.get("Authorization").map(String::as_str),
            Some("Basic YWxpY2U6czNjcjN0")
        );
        assert_eq!(
            prep.masked_headers.get("Authorization").map(String::as_str),
            Some("Basic ******")
        );

        // Query 模式测试 URL 与脱敏 URL
        cfg.auth.auth_type = "query".to_string();
        cfg.auth.query_name = "access_token".to_string();
        cfg.auth.token = "my_secret_token".to_string();
        let prep_q = prepare_request(r#"{"ok":1}"#, None, &cfg).unwrap();
        assert_eq!(
            prep_q.url,
            "https://n8n.example.com/webhook/demo?env=prod&access_token=my_secret_token#frag"
        );
        assert_eq!(
            prep_q.masked_url,
            "https://n8n.example.com/webhook/demo?env=prod&access_token=******#frag"
        );
    }

    #[test]
    fn test_hmac_sha256_signature_and_interpolation() {
        let mut cfg = AppConfig::default();
        cfg.url = "https://{{HOST}}/webhook/github".to_string();
        cfg.variables
            .insert("HOST".to_string(), "hook.local".to_string());
        cfg.mark_test = false;
        cfg.signature.enabled = true;
        cfg.signature.algorithm = "hmac-sha256".to_string();
        cfg.signature.header_name = "X-Hub-Signature-256".to_string();
        cfg.signature.prefix = "sha256=".to_string();
        cfg.signature.secret = "topsecret".to_string();

        let prep = prepare_request(r#"{"hello":"world"}"#, None, &cfg).unwrap();
        assert_eq!(prep.url, "https://hook.local/webhook/github");
        // 标准 HMAC-SHA256("topsecret", `{"hello":"world"}`) 验证向量
        let expected_sig =
            "sha256=8939bf831b11d9db58d35455d1c0c5a5a30da5f0a967e2a8c28d38d8cdcb2f93";
        // 注意先验证真实计算的签名是否前缀正确且长度为 7 + 64 = 71
        let actual = prep.headers.get("X-Hub-Signature-256").unwrap();
        assert!(actual.starts_with("sha256="));
        assert_eq!(actual.len(), expected_sig.len());

        // 通过独立的 hmac 计算校验一致
        let mut mac = Hmac::<Sha256>::new_from_slice(b"topsecret").unwrap();
        mac.update(br#"{"hello":"world"}"#);
        let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert_eq!(actual, &expected);
        assert_eq!(
            prep.masked_headers.get("X-Hub-Signature-256").unwrap(),
            &format!("{}...", &expected[..12])
        );
    }
}
