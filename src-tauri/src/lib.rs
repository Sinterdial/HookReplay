//! Tauri v2 核心入口与全部 IPC Command 注册中心（Lib）
//!
//! 本文件负责：
//! 1. 将 Rust 后端模块（config / fonts / parser / mutator / matcher / runner）暴露给 Vue 3 前端调用；
//! 2. 维护应用级配置目录（`%APPDATA%/com.hookreplay.studio`）与历史归档目录（`history/`）；
//! 3. 安装全局异常捕获与按二进制路径隔离的 WebView2 初始化容错逻辑，彻底解决便携版闪退问题。

pub mod config;
pub mod fonts;
pub mod matcher;
pub mod mutator;
pub mod parser;
pub mod runner;

use config::{
    build_config_snapshot, builtin_config_templates, create_template_file, load_gui_state,
    normalize_path_key, normalize_path_string, parse_config_to_yaml_value, remove_mounted_profile,
    resolve_base_dir, resolve_path, save_gui_state, toggle_n8n_webhook_url,
    upsert_mounted_profile, AppConfig, ConfigSnapshot, ConfigTemplateInfo,
};
use fonts::{list_installed_fonts, SystemFontInfo};
use parser::{
    inspect_log_categories, list_dir_log_files, parse_scratchpad, persist_scratchpad_messages,
    read_entry_raw, scan_sources, IndexedEntry, PersistScratchpadRequest, PersistScratchpadResult,
    SourceScanResult,
};
use runner::{
    control_run, delete_report_file, list_reports, ping_endpoint, replay_single, start_batch_run,
    PingResult, ReportSummary, RunItemResult, RunManager, SegmentAction,
};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// 应用全局共享状态（存放 gui-config.json 路径、历史目录与并发运行管理器）
pub struct AppState {
    pub state_file: PathBuf,
    pub history_dir: PathBuf,
    pub run_manager: Arc<RunManager>,
}

/// 在 Windows 下弹出原生系统错误提示框（解决 Release 隐藏控制台后启动异常静默闪退问题）
fn show_native_error_dialog(title: &str, message: &str) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(
                hwnd: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                u_type: u32,
            ) -> i32;
        }
        let text_w: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        const MB_ICONERROR_AND_OK: u32 = 0x0000_0010;
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text_w.as_ptr(),
                title_w.as_ptr(),
                MB_ICONERROR_AND_OK,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("[{}] {}", title, message);
    }
}

/// 解析兜底的崩溃日志存放目录
fn fallback_app_data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        PathBuf::from(appdata).join("com.hookreplay.studio")
    } else {
        std::env::temp_dir().join("com.hookreplay.studio")
    }
}

/// 将启动异常或未捕获 Panic 写入 crash.log 便于定位排错
fn write_crash_log(app_dir: &Path, message: &str) {
    let _ = fs::create_dir_all(app_dir);
    let log_file = app_dir.join("crash.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_file) {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(f, "[{}] {}\n---", ts, message);
    }
}

/// 计算稳定 FNV-1a 64 位十六进制指纹（用于按可执行文件路径隔离 WebView2 数据目录）
fn fnv1a_64_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// 尽力清理已退出实例残留的 `webview2-inst-*` 临时目录（运行中实例因文件锁不会被误删）
fn cleanup_stale_webview_dirs(local_data_dir: &Path) {
    let Ok(entries) = fs::read_dir(local_data_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.starts_with("webview2-inst-") {
            let _ = fs::remove_dir_all(&p);
        }
    }
}

/// 构建主桌面窗口（独立 WebView2 data_directory 以支持便携版/安装版并行启动不抢锁）
fn build_main_window(
    app: &AppHandle,
    label: &str,
    data_dir: PathBuf,
) -> tauri::Result<tauri::WebviewWindow> {
    let _ = fs::create_dir_all(&data_dir);
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("HookReplay · Webhook 批量回放与调试工作台")
        .inner_size(1360.0, 860.0)
        .min_inner_size(1024.0, 680.0)
        .resizable(true)
        .center()
        .data_directory(data_dir)
        .build()
}

/// 读取当前配置快照（已按 YAML > GUI > 默认值 合并并附带逐字段来源）
#[tauri::command]
fn get_config_snapshot(state: State<AppState>) -> ConfigSnapshot {
    let gui_state = load_gui_state(&state.state_file);
    build_config_snapshot(&gui_state)
}

/// 切换当前绑定的 YAML 配置档案路径（传 None 切换为纯 GUI 配置模式）
#[tauri::command]
fn select_yaml_profile(
    state: State<AppState>,
    yaml_path: Option<String>,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    if let Some(ref p) = yaml_path {
        if !Path::new(p).is_file() {
            return Err(format!("所选配置文件不存在：{}", p));
        }
        let norm = upsert_mounted_profile(&mut gui_state, p, None);
        gui_state.active_yaml_path = Some(norm);
    } else {
        gui_state.active_yaml_path = None;
    }
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 挂载一个或多个自定义 YAML / JSONC 配置文件，可选立即激活首个文件
#[tauri::command]
fn mount_yaml_profiles(
    state: State<AppState>,
    paths: Vec<String>,
    activate_first: bool,
) -> Result<ConfigSnapshot, String> {
    if paths.is_empty() {
        return Err("请至少提供一个配置文件路径".to_string());
    }
    let mut gui_state = load_gui_state(&state.state_file);
    let mut first_norm: Option<String> = None;

    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let p = Path::new(trimmed);
        if !p.is_file() {
            return Err(format!("配置文件不存在：{}", trimmed));
        }
        let norm = upsert_mounted_profile(&mut gui_state, trimmed, None);
        if first_norm.is_none() {
            first_norm = Some(norm);
        }
    }

    if activate_first {
        if let Some(first) = first_norm {
            gui_state.active_yaml_path = Some(first);
        }
    }
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 卸载指定的已挂载配置文件（若当前正激活该档案则自动切回纯 GUI 模式）
#[tauri::command]
fn unmount_yaml_profile(
    state: State<AppState>,
    path: String,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    remove_mounted_profile(&mut gui_state, &path);
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 重命名已挂载配置文件的展示别名
#[tauri::command]
fn rename_yaml_profile(
    state: State<AppState>,
    path: String,
    alias: String,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    upsert_mounted_profile(&mut gui_state, &path, Some(&alias));
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 添加自定义配置扫描目录
#[tauri::command]
fn add_profile_dir(
    state: State<AppState>,
    dir_path: String,
) -> Result<ConfigSnapshot, String> {
    let trimmed = dir_path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).is_dir() {
        return Err(format!("目标配置目录不存在：{}", dir_path));
    }
    let norm = normalize_path_string(trimmed);
    let key = normalize_path_key(&norm);
    let mut gui_state = load_gui_state(&state.state_file);
    if !gui_state
        .custom_profile_dirs
        .iter()
        .any(|d| normalize_path_key(d) == key)
    {
        gui_state.custom_profile_dirs.push(norm);
    }
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 移除自定义配置扫描目录
#[tauri::command]
fn remove_profile_dir(
    state: State<AppState>,
    dir_path: String,
) -> Result<ConfigSnapshot, String> {
    let key = normalize_path_key(&dir_path);
    let mut gui_state = load_gui_state(&state.state_file);
    gui_state
        .custom_profile_dirs
        .retain(|d| normalize_path_key(d) != key);
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 获取内置配置模板列表
#[tauri::command]
fn list_config_templates() -> Vec<ConfigTemplateInfo> {
    builtin_config_templates()
}

/// 从内置模板生成 YAML 配置文件并按需挂载激活
#[tauri::command]
fn create_config_from_template(
    state: State<AppState>,
    template_id: String,
    target_path: String,
    mount_and_activate: bool,
) -> Result<ConfigSnapshot, String> {
    let created = create_template_file(&template_id, &target_path)?;
    let mut gui_state = load_gui_state(&state.state_file);
    if mount_and_activate {
        let created_str = created.to_string_lossy().to_string();
        let norm = upsert_mounted_profile(&mut gui_state, &created_str, None);
        gui_state.active_yaml_path = Some(norm);
        save_gui_state(&state.state_file, &gui_state)?;
    }
    Ok(build_config_snapshot(&gui_state))
}

/// 枚举本机已安装字体列表
#[tauri::command]
fn list_system_fonts() -> Vec<SystemFontInfo> {
    list_installed_fonts()
}

/// 保存外观字体、双字号与流光动效强度设置
#[tauri::command]
fn set_appearance_settings(
    state: State<AppState>,
    ui_font: String,
    mono_font: String,
    font_size: u8,
    code_font_size: Option<u8>,
    aurora_intensity: String,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    gui_state.ui_font = ui_font.trim().to_string();
    gui_state.mono_font = mono_font.trim().to_string();
    gui_state.font_size = font_size.clamp(11, 22);
    if let Some(csz) = code_font_size {
        gui_state.code_font_size = csz.clamp(10, 24);
    }
    gui_state.aurora_intensity = match aurora_intensity.as_str() {
        "rich" | "subtle" | "off" => aurora_intensity,
        _ => "rich".to_string(),
    };
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 标记首次安装/使用引导向导是否完成
#[tauri::command]
fn complete_onboarding(
    state: State<AppState>,
    completed: bool,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    gui_state.onboarding_completed = completed;
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 保存 GUI 兜底配置到 `%APPDATA%/com.hookreplay.studio/gui-config.json`
#[tauri::command]
fn save_gui_config(
    state: State<AppState>,
    gui_config: AppConfig,
) -> Result<ConfigSnapshot, String> {
    let mut gui_state = load_gui_state(&state.state_file);
    gui_state.gui_config = gui_config;
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 将指定配置结构直接序列化并写回当前 YAML（或另存为新的 YAML 文件）
#[tauri::command]
fn save_yaml_from_config(
    state: State<AppState>,
    target_path: String,
    config: AppConfig,
) -> Result<ConfigSnapshot, String> {
    // 若用户显式点「同步写回 YAML」，同时将顶层 expect/strict 保持与 assertions 一致
    let mut to_save = config;
    to_save.expect = to_save.assertions.expect.clone();
    to_save.strict = to_save.assertions.strict;

    let yaml_text = serde_yaml::to_string(&to_save)
        .map_err(|e| format!("序列化 YAML 失败：{}", e))?;
    let header_comment = "# HookReplay 配置档案（YAML 优先级高于 GUI 兜底配置）\n\n";
    let p = Path::new(&target_path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
        }
    }
    fs::write(p, format!("{}{}", header_comment, yaml_text))
        .map_err(|e| format!("写入 YAML 文件失败：{}", e))?;

    let mut gui_state = load_gui_state(&state.state_file);
    let norm = upsert_mounted_profile(&mut gui_state, &target_path, None);
    gui_state.active_yaml_path = Some(norm);
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 在线编辑并保存原始 YAML 源码（先校验语法再落盘）
#[tauri::command]
fn save_raw_yaml(
    state: State<AppState>,
    target_path: String,
    raw_yaml: String,
) -> Result<ConfigSnapshot, String> {
    let parsed = parse_config_to_yaml_value(&raw_yaml, Some(&target_path))?;
    // 验证关键结构可反序列化为 AppConfig，避免写入错误字段类型
    let _ = config::merge_configs(Some(&parsed), &AppConfig::default())?;

    let p = Path::new(&target_path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{}", e))?;
        }
    }
    fs::write(p, raw_yaml).map_err(|e| format!("保存 YAML 文件失败：{}", e))?;

    let mut gui_state = load_gui_state(&state.state_file);
    let norm = upsert_mounted_profile(&mut gui_state, &target_path, None);
    gui_state.active_yaml_path = Some(norm);
    save_gui_state(&state.state_file, &gui_state)?;
    Ok(build_config_snapshot(&gui_state))
}

/// 切换并持久化界面主题（system / light / dark）
#[tauri::command]
fn set_theme_mode(state: State<AppState>, theme: String) -> Result<(), String> {
    let mut gui_state = load_gui_state(&state.state_file);
    gui_state.theme = theme;
    save_gui_state(&state.state_file, &gui_state)
}

/// 一键切换 n8n URL 的正式/测试路径（仅返回新字符串，由前端决定只临时使用或保存）
#[tauri::command]
fn toggle_n8n_url(url: String) -> String {
    toggle_n8n_webhook_url(&url)
}

/// 扫描配置数据源并返回轻量分段与条目索引
#[tauri::command]
fn scan_input_sources(
    state: State<AppState>,
    config_override: Option<AppConfig>,
) -> Result<SourceScanResult, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    scan_sources(&cfg, &base_dir)
}

/// 解析临时粘贴调试台（Scratchpad）文本
#[tauri::command]
fn parse_scratchpad_text(
    state: State<AppState>,
    raw_text: String,
    config_override: Option<AppConfig>,
) -> Result<SourceScanResult, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    parse_scratchpad(&raw_text, &cfg)
}

/// 读取指定日志文件中已有的分段分类名称列表
#[tauri::command]
fn inspect_log_categories_cmd(
    state: State<AppState>,
    file_path: String,
    comment_prefixes: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    let resolved = resolve_path(&file_path, &base_dir);
    let prefixes = comment_prefixes.unwrap_or(snap.effective_config.comment_prefixes);
    inspect_log_categories(&resolved, &prefixes)
}

/// 列出指定测试目录下的可追加日志文件列表
#[tauri::command]
fn list_dir_log_files_cmd(
    state: State<AppState>,
    dir_path: String,
    extensions: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    let resolved = resolve_path(&dir_path, &base_dir);
    let exts = extensions.unwrap_or(snap.effective_config.extensions);
    list_dir_log_files(&resolved, &exts)
}

/// 将临时粘贴的原始消息归类持久化保存到指定的测试文件或测试目录
#[tauri::command]
fn persist_scratchpad_cmd(
    state: State<AppState>,
    request: PersistScratchpadRequest,
) -> Result<PersistScratchpadResult, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    persist_scratchpad_messages(
        &request,
        &snap.effective_config.comment_prefixes,
        &base_dir,
    )
}

/// 按偏移懒加载单条原始 JSON 字符串
#[tauri::command]
fn get_entry_raw(entry: IndexedEntry) -> Result<String, String> {
    read_entry_raw(&entry)
}

/// 发送一次测试连通性 Ping
#[tauri::command]
async fn test_endpoint_ping(
    state: State<'_, AppState>,
    override_url: Option<String>,
    config_override: Option<AppConfig>,
) -> Result<PingResult, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    Ok(ping_endpoint(override_url.as_deref(), &cfg).await)
}

/// 在线修改并重发单条消息（Inspector 调用，发送完成后自动保存为历史报告）
#[tauri::command]
async fn replay_single_item(
    state: State<'_, AppState>,
    raw_json: String,
    entry: IndexedEntry,
    override_url: Option<String>,
    config_override: Option<AppConfig>,
) -> Result<RunItemResult, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    replay_single(
        &raw_json,
        &entry,
        override_url.as_deref(),
        &cfg,
        &base_dir,
        &state.history_dir,
    )
    .await
}

/// 启动后台批量回放任务
#[tauri::command]
async fn start_run(
    app: AppHandle,
    state: State<'_, AppState>,
    entries: Vec<IndexedEntry>,
    config_override: Option<AppConfig>,
    parse_errors_count: usize,
    comments_skipped: usize,
    files: Vec<String>,
) -> Result<String, String> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    start_batch_run(
        app,
        state.run_manager.clone(),
        entries,
        cfg,
        snap.yaml_path,
        state.history_dir.clone(),
        parse_errors_count,
        comments_skipped,
        files,
    )
    .await
}

/// 控制正在运行的批量任务（pause / resume / abort / confirm）
#[tauri::command]
async fn control_active_run(
    state: State<'_, AppState>,
    action: String,
    segment_action: Option<SegmentAction>,
) -> Result<(), String> {
    control_run(&state.run_manager, &action, segment_action).await
}

/// 列出历史运行报告列表
#[tauri::command]
fn get_reports_list(
    state: State<AppState>,
    config_override: Option<AppConfig>,
) -> Vec<ReportSummary> {
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    list_reports(&cfg, &base_dir, &state.history_dir)
}

/// 删除单份历史运行报告并返回刷新后的列表
#[tauri::command]
fn delete_report(
    state: State<AppState>,
    path: String,
    config_override: Option<AppConfig>,
) -> Result<Vec<ReportSummary>, String> {
    delete_report_file(&path)?;
    let gui_state = load_gui_state(&state.state_file);
    let snap = build_config_snapshot(&gui_state);
    let cfg = config_override.unwrap_or(snap.effective_config);
    let base_dir = resolve_base_dir(snap.yaml_path.as_deref());
    Ok(list_reports(&cfg, &base_dir, &state.history_dir))
}

/// 读取指定历史报告的完整 JSON 内容
#[tauri::command]
fn read_report_detail(path: String) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("读取历史报告失败（{}）：{}", path, e))?;
    serde_json::from_str(&text).map_err(|e| format!("解析历史报告 JSON 失败：{}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 安装全局 Panic 捕获钩子，避免 Release 无控制台下静默闪退
    std::panic::set_hook(Box::new(|info| {
        let dir = fallback_app_data_dir();
        let msg = format!("HookReplay 运行出现未捕获异常：\n{}", info);
        write_crash_log(&dir, &msg);
        show_native_error_dialog("HookReplay 启动或运行异常", &msg);
    }));

    let run_res = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 将 GUI 配置与历史归档存放在系统 %APPDATA% 应用数据目录，保证安装到 C:\Program Files 也可正常写入
            let app_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| fallback_app_data_dir());
            let _ = fs::create_dir_all(&app_dir);
            let state_file = app_dir.join("gui-config.json");
            let history_dir = app_dir.join("history");
            let _ = fs::create_dir_all(&history_dir);

            app.manage(AppState {
                state_file,
                history_dir,
                run_manager: Arc::new(RunManager::default()),
            });

            // 为每个可执行文件路径分配独立 WebView2 数据目录，避免便携版与安装版争抢 EBWebView 文件锁（0x800700AA）
            let local_data_dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| app_dir.clone());
            cleanup_stale_webview_dirs(&local_data_dir);

            let exe_key = std::env::current_exe()
                .ok()
                .map(|p| normalize_path_key(&p.to_string_lossy()))
                .unwrap_or_else(|| "default".to_string());
            let primary_wv_dir =
                local_data_dir.join(format!("webview2-bin-{}", fnv1a_64_hex(&exe_key)));

            let handle = app.handle();
            if let Err(first_err) = build_main_window(handle, "main", primary_wv_dir.clone()) {
                // 若主目录被并发实例或提权进程占用，自动使用本次进程专属目录重试一次
                let pid = std::process::id();
                let millis = chrono::Utc::now().timestamp_millis();
                let fallback_wv_dir =
                    local_data_dir.join(format!("webview2-inst-{}-{}", pid, millis));
                if let Err(second_err) =
                    build_main_window(handle, "main-fallback", fallback_wv_dir.clone())
                {
                    let detail = format!(
                        "无法初始化桌面 WebView2 窗口。\n\n初次尝试失败：{}\n目录：{}\n\n回退尝试失败：{}\n目录：{}\n\n请确认本机已安装 Microsoft Edge WebView2 运行时。",
                        first_err,
                        primary_wv_dir.display(),
                        second_err,
                        fallback_wv_dir.display()
                    );
                    write_crash_log(&app_dir, &detail);
                    show_native_error_dialog("HookReplay 启动失败", &detail);
                    return Err(Box::new(second_err));
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config_snapshot,
            select_yaml_profile,
            mount_yaml_profiles,
            unmount_yaml_profile,
            rename_yaml_profile,
            add_profile_dir,
            remove_profile_dir,
            list_config_templates,
            create_config_from_template,
            list_system_fonts,
            set_appearance_settings,
            complete_onboarding,
            save_gui_config,
            save_yaml_from_config,
            save_raw_yaml,
            set_theme_mode,
            toggle_n8n_url,
            scan_input_sources,
            parse_scratchpad_text,
            inspect_log_categories_cmd,
            list_dir_log_files_cmd,
            persist_scratchpad_cmd,
            get_entry_raw,
            test_endpoint_ping,
            replay_single_item,
            start_run,
            control_active_run,
            get_reports_list,
            delete_report,
            read_report_detail,
        ])
        .run(tauri::generate_context!());

    if let Err(err) = run_res {
        let dir = fallback_app_data_dir();
        let msg = format!("启动 HookReplay 桌面运行时失败：{}", err);
        write_crash_log(&dir, &msg);
        show_native_error_dialog("HookReplay 启动失败", &msg);
    }
}
