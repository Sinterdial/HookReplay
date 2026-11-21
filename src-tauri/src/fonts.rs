//! 本机已安装字体枚举模块（System Fonts）
//!
//! 核心职责：
//! 1. 在 Windows 平台直接通过原生 `gdi32.dll` (`EnumFontFamiliesExW`) 枚举当前系统已安装的全部字体族，
//!    零额外第三方 crate 依赖；
//! 2. 过滤 `@` 开头的竖排字体重复条目，并标识等宽字体（`is_monospace`），方便用户分别挑选界面正文字体与 JSON 等宽字体；
//! 3. 非 Windows 平台提供常用系统字体兜底列表，确保跨平台单元测试或预览正常编译运行。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 单个已安装系统字体元数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SystemFontInfo {
    /// 字体族名称（如 "Microsoft YaHei UI" / "Cascadia Code"）
    pub name: String,
    /// 是否为等宽字体（FIXED_PITCH）
    pub is_monospace: bool,
}

/// 枚举当前操作系统已安装字体（去重并按名称排序）
pub fn list_installed_fonts() -> Vec<SystemFontInfo> {
    #[cfg(target_os = "windows")]
    {
        enumerate_windows_fonts()
    }
    #[cfg(not(target_os = "windows"))]
    {
        fallback_fonts()
    }
}

#[cfg(not(target_os = "windows"))]
fn fallback_fonts() -> Vec<SystemFontInfo> {
    [
        ("PingFang SC", false),
        ("Microsoft YaHei UI", false),
        ("Segoe UI", false),
        ("Noto Sans CJK SC", false),
        ("Cascadia Code", true),
        ("JetBrains Mono", true),
        ("Consolas", true),
        ("Fira Code", true),
    ]
    .into_iter()
    .map(|(name, is_monospace)| SystemFontInfo {
        name: name.to_string(),
        is_monospace,
    })
    .collect()
}

#[cfg(target_os = "windows")]
mod win_ffi {
    use std::ffi::c_void;

    pub const DEFAULT_CHARSET: u8 = 1;
    pub const FIXED_PITCH: u8 = 1;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct LogFontW {
        pub lf_height: i32,
        pub lf_width: i32,
        pub lf_escapement: i32,
        pub lf_orientation: i32,
        pub lf_weight: i32,
        pub lf_italic: u8,
        pub lf_underline: u8,
        pub lf_strike_out: u8,
        pub lf_char_set: u8,
        pub lf_out_precision: u8,
        pub lf_clip_precision: u8,
        pub lf_quality: u8,
        pub lf_pitch_and_family: u8,
        pub lf_face_name: [u16; 32],
    }

    impl Default for LogFontW {
        fn default() -> Self {
            Self {
                lf_height: 0,
                lf_width: 0,
                lf_escapement: 0,
                lf_orientation: 0,
                lf_weight: 0,
                lf_italic: 0,
                lf_underline: 0,
                lf_strike_out: 0,
                lf_char_set: DEFAULT_CHARSET,
                lf_out_precision: 0,
                lf_clip_precision: 0,
                lf_quality: 0,
                lf_pitch_and_family: 0,
                lf_face_name: [0; 32],
            }
        }
    }

    #[repr(C)]
    pub struct EnumLogFontExW {
        pub elf_log_font: LogFontW,
        pub elf_full_name: [u16; 64],
        pub elf_style: [u16; 32],
        pub elf_script: [u16; 32],
    }

    pub type FontEnumExProcW = unsafe extern "system" fn(
        lpelfe: *const EnumLogFontExW,
        lpntme: *const c_void,
        font_type: u32,
        lparam: isize,
    ) -> i32;

    #[link(name = "user32")]
    extern "system" {
        pub fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        pub fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        pub fn EnumFontFamiliesExW(
            hdc: *mut c_void,
            lp_logfont: *const LogFontW,
            lp_proc: FontEnumExProcW,
            l_param: isize,
            dw_flags: u32,
        ) -> i32;
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_font_callback(
    lpelfe: *const win_ffi::EnumLogFontExW,
    _lpntme: *const std::ffi::c_void,
    _font_type: u32,
    lparam: isize,
) -> i32 {
    if lpelfe.is_null() || lparam == 0 {
        return 1;
    }
    let map = &mut *(lparam as *mut BTreeMap<String, bool>);
    let lf = &(*lpelfe).elf_log_font;

    // 截断 NUL 终止符并还原 UTF-16 字体族名称
    let face_slice = &lf.lf_face_name;
    let len = face_slice
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(face_slice.len());
    let name = String::from_utf16_lossy(&face_slice[..len]).trim().to_string();

    // 跳过空名及 `@` 开头的 Windows 竖排旋转字体别名
    if name.is_empty() || name.starts_with('@') {
        return 1;
    }

    let is_mono = (lf.lf_pitch_and_family & 0x03) == win_ffi::FIXED_PITCH;
    let entry = map.entry(name).or_insert(false);
    if is_mono {
        *entry = true;
    }
    1
}

#[cfg(target_os = "windows")]
fn enumerate_windows_fonts() -> Vec<SystemFontInfo> {
    let mut map: BTreeMap<String, bool> = BTreeMap::new();
    unsafe {
        let hdc = win_ffi::GetDC(std::ptr::null_mut());
        if !hdc.is_null() {
            let lf = win_ffi::LogFontW::default();
            let lparam = (&mut map as *mut BTreeMap<String, bool>) as isize;
            win_ffi::EnumFontFamiliesExW(hdc, &lf, enum_font_callback, lparam, 0);
            win_ffi::ReleaseDC(std::ptr::null_mut(), hdc);
        }
    }

    let mut list: Vec<SystemFontInfo> = map
        .into_iter()
        .map(|(name, is_monospace)| SystemFontInfo { name, is_monospace })
        .collect();

    list.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_system_fonts_non_empty() {
        let fonts = list_installed_fonts();
        assert!(!fonts.is_empty(), "应能枚举到系统已安装字体");
        assert!(
            fonts.iter().all(|f| !f.name.starts_with('@')),
            "不应包含 @ 开头的竖排字体别名"
        );
        #[cfg(target_os = "windows")]
        {
            let has_common = fonts.iter().any(|f| {
                f.name.eq_ignore_ascii_case("Segoe UI")
                    || f.name.eq_ignore_ascii_case("Consolas")
                    || f.name.eq_ignore_ascii_case("Microsoft YaHei UI")
            });
            assert!(has_common, "Windows 系统字体中应包含常见标准字体");
        }
    }
}
