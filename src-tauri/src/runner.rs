//! 异步批量回放控制引擎、单条重发、Ping 探测与报告落盘模块（Runner）
//!
//! 核心职责：
//! 1. 执行批量回放任务：严格按分段推进、支持并发池（Semaphore）、限流延迟、重试与段间等待；
//! 2. 实现 `interactive: true` 交互确认机制：每段执行前挂起并推送 `run://segment-waiting`，
//!    响应前端的 y（发送本段）/ n（跳过本段）/ a（全部自动）/ q（中止）；
//! 3. 支持运行中随时暂停（Pause）、恢复（Resume）、强行中止（Abort）与单条在线改写重发；
//! 4. 输出标准 JSON 测试报告与逐条响应落盘，自动按时间戳归档历史运行记录。

use crate::config::{
    normalize_path_key, normalize_path_string, resolve_base_dir, resolve_path, AppConfig,
};
use crate::matcher::{evaluate_with_config, EvaluationOutcome};
use crate::mutator::prepare_request;
use crate::parser::{read_entry_raw, IndexedEntry};
use chrono::{DateTime, Local, Utc};
use reqwest::{Client, Method};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Mutex, Notify, Semaphore};
use uuid::Uuid;

/// 交互确认动作枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentAction {
    Y,
    N,
    A,
    Q,
}

/// 批量任务当前生命周期状态（通过 watch 通道实时同步）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlSignal {
    Running,
    Paused,
    Aborted,
}

/// 全局任务运行控制句柄（存放在 Tauri State 中）
pub struct RunManager {
    pub active_run: Mutex<Option<Arc<ActiveRunHandle>>>,
}

impl Default for RunManager {
    fn default() -> Self {
        Self {
            active_run: Mutex::new(None),
        }
    }
}

/// 正在运行的任务同步原语
pub struct ActiveRunHandle {
    pub run_id: String,
    control_tx: watch::Sender<ControlSignal>,
    confirm_tx: Mutex<Option<tokio::sync::oneshot::Sender<SegmentAction>>>,
    resume_notify: Notify,
}

/// 单条执行结果（与前端列表和导出 JSON 报告共用）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunItemResult {
    pub index: usize,
    pub segment: usize,
    pub source: String,
    pub line: usize,
    pub label: String,
    pub event_type: String,
    pub status: u16,
    pub outcome: String,
    pub badge: String,
    pub ok: bool,
    pub unmatched: bool,
    pub ms: u64,
    pub error: String,
    pub response: String,
    pub final_url: String,
    pub masked_headers: BTreeMap<String, String>,
    pub sent_body: String,
}

/// 历史报告简要信息（供左栏报告列表渲染）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    #[serde(default)]
    pub run_id: String,
    pub file_name: String,
    pub path: String,
    pub started_at: String,
    pub finished_at: String,
    pub target: String,
    pub total: usize,
    pub ok: usize,
    pub deduped: usize,
    pub unmatched: usize,
    pub failed: usize,
    pub duration_ms: u64,
}

/// 完整报告结构（向后兼容原 `latest.json` 字段，同时追加 `deduped` 细粒度统计与 `runId`）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullRunReport {
    #[serde(default)]
    pub run_id: String,
    pub version: String,
    pub started_at: String,
    pub finished_at: String,
    pub target: String,
    pub auth: String,
    pub strict: bool,
    pub mark_test: bool,
    pub total: usize,
    pub ok: usize,
    #[serde(default)]
    pub deduped: usize,
    pub unmatched: usize,
    pub failed: usize,
    pub parse_errors: usize,
    pub comments_skipped: usize,
    pub segments: usize,
    pub segment_wait_ms: u64,
    pub interactive: bool,
    pub skipped_items: usize,
    pub skipped_segments: usize,
    pub duration_ms: u64,
    pub files: Vec<String>,
    pub results: Vec<RunItemResult>,
}

/// 推送给前端的分段等待确认事件载荷
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentWaitingPayload {
    pub run_id: String,
    pub segment_index: usize,
    pub total_segments: usize,
    pub segment_id: usize,
    pub comment: String,
    pub source_name: String,
    pub start_line: usize,
    pub count: usize,
}

/// 推送给前端的单条状态更新事件载荷
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemProgressPayload {
    pub run_id: String,
    pub done_count: usize,
    pub total_selected: usize,
    pub current_segment: usize,
    pub result: RunItemResult,
}

/// 推送给前端的任务状态变更载荷
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatusPayload {
    pub run_id: String,
    pub state: String,
    pub message: String,
}

/// 推送给前端的段间等待倒计时事件载荷
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentWaitCountdownPayload {
    pub run_id: String,
    pub wait_ms: u64,
    pub next_segment: usize,
}

/// Ping 探测结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub reachable: bool,
    pub status: u16,
    pub ms: u64,
    pub message: String,
}

/// 构造复用的异步 HTTP 客户端（使用 Windows 原生 SChannel 支持自签名证书跳过）
fn build_http_client(cfg: &AppConfig) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_millis(cfg.timeout_ms.max(500)))
        .danger_accept_invalid_certs(cfg.insecure)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败：{}", e))
}

/// 执行单次 HTTP 请求（内含重试机制：仅对网络错误与 5xx 重试）
async fn send_with_retry(
    client: &Client,
    method: &Method,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &str,
    retries: u32,
    retry_delay_ms: u64,
) -> Result<(u16, String, u64), String> {
    let mut last_err = String::new();
    for attempt in 0..=retries {
        let started = Instant::now();
        let mut req = client.request(method.clone(), url);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        if !body.is_empty() {
            req = req.body(body.to_string());
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let ms = started.elapsed().as_millis() as u64;
                if status >= 500 && attempt < retries {
                    last_err = format!("HTTP {}", status);
                } else {
                    let bytes = resp.bytes().await.unwrap_or_default();
                    let max_len = bytes.len().min(200 * 1024);
                    let resp_text = String::from_utf8_lossy(&bytes[..max_len]).to_string();
                    return Ok((status, resp_text, ms));
                }
            }
            Err(err) => {
                last_err = if err.is_timeout() {
                    format!("请求超时（>{}ms）", err)
                } else {
                    err.to_string()
                };
            }
        }
        if attempt < retries && retry_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(retry_delay_ms)).await;
        }
    }
    Err(last_err)
}

/// 探测目标入口连通性
pub async fn ping_endpoint(override_url: Option<&str>, cfg: &AppConfig) -> PingResult {
    let dummy = r#"{"__ping":true,"__test":true}"#;
    let prep = match prepare_request(dummy, override_url, cfg) {
        Ok(p) => p,
        Err(e) => {
            return PingResult {
                reachable: false,
                status: 0,
                ms: 0,
                message: e,
            }
        }
    };
    let client = match build_http_client(cfg) {
        Ok(c) => c,
        Err(e) => {
            return PingResult {
                reachable: false,
                status: 0,
                ms: 0,
                message: e,
            }
        }
    };
    let started = Instant::now();
    // 使用 HEAD/OPTIONS 探测避免在真实工作流触发动作；若 405 则说明端口与 TLS 已完全连通
    let mut req = client.request(Method::HEAD, &prep.url);
    for (k, v) in &prep.headers {
        req = req.header(k, v);
    }
    match req.send().await {
        Ok(resp) => {
            let ms = started.elapsed().as_millis() as u64;
            let st = resp.status().as_u16();
            PingResult {
                reachable: true,
                status: st,
                ms,
                message: format!("网络与 TLS 已连通（HTTP {}，{}ms）", st, ms),
            }
        }
        Err(err) => PingResult {
            reachable: false,
            status: 0,
            ms: started.elapsed().as_millis() as u64,
            message: format!("无法连接目标：{}", err),
        },
    }
}

/// 将单条直接重发结果封装为标准历史运行报告对象
pub(crate) fn build_single_run_report(
    item: &RunItemResult,
    entry: &IndexedEntry,
    cfg: &AppConfig,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    elapsed_ms: u64,
) -> FullRunReport {
    let target = if !item.final_url.is_empty() {
        item.final_url.clone()
    } else {
        cfg.url.clone()
    };
    let files = if !entry.source.is_empty() {
        vec![entry.source.clone()]
    } else {
        Vec::new()
    };

    FullRunReport {
        run_id: Uuid::new_v4().to_string(),
        version: "1.2.0".to_string(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        target,
        auth: cfg.auth.auth_type.clone(),
        strict: cfg.assertions.strict,
        mark_test: cfg.mark_test,
        total: 1,
        ok: usize::from(item.ok),
        deduped: usize::from(item.outcome == "deduped"),
        unmatched: usize::from(item.unmatched),
        failed: usize::from(item.outcome == "failed"),
        parse_errors: 0,
        comments_skipped: 0,
        segments: 1,
        segment_wait_ms: 0,
        interactive: false,
        skipped_items: 0,
        skipped_segments: 0,
        duration_ms: item.ms.max(elapsed_ms),
        files,
        results: vec![item.clone()],
    }
}

/// 在线单条即时重发（Inspector 改写后直接发送、返回完整结果并自动归档历史报告）
pub async fn replay_single(
    raw_json: &str,
    entry: &IndexedEntry,
    override_url: Option<&str>,
    cfg: &AppConfig,
    base_dir: &Path,
    history_dir: &Path,
) -> Result<RunItemResult, String> {
    let prep = prepare_request(raw_json, override_url, cfg)?;
    let client = build_http_client(cfg)?;
    let method = Method::from_bytes(prep.method.as_bytes())
        .map_err(|e| format!("无效的 HTTP 方法：{}", e))?;

    let started_at_dt = Utc::now();
    let started_clock = Instant::now();

    let item = match send_with_retry(
        &client,
        &method,
        &prep.url,
        &prep.headers,
        &prep.body,
        cfg.retries,
        cfg.retry_delay_ms,
    )
    .await
    {
        Ok((status, resp_body, ms)) => {
            let eval: EvaluationOutcome = evaluate_with_config(status, &resp_body, cfg);
            RunItemResult {
                index: entry.index,
                segment: entry.segment,
                source: entry.source.clone(),
                line: entry.line,
                label: entry.label.clone(),
                event_type: entry.event_type.clone(),
                status,
                outcome: eval.outcome,
                badge: eval.badge,
                ok: eval.ok,
                unmatched: eval.unmatched,
                ms,
                error: String::new(),
                response: resp_body.chars().take(12_000).collect(),
                final_url: prep.masked_url,
                masked_headers: prep.masked_headers,
                sent_body: prep.body,
            }
        }
        Err(err) => RunItemResult {
            index: entry.index,
            segment: entry.segment,
            source: entry.source.clone(),
            line: entry.line,
            label: entry.label.clone(),
            event_type: entry.event_type.clone(),
            status: 0,
            outcome: "failed".to_string(),
            badge: "网络错误".to_string(),
            ok: false,
            unmatched: false,
            ms: started_clock.elapsed().as_millis() as u64,
            error: err,
            response: String::new(),
            final_url: prep.masked_url,
            masked_headers: prep.masked_headers,
            sent_body: prep.body,
        },
    };

    let finished_at_dt = Utc::now();
    let elapsed_ms = started_clock.elapsed().as_millis() as u64;
    let report_obj = build_single_run_report(
        &item,
        entry,
        cfg,
        started_at_dt,
        finished_at_dt,
        elapsed_ms,
    );
    persist_run_report_with_prefix(&report_obj, cfg, base_dir, history_dir, "single");

    Ok(item)
}

/// 等待任务从暂停状态恢复或中止
async fn wait_if_paused(handle: &ActiveRunHandle, mut rx: watch::Receiver<ControlSignal>) -> bool {
    loop {
        let sig = *rx.borrow_and_update();
        match sig {
            ControlSignal::Running => return true,
            ControlSignal::Aborted => return false,
            ControlSignal::Paused => {
                tokio::select! {
                    _ = rx.changed() => {},
                    _ = handle.resume_notify.notified() => {},
                }
            }
        }
    }
}

/// 启动后台批量回放任务（通过 Tauri Event 实时向前端推送进度与交互信号）
pub async fn start_batch_run(
    app: AppHandle,
    manager: Arc<RunManager>,
    entries: Vec<IndexedEntry>,
    cfg: AppConfig,
    yaml_path: Option<String>,
    history_dir: PathBuf,
    parse_errors_count: usize,
    comments_skipped: usize,
    files: Vec<String>,
) -> Result<String, String> {
    if entries.is_empty() {
        return Err("没有可发送的条目，请先勾选分段或消息".to_string());
    }
    // 提前校验一次 URL 与客户端构造，配置错误立即在启动时返回中文报错
    let _ = prepare_request("{}", None, &cfg)?;
    let client = build_http_client(&cfg)?;

    let mut lock = manager.active_run.lock().await;
    if lock.is_some() {
        return Err("已有回放任务正在执行中，请先等待结束或停止当前任务".to_string());
    }

    let run_id = Uuid::new_v4().to_string();
    let (control_tx, control_rx) = watch::channel(ControlSignal::Running);
    let handle = Arc::new(ActiveRunHandle {
        run_id: run_id.clone(),
        control_tx,
        confirm_tx: Mutex::new(None),
        resume_notify: Notify::new(),
    });
    *lock = Some(handle.clone());
    drop(lock);

    // 按出现顺序分组为段
    let mut groups: Vec<Vec<IndexedEntry>> = Vec::new();
    let mut seg_idx_map = BTreeMap::<usize, usize>::new();
    for entry in entries {
        if let Some(&pos) = seg_idx_map.get(&entry.segment) {
            groups[pos].push(entry);
        } else {
            seg_idx_map.insert(entry.segment, groups.len());
            groups.push(vec![entry]);
        }
    }

    let total_selected: usize = groups.iter().map(|g| g.len()).sum();
    let total_segments = groups.len();
    let base_dir = resolve_base_dir(yaml_path.as_deref());
    let run_id_clone = run_id.clone();

    tokio::spawn(async move {
        let started_at_dt = Utc::now();
        let started_clock = Instant::now();
        let _ = app.emit(
            "run://status",
            RunStatusPayload {
                run_id: run_id_clone.clone(),
                state: "running".to_string(),
                message: format!("开始回放：共 {} 条（{} 段）", total_selected, total_segments),
            },
        );

        let mut all_results: Vec<RunItemResult> = Vec::with_capacity(total_selected);
        let mut send_all = false;
        let mut skipped_items = 0usize;
        let mut skipped_segments = 0usize;
        let mut done_count = 0usize;
        let mut aborted_early = false;

        let save_dir_path = if cfg.save_response_dir.trim().is_empty() {
            None
        } else {
            let p = resolve_path(&cfg.save_response_dir, &base_dir);
            let _ = fs::create_dir_all(&p);
            Some(p)
        };

        for (group_idx, group) in groups.iter().enumerate() {
            if !wait_if_paused(&handle, control_rx.clone()).await {
                aborted_early = true;
                break;
            }

            // 交互确认模式：发每段前询问（除非已选 a=全部自动发送）
            if cfg.interactive && !send_all {
                let first = &group[0];
                let (tx, rx) = tokio::sync::oneshot::channel::<SegmentAction>();
                {
                    let mut guard = handle.confirm_tx.lock().await;
                    *guard = Some(tx);
                }
                let _ = app.emit(
                    "run://segment-waiting",
                    SegmentWaitingPayload {
                        run_id: run_id_clone.clone(),
                        segment_index: group_idx + 1,
                        total_segments,
                        segment_id: first.segment,
                        comment: first.comment.clone(),
                        source_name: first.source_name.clone(),
                        start_line: first.line,
                        count: group.len(),
                    },
                );

                // 等待用户点按钮或任务被强行中止
                let mut abort_rx = control_rx.clone();
                let chosen = tokio::select! {
                    res = rx => res.unwrap_or(SegmentAction::Q),
                    _ = async {
                        loop {
                            if *abort_rx.borrow_and_update() == ControlSignal::Aborted {
                                break;
                            }
                            if abort_rx.changed().await.is_err() {
                                break;
                            }
                        }
                    } => SegmentAction::Q,
                };
                {
                    let mut guard = handle.confirm_tx.lock().await;
                    *guard = None;
                }

                match chosen {
                    SegmentAction::Q => {
                        skipped_items += groups[group_idx..].iter().map(|g| g.len()).sum::<usize>();
                        skipped_segments += total_segments - group_idx;
                        aborted_early = true;
                        break;
                    }
                    SegmentAction::N => {
                        skipped_items += group.len();
                        skipped_segments += 1;
                        continue;
                    }
                    SegmentAction::A => {
                        send_all = true;
                    }
                    SegmentAction::Y => {}
                }
            }

            // 段内按 concurrency 并发发送（当 concurrency == 1 时严格保序）
            let sem = Arc::new(Semaphore::new(cfg.concurrency.max(1)));
            let mut join_set = tokio::task::JoinSet::new();
            let group_len = group.len();

            for (pos, entry) in group.iter().cloned().enumerate() {
                if cfg.concurrency <= 1 {
                    if !wait_if_paused(&handle, control_rx.clone()).await {
                        aborted_early = true;
                        break;
                    }
                    let res = execute_one_entry(&client, &entry, &cfg, save_dir_path.as_deref()).await;
                    done_count += 1;
                    let _ = app.emit(
                        "run://item-done",
                        ItemProgressPayload {
                            run_id: run_id_clone.clone(),
                            done_count,
                            total_selected,
                            current_segment: entry.segment,
                            result: res.clone(),
                        },
                    );
                    all_results.push(res);
                    if cfg.delay_ms > 0 && pos + 1 < group_len {
                        tokio::time::sleep(Duration::from_millis(cfg.delay_ms)).await;
                    }
                } else {
                    let permit = sem.clone().acquire_owned().await.unwrap();
                    let client_c = client.clone();
                    let cfg_c = cfg.clone();
                    let save_dir_c = save_dir_path.clone();
                    let handle_c = handle.clone();
                    let rx_c = control_rx.clone();
                    join_set.spawn(async move {
                        let _p = permit;
                        if !wait_if_paused(&handle_c, rx_c).await {
                            return None;
                        }
                        let res = execute_one_entry(&client_c, &entry, &cfg_c, save_dir_c.as_deref()).await;
                        if cfg_c.delay_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(cfg_c.delay_ms)).await;
                        }
                        Some(res)
                    });
                }
            }

            if cfg.concurrency > 1 {
                let mut seg_results = Vec::new();
                while let Some(joined) = join_set.join_next().await {
                    if let Ok(Some(res)) = joined {
                        done_count += 1;
                        let _ = app.emit(
                            "run://item-done",
                            ItemProgressPayload {
                                run_id: run_id_clone.clone(),
                                done_count,
                                total_selected,
                                current_segment: res.segment,
                                result: res.clone(),
                            },
                        );
                        seg_results.push(res);
                    }
                }
                seg_results.sort_by_key(|r| r.index);
                all_results.extend(seg_results);
                if *control_rx.borrow() == ControlSignal::Aborted {
                    aborted_early = true;
                }
            }

            if aborted_early {
                break;
            }

            // 段与段之间等待
            if group_idx + 1 < total_segments && cfg.segment_wait_ms > 0 {
                let _ = app.emit(
                    "run://segment-wait-countdown",
                    SegmentWaitCountdownPayload {
                        run_id: run_id_clone.clone(),
                        wait_ms: cfg.segment_wait_ms,
                        next_segment: groups[group_idx + 1][0].segment,
                    },
                );
                let mut abort_rx = control_rx.clone();
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(cfg.segment_wait_ms)) => {},
                    _ = async {
                        loop {
                            if *abort_rx.borrow_and_update() == ControlSignal::Aborted {
                                break;
                            }
                            if abort_rx.changed().await.is_err() {
                                break;
                            }
                        }
                    } => {
                        aborted_early = true;
                        break;
                    }
                }
            }
        }

        // 统计结果并按配置写入报告文件
        let finished_at_dt = Utc::now();
        let duration_ms = started_clock.elapsed().as_millis() as u64;
        let ok_count = all_results.iter().filter(|r| r.ok).count();
        let deduped_count = all_results.iter().filter(|r| r.outcome == "deduped").count();
        let unmatched_count = all_results.iter().filter(|r| r.unmatched).count();
        let failed_count = all_results.iter().filter(|r| r.outcome == "failed").count();

        let report_obj = FullRunReport {
            run_id: run_id_clone.clone(),
            version: "1.2.0".to_string(),
            started_at: started_at_dt.to_rfc3339(),
            finished_at: finished_at_dt.to_rfc3339(),
            target: cfg.url.clone(),
            auth: cfg.auth.auth_type.clone(),
            strict: cfg.assertions.strict,
            mark_test: cfg.mark_test,
            total: all_results.len(),
            ok: ok_count,
            deduped: deduped_count,
            unmatched: unmatched_count,
            failed: failed_count,
            parse_errors: parse_errors_count,
            comments_skipped,
            segments: total_segments,
            segment_wait_ms: cfg.segment_wait_ms,
            interactive: cfg.interactive,
            skipped_items,
            skipped_segments,
            duration_ms,
            files,
            results: all_results,
        };

        if report_obj.total > 0 {
            persist_run_report(&report_obj, &cfg, &base_dir, &history_dir);
        }

        {
            let mut guard = manager.active_run.lock().await;
            *guard = None;
        }

        let _ = app.emit("run://finished", &report_obj);
        let _ = app.emit(
            "run://status",
            RunStatusPayload {
                run_id: run_id_clone,
                state: if aborted_early { "aborted" } else { "finished" }.to_string(),
                message: if aborted_early {
                    "回放已停止"
                } else {
                    "全部选中条目已回放完成"
                }
                .to_string(),
            },
        );
    });

    Ok(run_id)
}

/// 执行单条索引条目的读取、改写与发送
async fn execute_one_entry(
    client: &Client,
    entry: &IndexedEntry,
    cfg: &AppConfig,
    save_dir: Option<&Path>,
) -> RunItemResult {
    let raw = match read_entry_raw(entry) {
        Ok(r) => r,
        Err(err) => {
            return RunItemResult {
                index: entry.index,
                segment: entry.segment,
                source: entry.source.clone(),
                line: entry.line,
                label: entry.label.clone(),
                event_type: entry.event_type.clone(),
                status: 0,
                outcome: "failed".to_string(),
                badge: "读取失败".to_string(),
                ok: false,
                unmatched: false,
                ms: 0,
                error: err,
                response: String::new(),
                final_url: cfg.url.clone(),
                masked_headers: BTreeMap::new(),
                sent_body: String::new(),
            };
        }
    };

    let prep = match prepare_request(&raw, None, cfg) {
        Ok(p) => p,
        Err(err) => {
            return RunItemResult {
                index: entry.index,
                segment: entry.segment,
                source: entry.source.clone(),
                line: entry.line,
                label: entry.label.clone(),
                event_type: entry.event_type.clone(),
                status: 0,
                outcome: "failed".to_string(),
                badge: "构造失败".to_string(),
                ok: false,
                unmatched: false,
                ms: 0,
                error: err,
                response: String::new(),
                final_url: cfg.url.clone(),
                masked_headers: BTreeMap::new(),
                sent_body: raw,
            };
        }
    };

    let method = Method::from_bytes(prep.method.as_bytes()).unwrap_or(Method::POST);
    match send_with_retry(
        client,
        &method,
        &prep.url,
        &prep.headers,
        &prep.body,
        cfg.retries,
        cfg.retry_delay_ms,
    )
    .await
    {
        Ok((status, resp_body, ms)) => {
            if let Some(dir) = save_dir {
                let fname = format!("{:03}-{}.json", entry.index, status);
                let _ = fs::write(dir.join(fname), &resp_body);
            }
            let eval = evaluate_with_config(status, &resp_body, cfg);
            RunItemResult {
                index: entry.index,
                segment: entry.segment,
                source: entry.source.clone(),
                line: entry.line,
                label: entry.label.clone(),
                event_type: entry.event_type.clone(),
                status,
                outcome: eval.outcome,
                badge: eval.badge,
                ok: eval.ok,
                unmatched: eval.unmatched,
                ms,
                error: String::new(),
                response: resp_body.chars().take(4000).collect(),
                final_url: prep.masked_url,
                masked_headers: prep.masked_headers,
                sent_body: prep.body,
            }
        }
        Err(err) => RunItemResult {
            index: entry.index,
            segment: entry.segment,
            source: entry.source.clone(),
            line: entry.line,
            label: entry.label.clone(),
            event_type: entry.event_type.clone(),
            status: 0,
            outcome: "failed".to_string(),
            badge: "网络错误".to_string(),
            ok: false,
            unmatched: false,
            ms: 0,
            error: err,
            response: String::new(),
            final_url: prep.masked_url,
            masked_headers: prep.masked_headers,
            sent_body: prep.body,
        },
    }
}

/// 对运行中的任务执行暂停 / 恢复 / 中止 / 分段确认控制
pub async fn control_run(
    manager: &RunManager,
    action: &str,
    segment_action: Option<SegmentAction>,
) -> Result<(), String> {
    let guard = manager.active_run.lock().await;
    let Some(ref handle) = *guard else {
        return Err("当前没有正在执行的任务".to_string());
    };
    match action {
        "pause" => {
            let _ = handle.control_tx.send(ControlSignal::Paused);
        }
        "resume" => {
            let _ = handle.control_tx.send(ControlSignal::Running);
            handle.resume_notify.notify_waiters();
        }
        "abort" => {
            let _ = handle.control_tx.send(ControlSignal::Aborted);
            handle.resume_notify.notify_waiters();
            if let Some(tx) = handle.confirm_tx.lock().await.take() {
                let _ = tx.send(SegmentAction::Q);
            }
        }
        "confirm" => {
            let act = segment_action.ok_or_else(|| "缺少分段确认动作参数".to_string())?;
            if let Some(tx) = handle.confirm_tx.lock().await.take() {
                let _ = tx.send(act);
            } else {
                return Err("当前并未处于分段确认等待状态".to_string());
            }
        }
        _ => return Err(format!("未知的控制动作：{}", action)),
    }
    Ok(())
}

/// 将单次运行报告永久归档为时间戳文件，并在配置了 `cfg.report` 时兼容写入目标文件（如 latest.json）
pub(crate) fn persist_run_report(
    report_obj: &FullRunReport,
    cfg: &AppConfig,
    base_dir: &Path,
    history_dir: &Path,
) {
    persist_run_report_with_prefix(report_obj, cfg, base_dir, history_dir, "run");
}

/// 按指定文件名前缀将运行报告归档保存（批量运行用 `run`，单条发送用 `single`）
pub(crate) fn persist_run_report_with_prefix(
    report_obj: &FullRunReport,
    cfg: &AppConfig,
    base_dir: &Path,
    history_dir: &Path,
    prefix: &str,
) {
    let Ok(json_text) = serde_json::to_string_pretty(report_obj) else {
        return;
    };
    let payload = format!("{}\n", json_text);

    // 1. 决定历史归档目录：优先使用 cfg.report 所在目录，否则使用用户数据目录下的 history/
    let archive_dir = if !cfg.report.trim().is_empty() {
        let rp = resolve_path(&cfg.report, base_dir);
        rp.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| history_dir.to_path_buf())
    } else {
        history_dir.to_path_buf()
    };

    let _ = fs::create_dir_all(&archive_dir);
    let ts = Local::now().format("%Y%m%d-%H%M%S");
    let short_id: String = report_obj.run_id.chars().take(6).collect();
    let archive_name = if short_id.is_empty() {
        format!("{}-{}.json", prefix, ts)
    } else {
        format!("{}-{}-{}.json", prefix, ts, short_id)
    };
    let mut archive_path = archive_dir.join(&archive_name);
    if archive_path.exists() {
        // 防极端并发下同秒同前缀碰撞
        let unique_suffix = Uuid::new_v4().to_string();
        archive_path = archive_dir.join(format!("{}-{}-{}.json", prefix, ts, &unique_suffix[..6]));
    }
    let _ = fs::write(&archive_path, &payload);

    // 2. 若配置了指定报告输出路径（例如 ./test_reports/latest.json），同步写入以兼容 CLI 与外部脚本
    if !cfg.report.trim().is_empty() {
        let report_path = resolve_path(&cfg.report, base_dir);
        if normalize_path_key(&report_path.to_string_lossy())
            != normalize_path_key(&archive_path.to_string_lossy())
        {
            if let Some(parent) = report_path.parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = fs::create_dir_all(parent);
                }
            }
            let _ = fs::write(&report_path, &payload);
        }
    }
}

/// 扫描报告目录下的历史 JSON 报告文件列表（规范化路径去重 + 按 runId/运行签名去重）
pub fn list_reports(cfg: &AppConfig, base_dir: &Path, history_dir: &Path) -> Vec<ReportSummary> {
    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    if !cfg.report.trim().is_empty() {
        let rp = resolve_path(&cfg.report, base_dir);
        if let Some(parent) = rp.parent() {
            candidate_dirs.push(parent.to_path_buf());
        }
    }
    candidate_dirs.push(base_dir.join("test_reports"));
    candidate_dirs.push(base_dir.join("reports"));
    candidate_dirs.push(history_dir.to_path_buf());

    let mut seen_path_keys = std::collections::HashSet::<String>::new();
    let mut dedup_by_run = BTreeMap::<String, ReportSummary>::new();

    for dir in candidate_dirs {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if ext != "json" {
                    continue;
                }
                let norm_path = normalize_path_string(&p.to_string_lossy());
                let path_key = normalize_path_key(&norm_path);
                if !seen_path_keys.insert(path_key) {
                    continue;
                }

                if let Ok(text) = fs::read_to_string(&p) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(results) = val.get("results").and_then(|r| r.as_array()) {
                            let deduped = results
                                .iter()
                                .filter(|r| {
                                    r.get("outcome").and_then(|o| o.as_str()) == Some("deduped")
                                        || r.get("response")
                                            .and_then(|x| x.as_str())
                                            .map(|s| s.contains("\"deduped\":true"))
                                            .unwrap_or(false)
                                })
                                .count();
                            let run_id = val
                                .get("runId")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let file_name = p
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("report.json")
                                .to_string();
                            let started_at = val
                                .get("startedAt")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let finished_at = val
                                .get("finishedAt")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let target = val
                                .get("target")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();
                            let total =
                                val.get("total").and_then(|n| n.as_u64()).unwrap_or(0) as usize;

                            let summary = ReportSummary {
                                run_id: run_id.clone(),
                                file_name: file_name.clone(),
                                path: norm_path,
                                started_at: started_at.clone(),
                                finished_at: finished_at.clone(),
                                target: target.clone(),
                                total,
                                ok: val.get("ok").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
                                deduped,
                                unmatched: val
                                    .get("unmatched")
                                    .and_then(|n| n.as_u64())
                                    .unwrap_or(0)
                                    as usize,
                                failed: val.get("failed").and_then(|n| n.as_u64()).unwrap_or(0)
                                    as usize,
                                duration_ms: val
                                    .get("durationMs")
                                    .and_then(|n| n.as_u64())
                                    .unwrap_or(0),
                            };

                            // 同一次运行同时产生 run-*.json / single-*.json 与 latest.json 时，只保留时间戳归档那一份
                            let dedup_key = if !run_id.is_empty() {
                                format!("id:{}", run_id)
                            } else {
                                format!("sig:{}|{}|{}|{}", started_at, finished_at, target, total)
                            };
                            let is_archive_name = |name: &str| {
                                name.starts_with("run-") || name.starts_with("single-")
                            };
                            let replace_existing = match dedup_by_run.get(&dedup_key) {
                                None => true,
                                Some(existing) => {
                                    is_archive_name(&file_name)
                                        && !is_archive_name(&existing.file_name)
                                }
                            };
                            if replace_existing {
                                dedup_by_run.insert(dedup_key, summary);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut list: Vec<ReportSummary> = dedup_by_run.into_values().collect();
    list.sort_by(|a, b| {
        b.finished_at
            .cmp(&a.finished_at)
            .then_with(|| b.file_name.cmp(&a.file_name))
    });
    list
}

/// 删除单个历史报告 JSON 文件（若同目录 latest.json 对应同一次 runId 则一并清理，防止复活重复）
pub fn delete_report_file(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("报告文件路径不能为空".to_string());
    }
    let p = Path::new(trimmed);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" {
        return Err("安全限制：仅允许删除 .json 格式的历史报告文件".to_string());
    }
    if !p.is_file() {
        return Err(format!("目标报告文件不存在：{}", trimmed));
    }

    let deleted_run_id = fs::read_to_string(p)
        .ok()
        .and_then(|txt| serde_json::from_str::<serde_json::Value>(&txt).ok())
        .and_then(|val| val.get("runId").and_then(|s| s.as_str()).map(ToString::to_string))
        .unwrap_or_default();

    fs::remove_file(p).map_err(|e| format!("删除报告文件失败：{}", e))?;

    if !deleted_run_id.is_empty() {
        if let Some(parent) = p.parent() {
            let latest_path = parent.join("latest.json");
            if latest_path.is_file() {
                if let Ok(latest_txt) = fs::read_to_string(&latest_path) {
                    if let Ok(latest_val) = serde_json::from_str::<serde_json::Value>(&latest_txt) {
                        if latest_val.get("runId").and_then(|s| s.as_str())
                            == Some(deleted_run_id.as_str())
                        {
                            let _ = fs::remove_file(latest_path);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dummy_report(run_id: &str, finished_at: &str) -> FullRunReport {
        FullRunReport {
            run_id: run_id.to_string(),
            version: "1.2.0".to_string(),
            started_at: "2026-11-21T08:00:00Z".to_string(),
            finished_at: finished_at.to_string(),
            target: "https://n8n.example.com/webhook/demo".to_string(),
            auth: "basic".to_string(),
            strict: false,
            mark_test: true,
            total: 1,
            ok: 1,
            deduped: 0,
            unmatched: 0,
            failed: 0,
            parse_errors: 0,
            comments_skipped: 0,
            segments: 1,
            segment_wait_ms: 0,
            interactive: false,
            skipped_items: 0,
            skipped_segments: 0,
            duration_ms: 42,
            files: vec!["a.txt".to_string()],
            results: vec![RunItemResult {
                index: 1,
                segment: 1,
                source: "a.txt".to_string(),
                line: 1,
                label: "test | demo".to_string(),
                event_type: "test".to_string(),
                status: 200,
                outcome: "ok".to_string(),
                badge: "200 成功".to_string(),
                ok: true,
                unmatched: false,
                ms: 42,
                error: String::new(),
                response: r#"{"ok":true}"#.to_string(),
                final_url: "https://n8n.example.com/webhook/demo".to_string(),
                masked_headers: BTreeMap::new(),
                sent_body: "{}".to_string(),
            }],
        }
    }

    #[test]
    fn test_history_archive_and_deduplication() {
        let temp_dir =
            std::env::temp_dir().join(format!("hookreplay_runner_test_{}", Uuid::new_v4()));
        let history_dir = temp_dir.join("appdata_history");
        fs::create_dir_all(&temp_dir).unwrap();

        let mut cfg = AppConfig::default();
        // 故意使用 "./test_reports/latest.json" 触发旧版路径不规范的潜在重复场景
        cfg.report = "./test_reports/latest.json".to_string();

        let r1 = make_dummy_report("11111111-aaaa-bbbb-cccc-000000000001", "2026-11-21T08:01:00Z");
        let r2 = make_dummy_report("22222222-aaaa-bbbb-cccc-000000000002", "2026-11-21T08:02:00Z");

        persist_run_report(&r1, &cfg, &temp_dir, &history_dir);
        persist_run_report(&r2, &cfg, &temp_dir, &history_dir);

        // 应保存了 2 份历史归档且不因 latest.json 或 ./ 路径导致重复
        let reports = list_reports(&cfg, &temp_dir, &history_dir);
        assert_eq!(reports.len(), 2, "两次测试应保存两份互不覆盖且不重复的历史");
        assert_eq!(reports[0].run_id, r2.run_id);
        assert_eq!(reports[1].run_id, r1.run_id);
        assert!(reports[0].file_name.starts_with("run-"));
        assert!(reports[1].file_name.starts_with("run-"));

        // 删除最新一份（与 latest.json 同 runId）并确认不会由 latest.json 复活
        delete_report_file(&reports[0].path).unwrap();
        let after_del = list_reports(&cfg, &temp_dir, &history_dir);
        assert_eq!(after_del.len(), 1);
        assert_eq!(after_del[0].run_id, r1.run_id);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_single_replay_report_persistence_and_dedup() {
        let temp_dir =
            std::env::temp_dir().join(format!("hookreplay_single_test_{}", Uuid::new_v4()));
        let history_dir = temp_dir.join("appdata_history");
        fs::create_dir_all(&temp_dir).unwrap();

        let mut cfg = AppConfig::default();
        cfg.url = "https://n8n.example.com/webhook/demo".to_string();
        cfg.report = "./test_reports/latest.json".to_string();

        let entry = IndexedEntry {
            index: 3,
            segment: 2,
            source: "sample.jsonl".to_string(),
            source_name: "sample.jsonl".to_string(),
            line: 18,
            byte_offset: 0,
            byte_len: 12,
            inline_raw: None,
            label: "transfer | movie".to_string(),
            event_type: "transfer.complete".to_string(),
            title: "Demo Item".to_string(),
            comment: "测试分组".to_string(),
        };
        let item = RunItemResult {
            index: 3,
            segment: 2,
            source: "sample.jsonl".to_string(),
            line: 18,
            label: "transfer | movie".to_string(),
            event_type: "transfer.complete".to_string(),
            status: 200,
            outcome: "ok".to_string(),
            badge: "200 成功".to_string(),
            ok: true,
            unmatched: false,
            ms: 88,
            error: String::new(),
            response: r#"{"ok":true}"#.to_string(),
            final_url: cfg.url.clone(),
            masked_headers: BTreeMap::new(),
            sent_body: r#"{"type":"transfer.complete"}"#.to_string(),
        };

        let started_at = Utc::now();
        let report_obj = build_single_run_report(&item, &entry, &cfg, started_at, started_at, 88);
        persist_run_report_with_prefix(&report_obj, &cfg, &temp_dir, &history_dir, "single");

        let reports = list_reports(&cfg, &temp_dir, &history_dir);
        assert_eq!(reports.len(), 1, "单条发送应持久化 1 份报告且不与 latest.json 重复");
        assert!(reports[0].file_name.starts_with("single-"));
        assert_eq!(reports[0].total, 1);
        assert_eq!(reports[0].ok, 1);

        let raw_text = fs::read_to_string(&reports[0].path).unwrap();
        let full: FullRunReport = serde_json::from_str(&raw_text).unwrap();
        assert_eq!(full.total, 1);
        assert_eq!(full.results.len(), 1);
        assert_eq!(full.results[0].index, 3);

        delete_report_file(&reports[0].path).unwrap();
        let after_del = list_reports(&cfg, &temp_dir, &history_dir);
        assert!(after_del.is_empty(), "删除单条报告后应清理对应 latest.json");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
