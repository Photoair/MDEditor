//! mdedit — 轻量 Markdown 编辑器（Windows 桌面 GUI）
//!
//! 左侧：Markdown 编辑区（语法高亮）
//! 右侧：实时预览
//! 菜单：新建 / 打开 / 保存 / 另存为 / 导出 HTML
//! 说明：标记为 Windows GUI 子系统，运行时不创建任何控制台/PowerShell 窗口。

#![windows_subsystem = "windows"]

// 编辑区语法高亮模块（通过工具栏「高亮」开关启用）
mod highlight;
mod wysiwyg;

use eframe::egui;
use egui::{Color32, RichText, TopBottomPanel, CentralPanel, ScrollArea};
use std::path::PathBuf;

/// 把当前程序注册为 .md / .markdown 的默认打开方式（写 HKCU 注册表，免管理员权限）。
/// 成功返回 true；已在注册表中则返回 true。
fn register_file_assoc() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => return false,
    };
    let quoted = format!("\"{}\" \"%1\"", exe);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let ok = (|| -> std::io::Result<()> {
        // .md
        let (md, _) = hkcu.create_subkey(r"Software\Classes\.md")?;
        md.set_value("", &"mdedit.md")?;
        // .markdown
        let (mk, _) = hkcu.create_subkey(r"Software\Classes\.markdown")?;
        mk.set_value("", &"mdedit.md")?;
        // 协议 command
        let (cmd, _) = hkcu.create_subkey(r"Software\Classes\mdedit.md\shell\open\command")?;
        cmd.set_value("", &quoted)?;
        let (ft, _) = hkcu.create_subkey(r"Software\Classes\mdedit.md\DefaultIcon")?;
        ft.set_value("", &format!("{},0", exe))?;
        Ok(())
    })();
    ok.is_ok()
}

/// 当前程序版本（用于更新检测，与服务器 JSON 的 version 比较）
const APP_VERSION: &str = "1.0.0";
/// 更新检查接口：返回 JSON，约定字段 version / download_url / note
const UPDATE_CHECK_URL: &str = "https://www.yuanyuzhou.me/aderp/mdeditlatest.json";

/// 从 JSON 里取字符串字段值（"key":"value" 或 "key":value），简单解析。
fn json_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let i = s.find(&pat)? + pat.len();
    let rest = s[i..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if let Some(body) = rest.strip_prefix('"') {
        let end = body.find('"')?;
        return Some(body[..end].to_string());
    }
    let end = rest.find([',', '}', ']']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

/// 后台拉取最新版本信息，返回 (version, download_url, note)；失败返回 None。
/// 用系统自带 curl.exe 拉取，并加 CREATE_NO_WINDOW 防止弹出控制台黑框。
#[cfg(windows)]
fn fetch_update_info() -> Option<(String, String, String)> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("curl.exe")
        .args(["-s", "--max-time", "8", UPDATE_CHECK_URL])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let version = json_field(&s, "version")?;
    let url = json_field(&s, "download_url").or_else(|| json_field(&s, "url"))?;
    let note = json_field(&s, "note").unwrap_or_default();
    Some((version, url, note))
}

#[cfg(not(windows))]
fn fetch_update_info() -> Option<(String, String, String)> {
    let out = std::process::Command::new("curl")
        .args(["-s", "--max-time", "8", UPDATE_CHECK_URL])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let version = json_field(&s, "version")?;
    let url = json_field(&s, "download_url").or_else(|| json_field(&s, "url"))?;
    let note = json_field(&s, "note").unwrap_or_default();
    Some((version, url, note))
}

/// 版本号比较：remote > local 则为 true（按点分段数值比较）。
fn version_newer(remote: &str, local: &str) -> bool {
    let r: Vec<u32> = remote.split('.').filter_map(|s| s.trim().parse().ok()).collect();
    let l: Vec<u32> = local.split('.').filter_map(|s| s.trim().parse().ok()).collect();
    for i in 0..r.len().max(l.len()) {
        let a = r.get(i).copied().unwrap_or(0);
        let b = l.get(i).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// 用系统默认浏览器打开链接（下载/官网）。用 rundll32 交给默认浏览器，避免被当成本地路径。
fn open_url(url: &str) {    let _ = std::process::Command::new("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
}

/// 语言持久化文件：%APPDATA%\MDEditor\lang.txt（记录中英文选择，下次启动自动恢复）。
fn lang_file_path() -> std::path::PathBuf {
    let mut p = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    p.push("MDEditor");
    p.push("lang.txt");
    p
}

fn load_lang() -> Lang {
    if let Ok(s) = std::fs::read_to_string(lang_file_path()) {
        if s.trim().eq_ignore_ascii_case("en") {
            return Lang::En;
        }
    }
    Lang::Zh
}

fn save_lang(l: Lang) {
    if let Some(dir) = lang_file_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(lang_file_path(), if l == Lang::En { "en" } else { "zh" });
}

/// 启动本地回环小服务器（127.0.0.1 随机端口），接收网页自动同步回的 Markdown。
/// 返回 (端口, 接收 channel)。监听线程随进程退出而终止。
fn start_sync_server() -> Option<(u16, std::sync::mpsc::Receiver<String>)> {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut data = Vec::new();
            let mut buf = [0u8; 8192];
            let mut header_end: Option<usize> = None;
            // 读到请求头结束
            loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                data.extend_from_slice(&buf[..n]);
                if let Some(p) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = Some(p);
                    break;
                }
                if data.len() > 65536 {
                    break;
                }
            }
            let Some(pos) = header_end else {
                let _ = stream.shutdown(Shutdown::Both);
                continue;
            };
            let header = String::from_utf8_lossy(&data[..pos]);
            let is_post = header.starts_with("POST");
            // Content-Length
            let clen = header
                .lines()
                .find_map(|l| {
                    let ll = l.to_lowercase();
                    ll.strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            while data.len() < pos + 4 + clen {
                let n = match stream.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };
                data.extend_from_slice(&buf[..n]);
            }
            let body = &data[pos + 4..];
            let body_str = String::from_utf8_lossy(body).to_string();
            let resp_body = if is_post { "ok" } else { "mdedit-sync" };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST, GET, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.shutdown(Shutdown::Both);
            if is_post {
                let _ = tx.send(body_str);
            }
        }
    });
    Some((port, rx))
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MDEditor")
            .with_inner_size([1150.0, 760.0])
            .with_min_inner_size([700.0, 480.0]),
        ..Default::default()
    };
    let initial_file = std::env::args().nth(1);
    eframe::run_native(
        "mdedit",
        options,
        Box::new(move |cc| {
            // 纯白背景 + 黑色文字：完全参考记事本，最清晰、无黑框
            let mut visuals = egui::Visuals::light();
            visuals.panel_fill = egui::Color32::from_rgb(255, 255, 255);
            visuals.window_fill = egui::Color32::from_rgb(255, 255, 255);
            cc.egui_ctx.set_visuals(visuals);
            install_cjk_font(&cc.egui_ctx);
            Ok(Box::new(MdEditorApp::new(initial_file)))
        }),
    )
}

/// 加载系统中文字体（微软雅黑/宋体/黑体等）作为 UI 兜底字体，
/// 并注册为可选字体（family 名 = key），供「选择字体」使用。
fn install_cjk_font(ctx: &egui::Context) {
    // key(用于 FontFamily::Name) -> 字体文件
    let candidates: [(&str, &str); 6] = [
        ("yahei", r"C:\Windows\Fonts\msyh.ttc"),     // 微软雅黑（记事本风格，最清晰）
        ("msyhbd", r"C:\Windows\Fonts\msyhbd.ttc"),  // 微软雅黑粗体
        ("simhei", r"C:\Windows\Fonts\simhei.ttf"),  // 黑体
        ("dengxian", r"C:\Windows\Fonts\Deng.ttf"),  // 等线
        ("simsun", r"C:\Windows\Fonts\simsun.ttc"),  // 宋体
        ("kaiti", r"C:\Windows\Fonts\simkai.ttf"),   // 楷体
    ];
    let mut fonts = egui::FontDefinitions::default();
    for (key, p) in &candidates {
        if let Ok(bytes) = std::fs::read(p) {
            fonts
                .font_data
                .insert(key.to_string(), egui::FontData::from_owned(bytes));
            // 同时作为 Proportional/Monospace 的兜底，保证 UI 中文不乱码
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(key.to_string());
            }
            // 注册为独立可选的 FontFamily（供「选择字体」用 FontFamily::Name 引用）
            fonts.families.insert(
                egui::FontFamily::Name(std::sync::Arc::from(key.to_string())),
                vec![key.to_string()],
            );
        }
    }
    ctx.set_fonts(fonts);
}

/// 可选择的字体（key, 显示名）。key 与 install_cjk_font 注册的 family 对应。
const FONT_CHOICES: &[(&str, &str)] = &[
    ("yahei", "微软雅黑"),
    ("msyhbd", "微软雅黑 粗体"),
    ("simhei", "黑体"),
    ("dengxian", "等线"),
    ("simsun", "宋体"),
    ("kaiti", "楷体"),
    ("mono", "等宽"),
];

/// 由字体 key 取显示名。
fn font_display_name(key: &str) -> String {
    FONT_CHOICES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, d)| d.to_string())
        .unwrap_or_else(|| key.to_string())
}

/// 主应用状态
struct MdEditorApp {
    buffer: String,
    current_file: Option<PathBuf>,
    dirty: bool,
    status: String,
    show_help: bool,
    /// 最近一次渲染时编辑区的光标 char index（用于格式插入）
    cursor_char: usize,
    /// 语法高亮开关（默认关闭：关闭时光标 100% 精确；开启时启用高亮 layouter，
    /// 但自定义 layouter 在 egui 下可能带来轻微光标偏差）
    highlight_enabled: bool,
    /// 编辑区字体（family key，对应 FONT_CHOICES）与字号
    font_name: String,
    font_size: f32,
    /// 查找 / 替换状态
    show_find: bool,
    find_text: String,
    replace_text: String,
    /// 更新检测
    update_rx: Option<std::sync::mpsc::Receiver<Option<(String, String, String)>>>,
    update_available: bool,
    update_version: String,
    update_url: String,
    update_note: String,
    /// 关于弹窗
    show_about: bool,
    /// 启动后是否已执行过一次窗口居中
    centered_once: bool,
    /// 待处理的快捷键格式操作（延迟到帧内消费，避免在输入回调里弹对话框）
    pending_format: Option<Format>,
    pending_font_delta: Option<f32>,
    /// F5 触发的网页预览（延迟到帧内执行）
    pending_preview: bool,
    /// 界面语言
    lang: Lang,
    /// 所见即所得（编辑时右侧实时渲染预览）
    wysiwyg: bool,
    /// 网页自动同步回编辑器：本地回环小服务器
    sync_rx: Option<std::sync::mpsc::Receiver<String>>,
    sync_port: u16,
    sync_started: bool,
}

/// 格式操作（对应参考版工具栏）
#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Zh,
    En,
}

#[derive(Clone, Copy)]
enum Format {
    Heading,
    Heading2,
    Heading3,
    Bold,
    Italic,
    Strike,
    Link,
    Image,
    InlineCode,
    CodeBlock,
    UnorderedList,
    OrderedList,
    TaskList,
    Quote,
    Table,
    Divider,
}

impl MdEditorApp {
    fn new(initial_file: Option<String>) -> Self {
        let mut app = Self {
            buffer: String::new(),
            current_file: None,
            dirty: false,
            status: "就绪".to_string(),
            show_help: false,
            cursor_char: 0,
            highlight_enabled: false,
            font_name: "yahei".to_string(),
            font_size: 15.0,
            show_find: false,
            find_text: String::new(),
            replace_text: String::new(),
            update_rx: None,
            update_available: false,
            update_version: String::new(),
            update_url: String::new(),
            update_note: String::new(),
            show_about: false,
            centered_once: false,
            pending_format: None,
            pending_font_delta: None,
            pending_preview: false,
            lang: load_lang(),
            wysiwyg: false,
            sync_rx: None,
            sync_port: 0,
            sync_started: false,
        };
        // 启动时后台检测更新（不阻塞 UI；服务器无 JSON 或网络失败则静默）
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(fetch_update_info());
        });
        app.update_rx = Some(rx);
        if let Some(path) = initial_file {
            let pb = PathBuf::from(path);
            if let Some(content) = std::fs::read_to_string(&pb).ok() {
                app.buffer = content;
                app.current_file = Some(pb);
                app.status = "已打开文件".to_string();
            }
        } else if let Some(draft) = load_draft() {
            app.buffer = draft;
            app.status = "已恢复上次的草稿".to_string();
        } else {
            app.buffer = SAMPLE_MD.to_string();
        }
        app
    }

    fn open_path(&mut self, path: &PathBuf) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                self.buffer = content;
                self.current_file = Some(path.clone());
                self.dirty = false;
                self.status = format!("已打开: {}", path.display());
            }
            Err(e) => {
                self.status = format!("打开失败: {}", e);
            }
        }
    }

    fn save_to(&mut self, path: &PathBuf) -> bool {
        match std::fs::write(path, &self.buffer) {
            Ok(_) => {
                self.current_file = Some(path.clone());
                self.dirty = false;
                self.status = format!("已保存: {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("保存失败: {}", e);
                false
            }
        }
    }

    fn export_html(&mut self, path: &PathBuf) -> bool {
        // 完全参考网页版 MD 编辑器：导出结果与网页预览一致（内嵌同一渲染引擎与样式）
        let html = render_html_standalone_str(&self.buffer);

        match std::fs::write(path, &html) {
            Ok(_) => {
                self.status = format!("已导出 HTML: {}", path.display());
                true
            }
            Err(e) => {
                self.status = format!("导出失败: {}", e);
                false
            }
        }
    }

    // ---- 文件对话框（rfd 原生）----
    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter("文本文件", &["txt", "text"])
            .add_filter("HTML", &["html", "htm"])
            .add_filter("所有文件", &["*"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    fn save_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .add_filter("文本文件", &["txt"])
            .add_filter("所有文件", &["*"]);
        if let Some(cur) = &self.current_file {
            if let Some(name) = cur.file_name().and_then(|n| n.to_str()) {
                dlg = dlg.set_file_name(name);
            }
        }
        if let Some(path) = dlg.save_file() {
            let mut path = path;
            // 若扩展名不属于支持的纯文本类型，则自动补 .md
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            let supported = matches!(
                ext.as_deref(),
                Some("md") | Some("markdown") | Some("txt") | Some("text")
            );
            if !supported {
                path.set_extension("md");
            }
            self.save_to(&path);
        }
    }

    fn export_dialog(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("HTML", &["html"]);
        let base = self
            .current_file
            .as_ref()
            .map(|p| {
                let mut s = p.to_string_lossy().to_string();
                if s.to_lowercase().ends_with(".md") {
                    s.truncate(s.len() - 3);
                }
                s
            })
            .unwrap_or_else(|| "output".to_string());
        dlg = dlg.set_file_name(format!("{}.html", base));
        if let Some(path) = dlg.save_file() {
            let mut path = path;
            if !path
                .extension()
                .map(|e| e.to_str().map(|s| s == "html").unwrap_or(false))
                .unwrap_or(false)
            {
                path.set_extension("html");
            }
            self.export_html(&path);
        }
    }

    fn save_command(&mut self) {
        if let Some(path) = self.current_file.clone() {
            self.save_to(&path);
        } else {
            self.save_dialog();
        }
    }

    /// 根据光标 char index 在缓冲区插入 Markdown 格式（参考版工具栏）。
    /// 从系统剪贴板读取文本并插入到光标处（把网页导出的 Markdown 粘贴回来）。
    fn paste_from_clipboard(&mut self) {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-Command",
                "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-Clipboard -Raw",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok();
        let text = out.and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout);
                Some(s.trim_end().to_string())
            } else {
                None
            }
        });
        match text {
            Some(s) if !s.is_empty() => {
                let byte = char_to_byte(&self.buffer, self.cursor_char.min(chars_count(&self.buffer)));
                self.buffer.insert_str(byte, &s);
                self.cursor_char += chars_count(&s);
                self.dirty = true;
                self.status = "已粘贴剪贴板内容".to_string();
            }
            _ => self.status = "剪贴板为空或读取失败".to_string(),
        }
    }

    fn apply_format(&mut self, f: Format) {
        // 图片需要先选择本地文件，单独处理
        if let Format::Image = f {
            self.insert_image();
            return;
        }
        let byte = char_to_byte(&self.buffer, self.cursor_char.min(chars_count(&self.buffer)));
        let (before, after) = match f {
            Format::Heading => ("# ".to_string(), String::new()),
            Format::Heading2 => ("## ".to_string(), String::new()),
            Format::Heading3 => ("### ".to_string(), String::new()),
            Format::Bold => ("**".to_string(), "**".to_string()),
            Format::Italic => ("*".to_string(), "*".to_string()),
            Format::Strike => ("~~".to_string(), "~~".to_string()),
            Format::Link => ("[".to_string(), "](https://)".to_string()),
            Format::Image => (String::new(), String::new()),
            Format::InlineCode => ("`".to_string(), "`".to_string()),
            Format::CodeBlock => ("```\n".to_string(), "\n```".to_string()),
            Format::UnorderedList => ("- ".to_string(), "\n- ".to_string()),
            Format::OrderedList => ("1. ".to_string(), "\n1. ".to_string()),
            Format::TaskList => ("- [ ] ".to_string(), "\n- [ ] ".to_string()),
            Format::Quote => ("> ".to_string(), "\n> ".to_string()),
            Format::Table => {
                ("| 列1 | 列2 |\n| --- | --- |\n| 内容 | 内容 |".to_string(), String::new())
            }
            Format::Divider => ("\n---\n".to_string(), String::new()),
        };
        self.buffer.insert_str(byte, &before);
        // after 插入到内容之后（若无选定则紧随 before）
        let after_pos = byte + before.len();
        self.buffer.insert_str(after_pos, &after);
        self.cursor_char = chars_count(&self.buffer[..after_pos]);
        self.dirty = true;
        self.status = "编辑中...".to_string();
    }

    /// 选择本地图片并插入 `![描述](路径)`。图片与文档同目录时用相对文件名，
    /// 否则用完整路径（反斜杠转正斜杠），便于网页预览加载。
    fn insert_image(&mut self) {        if let Some(path) = rfd::FileDialog::new()
            .add_filter("图片", &["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"])
            .pick_file()
        {
            self.insert_image_path(&path);
        }
    }

    /// 把一张本地图片以 `![描述](路径)\n` 形式插入光标处，并在图片后自动补一行，
    /// 光标停在下一行行首，便于继续输入。
    fn insert_image_path(&mut self, path: &std::path::Path) {
        // 与当前文档同目录 → 用相对文件名，否则用完整路径
        let rel = self.current_file.as_ref().and_then(|cur| {
            let img_dir = path.parent()?;
            let doc_dir = cur.parent()?;
            if img_dir == doc_dir {
                path.file_name()?.to_str().map(|n| n.to_string())
            } else {
                None
            }
        });
        let reference = rel.unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
        let alt = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("图片")
            .to_string();
        // 末尾补一个换行：自动换到下一行，光标停在下一行行首
        let md = format!("![{}]({})\n", alt, reference);
        let byte = char_to_byte(&self.buffer, self.cursor_char.min(chars_count(&self.buffer)));
        self.buffer.insert_str(byte, &md);
        self.cursor_char = chars_count(&self.buffer[..byte + md.len()]);
        self.dirty = true;
        self.status = "已插入图片".to_string();
    }

    /// 处理从资源管理器拖入的文件：图片自动插入 `![描述](路径)`。
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let mut inserted = 0usize;
        for f in dropped {
            if let Some(path) = f.path {
                if is_image_path(&path) {
                    self.insert_image_path(&path);
                    inserted += 1;
                }
            }
        }
        if inserted > 0 {
            self.dirty = true;
            self.status = format!("已通过拖拽插入 {} 张图片", inserted);
        }
    }

    /// 编辑区字体 FontId：由「选择字体」+「字号」决定。
    fn editor_font_id(&self) -> egui::FontId {
        let fam = match self.font_name.as_str() {
            "mono" => egui::FontFamily::Monospace,
            n => egui::FontFamily::Name(std::sync::Arc::from(n.to_string())),
        };
        egui::FontId::new(self.font_size, fam)
    }

    /// 查找下一个：从当前光标向后查找，选中匹配并移动光标。
    fn find_next(&mut self, ctx: &egui::Context) {
        if self.find_text.is_empty() {
            self.status = "请输入查找内容".to_string();
            return;
        }
        let from = self.cursor_char.min(chars_count(&self.buffer));
        let found = {
            let rest = &self.buffer[char_to_byte(&self.buffer, from)..];
            rest.find(&self.find_text).map(|rel| {
                let abs = from + chars_count(&rest[..rel]);
                let end = abs + chars_count(&self.find_text);
                (abs, end)
            })
        };
        match found {
            Some((abs, end)) => {
                if let Some(mut st) = egui::widgets::text_edit::TextEditState::load(
                    ctx,
                    egui::Id::new("md_edit"),
                ) {
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(abs),
                    )));
                }
                self.cursor_char = end;
                self.status = format!("找到：第 {} 字符", abs + 1);
            }
            None => self.status = "未找到匹配".to_string(),
        }
    }

    /// 替换当前（光标处起）第一个匹配；若无则先查找。
    fn replace_one(&mut self, ctx: &egui::Context) {
        if self.find_text.is_empty() {
            self.status = "请输入查找内容".to_string();
            return;
        }
        let from = self.cursor_char.min(chars_count(&self.buffer));
        let target = {
            let rest = &self.buffer[char_to_byte(&self.buffer, from)..];
            rest.find(&self.find_text)
                .map(|rel| from + chars_count(&rest[..rel]))
        };
        if let Some(abs) = target {
            let byte = char_to_byte(&self.buffer, abs);
            let byte_end = byte + self.find_text.len();
            self.buffer.replace_range(byte..byte_end, &self.replace_text);
            self.cursor_char = abs + chars_count(&self.replace_text);
            self.dirty = true;
            self.status = "已替换 1 处".to_string();
        } else {
            self.find_next(ctx);
        }
    }

    /// 全部替换。
    fn replace_all(&mut self) {
        if self.find_text.is_empty() {
            self.status = "请输入查找内容".to_string();
            return;
        }
        let count = self.buffer.matches(&self.find_text).count();
        if count > 0 {
            self.buffer = self.buffer.replace(&self.find_text, &self.replace_text);
            self.dirty = true;
            self.status = format!("已全部替换 {} 处", count);
        } else {
            self.status = "未找到匹配".to_string();
        }
    }

    /// 打印：把渲染结果写成 HTML 并在浏览器中打开并触发系统打印对话框。
    fn print_preview(&mut self) {
        let mut html = render_html_standalone_str(&self.buffer);
        // 页面加载后自动弹出系统打印对话框
        html = html.replacen(
            "</body>",
            "<script>window.onload=function(){setTimeout(function(){window.print();},350);};</script></body>",
            1,
        );
        let path = std::env::current_exe()
            .map(|p| p.with_file_name("mdedit_print.html"))
            .unwrap_or_else(|_| std::path::PathBuf::from("mdedit_print.html"));
        match std::fs::write(&path, html) {
            Ok(()) => {
                let _ = std::process::Command::new("explorer").arg(&path).spawn();
                self.status = "已打开打印预览（请在浏览器中选择打印机）".to_string();
            }
            Err(e) => self.status = format!("打印失败：{}", e),
        }
    }

    /// 生成完整独立 HTML 字符串（含样式），用于复制到剪贴板。
    fn render_html_standalone(&self) -> String {
        render_html_standalone_str(&self.buffer)
    }

    /// 格式工具栏按钮：点击即对光标处插入对应格式。
    fn tool_btn(&mut self, ui: &mut egui::Ui, f: Format, label: &str, tip: &str) {
        if ui.button(RichText::new(label).size(14.0)).on_hover_text(tip).clicked() {
            self.apply_format(f);
        }
    }

    /// 网页预览：把当前内容渲染成完整 HTML，写入临时文件，用系统默认浏览器打开。
    /// 确保本地回环同步服务器已启动，返回监听端口（失败返回 0）。
    fn ensure_sync_server(&mut self) -> u16 {
        if !self.sync_started {
            self.sync_started = true;
            if let Some((port, rx)) = start_sync_server() {
                self.sync_port = port;
                self.sync_rx = Some(rx);
            }
        }
        self.sync_port
    }

    fn preview_in_browser(&mut self) {
        // 网页预览使用可编辑版（所见即所得），并启动本地回环服务器实现自动同步
        let port = self.ensure_sync_server();
        let html = render_html_editable_str(&self.buffer, port);
        // 写到程序同目录下的 mdedit_preview.html（带时间戳避免缓存）
        let path = std::env::current_exe()
            .map(|p| p.with_file_name("mdedit_preview.html"))
            .unwrap_or_else(|_| std::path::PathBuf::from("mdedit_preview.html"));
        match std::fs::write(&path, html) {
            Ok(()) => {
                // 用 explorer 交给系统默认浏览器打开（GUI 程序，无控制台弹窗）
                let _ = std::process::Command::new("explorer")
                    .arg(&path)
                    .spawn();
                self.status = "已在浏览器中打开预览".to_string();
            }
            Err(e) => {
                self.status = format!("预览失败：{}", e);
            }
        }
    }
}

/// 把 char 索引转成字节偏移（UTF-8）。
fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

/// 统计字符串字符数（不含换行忽略，纯字符数）。
fn chars_count(s: &str) -> usize {
    s.chars().count()
}

/// 统计"字数"（去掉空白后的字符数，参考版口径）。
fn word_count(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// 判断文件是否为受支持的图片格式（用于工具栏/拖拽插入）。
fn is_image_path(p: &std::path::Path) -> bool {
    matches!(
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg")
    )
}

/// 草稿文件路径：程序所在目录下的 mdedit_draft.md（自动保存/恢复）。
fn draft_path() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("mdedit_draft.md"))
}

/// 读取上次自动保存的草稿。
fn load_draft() -> Option<String> {
    let p = draft_path()?;
    std::fs::read_to_string(&p).ok()
}

/// 自动保存当前内容到草稿文件（仅当有内容变化时）。
fn save_draft(content: &str) {
    if let Some(p) = draft_path() {
        let _ = std::fs::write(p, content);
    }
}

/// 根据字节偏移推算行列号（第几行、第几列，从 1 开始）。
fn line_col(s: &str, byte: usize) -> (usize, usize) {
    let prefix = &s[..byte.min(s.len())];
    let mut line = 1usize;
    let mut col = 1usize;
    for c in prefix.chars() {
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// 生成完整独立 HTML 字符串，用于复制/网页预览。
/// 完全参考网页版 MD 编辑器：内嵌其 md()/inline() 解析器与 #preview 样式，
/// 由浏览器用完全相同的逻辑渲染，保证表格/链接/图片/列表等效果完全一致。
fn render_html_standalone_str(md: &str) -> String {
    render_html_str(md, false, 0)
}

/// 可编辑网页版（所见即所得）：内容区可编辑 + 顶部编辑工具条 + 自动同步回编辑器。
/// port > 0 时开启网页→编辑器自动同步。
fn render_html_editable_str(md: &str, port: u16) -> String {
    render_html_str(md, true, port)
}

fn render_html_str(md: &str, editable: bool, port: u16) -> String {
    let mut html = String::from(
        "<!DOCTYPE html>\n<html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
<title>Markdown</title><style>\
body{max-width:860px;margin:30px auto;background:#fbfaf6;color:#333;font:15px/1.8 -apple-system,Segoe UI,Microsoft YaHei,sans-serif;padding:0 22px}\
#mdtbar{position:sticky;top:0;z-index:10;display:flex;flex-wrap:wrap;gap:5px;align-items:center;background:#fff;border:1px solid #e5e7eb;border-radius:8px;padding:6px 8px;margin-bottom:16px}\
#mdtbar button{border:1px solid #e5e7eb;background:#fff;color:#333;border-radius:5px;padding:4px 10px;font:13px -apple-system,Segoe UI,Microsoft YaHei,sans-serif;cursor:pointer}\
#mdtbar button:hover{background:#f0f1f3}\
#preview[contenteditable=\"true\"]{outline:none;min-height:60vh;border:1px solid transparent;border-radius:6px;padding:4px}\
#preview[contenteditable=\"true\"]:focus{box-shadow:0 0 0 2px rgba(46,125,107,.22);border-color:rgba(46,125,107,.4)}\
#preview h1,#preview h2{border-bottom:1px solid #e5e7eb;padding-bottom:.3em;margin:.8em 0 .5em}\
#preview h1{font-size:1.6em}#preview h2{font-size:1.3em}#preview h3{font-size:1.1em}\
#preview h1:first-child,#preview h2:first-child,#preview h3:first-child{margin-top:.2em}\
#preview p{margin:.6em 0}\
#preview ul,#preview ol{margin:.6em 0;padding-left:1.6em}\
#preview li{margin:.15em 0}\
#preview li.task{list-style:none;margin-left:-1.3em}\
#preview code{font-family:ui-monospace,Consolas,monospace;font-size:.9em;background:#f3f4f6;padding:1px 5px;border-radius:4px}\
#preview pre{background:#282c34;color:#e6e6e6;padding:12px 14px;border-radius:8px;overflow:auto;margin:.8em 0}\
#preview pre code{background:none;color:inherit;padding:0}\
#preview blockquote{border-left:3px solid #2e7d6b;padding:.2em 1em;color:#555;background:#e7f3ef;border-radius:0 6px 6px 0;margin:.8em 0}\
#preview a{color:#2e7d6b;text-decoration:none}\
#preview a:hover{text-decoration:underline}\
#preview img{max-width:100%;border-radius:6px}\
#preview hr{border:none;border-top:2px dashed #e5e7eb;margin:1.4em 0}\
#preview table{border-collapse:collapse;margin:.8em 0;width:auto}\
#preview th,#preview td{border:1px solid #e5e7eb;padding:6px 12px}\
#preview th{background:#f0f1f3}\
#preview del{color:#6b7280}\
#preview input[type=checkbox]{margin-right:.4em;accent-color:#2e7d6b}\
</style></head><body><div id=\"preview\"></div>",
    );
    // 参考文件同款渲染引擎（md 逐行解析 + inline 行内解析），逐字一致
    html.push_str(
        r#"<script>
function esc(s){return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');}
function inline(s){
  s=esc(s);
  s=s.replace(/!\[([^\]]*)\]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)/g,'<img src="$2" alt="$1">');
  s=s.replace(/\[([^\]]+)\]\(([^)\s]+)(?:\s+["'][^"']*["'])?\)/g,'<a href="$2" target="_blank">$1</a>');
  s=s.replace(/`([^`]+)`/g,'<code>$1</code>');
  s=s.replace(/\*\*([^*]+)\*\*/g,'<strong>$1</strong>');
  s=s.replace(/__([^_]+)__/g,'<strong>$1</strong>');
  s=s.replace(/(^|[^*])\*([^*\n]+)\*/g,'$1<em>$2</em>');
  s=s.replace(/(^|[^_])_([^_\n]+)_/g,'$1<em>$2</em>');
  s=s.replace(/~~([^~]+)~~/g,'<del>$1</del>');
  return s;
}
function md(src){
  src=(src||'').replace(/\r\n/g,'\n');
  var lines=src.split('\n'),out=[],i=0,list=[],listType=null,inCode=false,codeBuf=[];
  function closeList(){if(listType){out.push((listType==='ul'?'<ul>':'<ol>')+list.map(function(l){return '<li'+(l.task?' class="task"':'')+'>'+l.html+'</li>';}).join('')+'</'+listType+'>');list=[];listType=null;}}
  for(;i<lines.length;i++){
    var L=lines[i];
    if(/^```/.test(L)){if(inCode){out.push('<pre><code>'+codeBuf.join('\n')+'</code></pre>');codeBuf=[];inCode=false;}else{closeList();inCode=true;}continue;}
    if(inCode){codeBuf.push(L);continue;}
    if(!L.trim()){closeList();if(i>0&&lines[i-1].trim())out.push('');continue;}
    var h=L.match(/^(#{1,6})\s+(.*)/);
    if(h){closeList();out.push('<h'+h[1].length+'>'+inline(h[2])+'</h'+h[1].length+'>');continue;}
    if(/^\s*([-*_])\s*(\1\s*){2,}$/.test(L)){closeList();out.push('<hr>');continue;}
    if(/^>\s?/.test(L)){closeList();var q=[],j=i;while(j<lines.length&&/^>/.test(lines[j])){q.push(lines[j].replace(/^>\s?/,''));j++;}out.push('<blockquote>'+inline(q.join('\n'))+'</blockquote>');i=j-1;continue;}
    var tk=L.match(/^\s*[-*+]\s+\[( |x|X)\]\s+(.*)/);
    if(tk){if(listType!=='ul'){closeList();listType='ul';}list.push({task:true,html:'<label><input type="checkbox"'+(tk[1].toLowerCase()==='x'?' checked':'')+' disabled> '+inline(tk[2])+'</label>'});continue;}
    var ul=L.match(/^\s*[-*+]\s+(.*)/);
    if(ul){if(listType!=='ul'){closeList();listType='ul';}list.push({task:false,html:inline(ul[1])});continue;}
    var ol=L.match(/^\s*\d+[.)]\s+(.*)/);
    if(ol){if(listType!=='ol'){closeList();listType='ol';}list.push({task:false,html:inline(ol[1])});continue;}
    if(/^\s*\|/.test(L)||/^\s*\|.*\|\s*$/.test(L)){
      var rows=[],j=i;
      while(j<lines.length&&/^\s*\|/.test(lines[j])){rows.push(lines[j].trim());j++;}
      if(rows.length>=2&&/^\s*\|?[\s:|-]+\|?\s*$/.test(rows[1])){
        var head=rows[0].replace(/^\||\|$/g,'').split('|').map(function(c){return inline(c.trim());});
        var body=rows.slice(2).map(function(r){return '<tr>'+r.replace(/^\||\|$/g,'').split('|').map(function(c){return '<td>'+inline(c.trim())+'</td>';}).join('')+'</tr>';}).join('');
        closeList();out.push('<table><thead><tr>'+head.map(function(c){return '<th>'+c+'</th>';}).join('')+'</tr></thead><tbody>'+body+'</tbody></table>');
        i=j-1;continue;
      }
    }
    closeList();out.push('<p>'+inline(L)+'</p>');
  }
  closeList();if(inCode)out.push('<pre><code>'+codeBuf.join('\n')+'</code></pre>');
  return out.join('\n');
}
document.getElementById('preview').innerHTML=md("#,
    );
    // 注意：raw string 以 "# 结束，结束符会把开引号"吞掉，
    // 因此这里必须单独补一个字符串开引号，再拼接 md 源。
    html.push_str("\"");
    html.push_str(&js_string(md));
    html.push_str("\");</script>");
    if editable {
        let js = if port > 0 {
            EDIT_TOOLBAR_JS.replace("__PORT__", &port.to_string())
        } else {
            EDIT_TOOLBAR_JS.replace("__PORT__", "0")
        };
        html.push_str(&js);
    }
    html.push_str("</body></html>");
    html
}

/// 可编辑网页的顶部工具条脚本：把内容区设为 contenteditable，并提供格式化/复制/打印按钮。
const EDIT_TOOLBAR_JS: &str = r##"<script>
function htmlToMd(root){
  var md='';
  function children(n){for(var i=0;i<n.childNodes.length;i++)walk(n.childNodes[i]);}
  function walk(node){
    if(node.nodeType===3){md+=node.textContent;return;}
    if(node.nodeType!==1)return;
    var tag=node.tagName.toLowerCase();
    if(tag==='h1'||tag==='h2'||tag==='h3'||tag==='h4'||tag==='h5'||tag==='h6'){md+='\n'+'#'.repeat(+tag[1])+' ';children(node);md+='\n\n';return;}
    if(tag==='p'){children(node);md+='\n\n';return;}
    if(tag==='pre'){md+='\n```\n'+node.innerText.replace(/^\n+/,'').replace(/\n+$/,'')+'\n```\n\n';return;}
    if(tag==='ul'||tag==='ol'){
      var prefix=tag==='ul'?'- ':'1. ';
      var lis=node.children;
      for(var i=0;i<lis.length;i++){
        if(lis[i].tagName.toLowerCase()==='li'){
          md+=prefix;children(lis[i]);md+='\n';
        }
      }
      md+='\n';return;
    }
    if(tag==='blockquote'){md+='\n> '+node.innerText.replace(/\n/g,'\n> ')+'\n\n';return;}
    if(tag==='table'){md+=tableToMd(node);return;}
    if(tag==='hr'){md+='\n---\n\n';return;}
    if(tag==='br'){md+='\n';return;}
    if(tag==='strong'||tag==='b'){md+='**';children(node);md+='**';return;}
    if(tag==='em'||tag==='i'){md+='*';children(node);md+='*';return;}
    if(tag==='code'){md+='`';children(node);md+='`';return;}
    if(tag==='del'||tag==='s'){md+='~~';children(node);md+='~~';return;}
    if(tag==='a'){md+='[';children(node);md+=']('+node.getAttribute('href')+')';return;}
    if(tag==='img'){md+='!['+(node.getAttribute('alt')||'')+']('+node.getAttribute('src')+')';return;}
    if(tag==='input'&&node.type==='checkbox'){md+=node.checked?'- [x] ':'- [ ] ';return;}
    children(node);
  }
  function tableToMd(t){
    var rows=t.querySelectorAll('tr'),out=[];
    rows.forEach(function(r,i){
      var cells=r.querySelectorAll('th,td');
      var cellsA=Array.prototype.slice.call(cells);
      var line='|'+cellsA.map(function(c){return ' '+c.innerText.replace(/\|/g,'\\|').trim()+' ';}).join('|')+'|';
      out.push(line);
      if(i===0){out.push('|'+cellsA.map(function(){return ' --- ';}).join('|')+'|');}
    });
    return '\n'+out.join('\n')+'\n\n';
  }
  walk(root);
  return md.replace(/\n{3,}/g,'\n\n').trim()+'\n';
}
(function(){
  var p=document.getElementById('preview');if(!p)return;
  var PORT=__PORT__;
  var syncTimer=null;
  function doSync(){
    if(!PORT)return;
    var md=htmlToMd(p);
    try{
      fetch('http://127.0.0.1:'+PORT+'/save',{method:'POST',headers:{'Content-Type':'text/plain'},body:md}).catch(function(){});
    }catch(e){}
  }
  var t=document.createElement('div');t.id='mdtbar';
  // 编辑 / 预览 切换
  var editing=false;
  var modeBtn=document.createElement('button');
  modeBtn.id='mdtmode';modeBtn.style.fontWeight='bold';
  function setMode(m){
    editing=(m==='edit');
    p.contentEditable=editing?'true':'false';
    fmtBar.style.display=editing?'inline-flex':'none';
    modeBtn.textContent=editing?'预览':'编辑';
  }
  modeBtn.onclick=function(){setMode(editing?'preview':'edit');p.focus();};
  t.appendChild(modeBtn);
  // 格式按钮容器（仅编辑模式显示）
  var fmtBar=document.createElement('span');
  fmtBar.style.display='none';fmtBar.style.gap='5px';
  var B=[['B','bold'],['I','italic'],['U','underline'],['S','strikeThrough'],
    ['H1','formatBlock','H1'],['H2','formatBlock','H2'],['H3','formatBlock','H3'],
    ['引用','formatBlock','BLOCKQUOTE'],['代码','formatBlock','PRE'],
    ['无序','insertUnorderedList'],['有序','insertOrderedList'],
    ['链接','link'],['撤销','undo'],['重做','redo'],['清除','removeFormat']];
  B.forEach(function(x){
    var b=document.createElement('button');b.textContent=x[0];
    b.onclick=function(){
      if(x[1]==='link'){var u=prompt('链接地址','https://');if(u)document.execCommand('createLink',false,u);}
      else{document.execCommand(x[1],false,x[2]||null);}
      p.focus();};
    fmtBar.appendChild(b);
  });
  t.appendChild(fmtBar);
  var sp=document.createElement('span');sp.style.flex='1';t.appendChild(sp);
  document.body.insertBefore(t,document.body.firstChild);
  p.addEventListener('input',function(){
    if(!editing||!PORT)return;
    clearTimeout(syncTimer);
    syncTimer=setTimeout(doSync,800);
  });
  setMode('preview'); // 默认预览：只读不可编辑
})();
</script>"##;

/// 把文本编码为安全的 JS 字符串字面量内容（用于内嵌 Markdown 源，双引号包裹）。
fn js_string(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // 转义尖括号，防止内容含 </script> 等提前终止脚本
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const SAMPLE_MD: &str = "# 欢迎使用 轻量 Markdown 编辑器

这是一个用 **Rust** 编写的本地 Markdown 编辑器，**像 TXT 一样轻量**，即开即用。

## 特性

- 左侧编辑，网页预览
- 语法高亮
- 导出 HTML
- 纯本地，单文件运行

## 链接

[豆包](https://www.doubao.com) 和 [百度](https://www.baidu.com)

## 图片

![示例图片](https://picsum.photos/320/180)

## 任务列表

- [x] 已完成的任务
- [ ] 待办事项
- [ ] 再记一件

## 表格

| 功能 | 说明 |
| --- | --- |
| 编辑 | 支持 Markdown 语法高亮 |
| 预览 | 网页渲染 |
| 导出 | 一键导出为 HTML |

## 代码示例

```rust
fn main() {
    println!(\"hello mdedit\");
}
```

> 提示：点击左上角「文件」菜单，可新建、打开、保存、网页预览文件。
";

impl eframe::App for MdEditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 界面语言：中/英切换闭包（不借用 self，供 UI 各处调用）
        let lang = self.lang;
        let t = move |zh: &'static str, en: &'static str| -> &'static str {
            if lang == Lang::Zh {
                zh
            } else {
                en
            }
        };

        // ---- 处理从资源管理器拖入的图片（自动插入）----
        self.handle_dropped_files(ctx);

        // ---- 启动后把窗口居中到屏幕一次 ----
        if !self.centered_once {
            self.centered_once = true;
            let (mon, win) = ctx.input(|i| {
                let v = i.viewport();
                (
                    v.monitor_size.unwrap_or(egui::vec2(1280.0, 800.0)),
                    v.inner_rect
                        .map(|r| r.size())
                        .unwrap_or(egui::vec2(1150.0, 760.0)),
                )
            });
            let pos = egui::pos2(
                ((mon.x - win.x) / 2.0).max(0.0),
                ((mon.y - win.y) / 2.0).max(0.0),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
        }

        // ---- 检测更新结果（后台线程完成后在此消费）----
        if let Some(rx) = &self.update_rx {
            if let Ok(Some((v, u, n))) = rx.try_recv() {
                self.update_rx = None;
                if version_newer(&v, APP_VERSION) {
                    self.update_available = true;
                    self.update_version = v;
                    self.update_url = u;
                    self.update_note = n;
                } else {
                    self.status = format!("已是最新版本 v{}", APP_VERSION);
                }
            }
        }

        // ---- 消费网页自动同步回的 Markdown（写回 buffer / 文件）----
        if let Some(rx) = &self.sync_rx {
            while let Ok(md) = rx.try_recv() {
                self.buffer = md;
                self.dirty = true;
                if let Some(path) = self.current_file.clone() {
                    if std::fs::write(&path, &self.buffer).is_ok() {
                        self.dirty = false;
                        self.status = "已从网页同步并保存到文件".to_string();
                    } else {
                        self.status = "已从网页同步（文件保存失败）".to_string();
                    }
                } else {
                    self.status = "已从网页同步到编辑器".to_string();
                }
            }
        }

        // ---- 菜单栏 ----
        TopBottomPanel::top("menu").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button(t("文件", "File"), |ui| {
                    if ui.button(t("新建", "New")).clicked() {
                        self.buffer = String::new();
                        self.current_file = None;
                        self.dirty = false;
                        self.status = t("已新建文档", "New document").to_string();
                        ui.close_menu();
                    }
                    if ui.button(t("打开...", "Open...")).clicked() {
                        self.open_dialog();
                        ui.close_menu();
                    }
                    if ui.button(t("保存", "Save")).clicked() {
                        self.save_command();
                        ui.close_menu();
                    }
                    if ui.button(t("另存为...", "Save As...")).clicked() {
                        self.save_dialog();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("导出 HTML...", "Export HTML...")).clicked() {
                        self.export_dialog();
                        ui.close_menu();
                    }
                    if ui.button(t("预览（网页）", "Preview (Web)")).clicked() {
                        self.preview_in_browser();
                        ui.close_menu();
                    }
                    if ui.button(t("打印（网页）", "Print (Web)")).clicked() {
                        self.print_preview();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("设为 .md 默认打开方式", "Set as default .md opener")).clicked() {
                        if register_file_assoc() {
                            self.status =
                                t("已注册：双击 .md 文件将用 mdedit 打开", "Registered: double-click .md opens with mdedit")
                                    .to_string();
                        } else {
                            self.status = t("注册默认打开方式失败", "Failed to register").to_string();
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("退出", "Exit")).clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button(t("编辑", "Edit"), |ui| {
                    if ui.button(t("查找...  (Ctrl+F)", "Find...  (Ctrl+F)")).clicked() {
                        self.show_find = true;
                        ui.close_menu();
                    }
                    if ui.button(t("替换...  (Ctrl+H)", "Replace...  (Ctrl+H)")).clicked() {
                        self.show_find = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("复制 Markdown", "Copy Markdown")).clicked() {
                        ctx.copy_text(self.buffer.clone());
                        self.status = t("Markdown 已复制到剪贴板", "Markdown copied").to_string();
                        ui.close_menu();
                    }
                    if ui.button(t("复制 HTML", "Copy HTML")).clicked() {
                        let html = self.render_html_standalone();
                        ctx.copy_text(html);
                        self.status = t("HTML 已复制到剪贴板", "HTML copied").to_string();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button(t("从剪贴板粘贴 Markdown  (Ctrl+Shift+V)", "Paste Markdown from clipboard  (Ctrl+Shift+V)")).clicked() {
                        self.paste_from_clipboard();
                        ui.close_menu();
                    }
                });

                ui.menu_button(t("视图", "View"), |ui| {
                    if ui.button(t("切换语言：English", "Language: 中文")).clicked() {
                        self.lang = if self.lang == Lang::Zh {
                            Lang::En
                        } else {
                            Lang::Zh
                        };
                        save_lang(self.lang);
                        ui.close_menu();
                    }
                    if ui.selectable_label(self.highlight_enabled, t("语法高亮", "Syntax highlight")).clicked() {
                        self.highlight_enabled = !self.highlight_enabled;
                    }
                    if ui
                        .selectable_label(self.wysiwyg, t("所见即所得（实时预览）", "WYSIWYG (live preview)"))
                        .clicked()
                    {
                        self.wysiwyg = !self.wysiwyg;
                    }
                    ui.separator();
                    if ui.button(t("字号 +", "Font +")).clicked() {
                        self.font_size = (self.font_size + 1.0).min(40.0);
                        ui.close_menu();
                    }
                    if ui.button(t("字号 -", "Font -")).clicked() {
                        self.font_size = (self.font_size - 1.0).max(9.0);
                        ui.close_menu();
                    }
                });

                ui.menu_button(t("帮助", "Help"), |ui| {
                    if ui.button(t("快捷键说明", "Shortcuts")).clicked() {
                        self.show_help = true;
                        ui.close_menu();
                    }
                    if ui.button(t("检查更新", "Check for updates")).clicked() {
                        self.status = t("正在检查更新...", "Checking for updates...").to_string();
                        let (tx, rx) = std::sync::mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(fetch_update_info());
                        });
                        self.update_rx = Some(rx);
                        ui.close_menu();
                    }
                    if ui.button(t("关于", "About")).clicked() {
                        self.show_about = true;
                        ui.close_menu();
                    }
                });

                // 标题
                ui.add_space(12.0);
                let title = self
                    .current_file
                    .as_ref()
                    .map(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("未命名")
                    })
                    .unwrap_or("未命名")
                    .to_string();
                let dirty_mark = if self.dirty { " ●" } else { "" };
                ui.label(
                    RichText::new(format!("{}{}", title, dirty_mark))
                        .color(Color32::from_rgb(60, 70, 90)),
                );
            });
        });

        // ---- 格式工具栏（参考版工具栏）----
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                // 语言切换（中 ⇄ 英）
                let lang_label = if self.lang == Lang::Zh { "EN" } else { "中文" };
                if ui
                    .button(RichText::new(lang_label).size(13.0).strong())
                    .on_hover_text("切换界面语言")
                    .clicked()
                {
                    self.lang = if self.lang == Lang::Zh { Lang::En } else { Lang::Zh };
                    save_lang(self.lang);
                }
                if ui
                    .button(RichText::new(t("▶ 网页预览", "▶ Preview")).size(14.0).strong())
                    .on_hover_text(t("在默认浏览器中预览渲染结果 (Ctrl+P)", "Preview in browser (Ctrl+P)"))
                    .clicked()
                {
                    self.preview_in_browser();
                }
                // 字号调整
                if ui.button(RichText::new("A-").size(13.0)).on_hover_text(t("减小字号", "Smaller font")).clicked() {
                    self.font_size = (self.font_size - 1.0).max(9.0);
                }
                ui.label(RichText::new(format!("{:.0}px", self.font_size)).size(12.0));
                if ui.button(RichText::new("A+").size(13.0)).on_hover_text(t("增大字号", "Larger font")).clicked() {
                    self.font_size = (self.font_size + 1.0).min(40.0);
                }
                // 字体选择
                let font_label = if self.lang == Lang::Zh {
                    font_display_name(&self.font_name)
                } else {
                    self.font_name.clone()
                };
                egui::ComboBox::from_id_salt("font_pick")
                    .selected_text(font_label)
                    .width(92.0)
                    .show_ui(ui, |ui| {
                        for (key, disp) in FONT_CHOICES {
                            let disp = if self.lang == Lang::Zh { *disp } else { *key };
                            ui.selectable_value(&mut self.font_name, key.to_string(), disp);
                        }
                    });
                ui.separator();
                // 语法高亮开关：开启后编辑区按 Markdown 着色；关闭时光标 100% 精确
                let hl_label = if self.highlight_enabled {
                    t("高亮:开", "HL:on")
                } else {
                    t("高亮:关", "HL:off")
                };
                if ui
                    .selectable_label(self.highlight_enabled, RichText::new(hl_label).size(14.0))
                    .on_hover_text(t(
                        "切换语法高亮（开启时可能带来轻微光标偏差；关闭时光标最精确）",
                        "Toggle syntax highlight (slight caret offset when on)",
                    ))
                    .clicked()
                {
                    self.highlight_enabled = !self.highlight_enabled;
                    self.status = if self.highlight_enabled {
                        t("已开启语法高亮", "Highlight on").to_string()
                    } else {
                        t("已关闭语法高亮", "Highlight off").to_string()
                    };
                }
                ui.separator();
                self.tool_btn(ui, Format::Heading, "H", t("标题", "Heading"));
                ui.separator();
                self.tool_btn(ui, Format::Bold, "B", t("加粗", "Bold"));
                self.tool_btn(ui, Format::Italic, "I", t("斜体", "Italic"));
                self.tool_btn(ui, Format::Strike, "S", t("删除线", "Strikethrough"));
                ui.separator();
                self.tool_btn(ui, Format::Link, t("链接", "Link"), t("插入链接", "Insert link"));
                self.tool_btn(ui, Format::Image, t("图片", "Image"), t("插入本地图片", "Insert local image"));
                self.tool_btn(ui, Format::InlineCode, t("代码", "Code"), t("行内代码", "Inline code"));
                self.tool_btn(ui, Format::CodeBlock, t("代码块", "Codeblk"), t("代码块", "Code block"));
                ui.separator();
                self.tool_btn(ui, Format::UnorderedList, t("列表", "List"), t("无序列表", "Unordered list"));
                self.tool_btn(ui, Format::OrderedList, t("有序", "Ordered"), t("有序列表", "Ordered list"));
                self.tool_btn(ui, Format::TaskList, t("任务", "Task"), t("任务列表", "Task list"));
                self.tool_btn(ui, Format::Quote, t("引用", "Quote"), t("引用", "Quote"));
                self.tool_btn(ui, Format::Table, t("表格", "Table"), t("插入表格", "Insert table"));
                self.tool_btn(ui, Format::Divider, t("分割线", "Divider"), t("分割线", "Divider"));
            });
            ui.add_space(2.0);
        });

        // ---- 状态栏（底部）----
        TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).color(Color32::from_rgb(70, 80, 95)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.buffer.lines().count();
                    let w = word_count(&self.buffer);
                    // 当前行列：由光标 char index 推算
                    let byte = char_to_byte(&self.buffer, self.cursor_char.min(chars_count(&self.buffer)));
                    let (line, col) = line_col(&self.buffer, byte);
                    let info = if self.lang == Lang::Zh {
                        format!("{} 字 · {} 行 · 第 {} 行 {} 列 · UTF-8", w, n, line, col)
                    } else {
                        format!("{} chars · {} lines · line {} col {} · UTF-8", w, n, line, col)
                    };
                    ui.label(
                        RichText::new(info)
                            .color(Color32::from_rgb(70, 80, 95)),
                    );
                });
            });
        });

        // ---- 快捷键 ----
        self.handle_shortcuts(ctx);

        // ---- F5 网页预览（延迟到帧内执行）----
        if self.pending_preview {
            self.pending_preview = false;
            self.preview_in_browser();
        }

        // ---- 帮助弹窗 ----
        if self.show_help {
            let mut open = true;
            egui::Window::new("快捷键说明")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_size([430.0, 320.0])
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(RichText::new("文件 / 视图").strong());
                    ui.label("Ctrl+N  新建        Ctrl+O  打开");
                    ui.label("Ctrl+S  保存        Ctrl+Shift+S  另存为");
                    ui.label("Ctrl+E  导出 HTML   Ctrl+P  网页预览");
                    ui.label("F5  网页预览        Ctrl+=  字号增大");
                    ui.label("Ctrl+-  字号减小");
                    ui.add_space(4.0);
                    ui.label(RichText::new("查找 / 替换").strong());
                    ui.label("Ctrl+F  查找        Ctrl+H  替换");
                    ui.add_space(4.0);
                    ui.label(RichText::new("格式（光标处插入）").strong());
                    ui.label("Ctrl+B  加粗        Ctrl+I  斜体");
                    ui.label("Ctrl+D  删除线      Ctrl+K  行内代码");
                    ui.label("Ctrl+Shift+C  代码块  Ctrl+Shift+L  无序列表");
                    ui.label("Ctrl+Shift+Q  引用  Ctrl+Shift+I  插入图片");
                    ui.label("Ctrl+1  标题1      Ctrl+2  标题2      Ctrl+3  标题3");
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("提示：以上 Ctrl 在 macOS 为 Command；多数格式可选中文字后应用。")
                            .color(Color32::from_rgb(80, 90, 110))
                            .size(12.0),
                    );
                });
            self.show_help = open;
        }

        // ---- 查找 / 替换弹窗 ----
        if self.show_find {
            let mut open = true;
            egui::Window::new("查找 / 替换")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_size([430.0, 230.0])
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("查找");
                        ui.add(egui::TextEdit::singleline(&mut self.find_text).desired_width(180.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("替换");
                        ui.add(egui::TextEdit::singleline(&mut self.replace_text).desired_width(180.0));
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("查找下一个").clicked() {
                            self.find_next(ctx);
                        }
                        if ui.button("替换").clicked() {
                            self.replace_one(ctx);
                        }
                        if ui.button("全部替换").clicked() {
                            self.replace_all();
                        }
                    });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new("点右上角 × 关闭")
                            .color(Color32::from_rgb(120, 130, 145))
                            .size(12.0),
                    );
                });
            self.show_find = open;
        }

        // ---- 更新提示弹窗（有可用新版本时）----
        if self.update_available {
            let mut open = true;
            egui::Window::new("发现新版本")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_size([460.0, 280.0])
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(
                        RichText::new(format!("新版本 v{} 可用（当前 v{}）", self.update_version, APP_VERSION))
                            .size(15.0)
                            .strong(),
                    );
                    if !self.update_note.trim().is_empty() {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.label("更新说明：");
                        ui.label(&self.update_note);
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("前往下载").strong()).clicked() {
                            open_url(&self.update_url);
                            self.update_available = false;
                            self.status = "已打开下载页，请选择保存位置".to_string();
                        }
                        if ui.button("稍后再说").clicked() {
                            self.update_available = false;
                        }
                    });
                    ui.label(
                        RichText::new("下载完成后，解压/替换旧文件即可。")
                            .size(12.0)
                            .color(Color32::from_rgb(120, 130, 145)),
                    );
                });
            // X 关闭也要同步（按钮已直接置 false；此判断只兜底 X 关闭）
            if !open {
                self.update_available = false;
            }
        }

        // ---- 关于弹窗 ----
        if self.show_about {
            let mut open = true;
            egui::Window::new("关于")
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .default_size([400.0, 280.0])
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(RichText::new("MDEditor").size(16.0).strong());
                    ui.label(format!("版本 v{}", APP_VERSION));
                    ui.label("纯本地、像 TXT 一样轻量的 Markdown 编辑器。");
                    ui.label("支持语法高亮、网页预览、查找替换、导出与打印。");
                    ui.separator();
                    if ui.button("访问官网").clicked() {
                        open_url("https://www.yuanyuzhou.me");
                    }
                });
            self.show_about = open;
        }

        // ---- 所见即所得：右侧实时渲染预览（可点击定位到源码）----
        let mut wys_click: Option<usize> = None;
        if self.wysiwyg {
            egui::SidePanel::right("wysiwyg_panel")
                .resizable(true)
                .default_width(430.0)
                .min_width(220.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(t("预览（所见即所得）", "Preview (WYSIWYG)")).strong());
                    ui.label(
                        RichText::new(t("点击内容可跳转到源码对应位置", "Click content to jump to its source"))
                            .size(11.0)
                            .color(Color32::from_rgb(120, 130, 145)),
                    );
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            wys_click = wysiwyg::render_preview(ui, &self.buffer, self.font_size);
                        });
                });
            // 点击预览 → 定位并聚焦源码编辑区
            if let Some(byte) = wys_click {
                let clamped = byte.min(self.buffer.len());
                self.cursor_char = chars_count(&self.buffer[..clamped]);
                if let Some(mut st) = egui::widgets::text_edit::TextEditState::load(
                    ctx,
                    egui::Id::new("md_edit"),
                ) {
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(
                        egui::text::CCursor::new(self.cursor_char),
                    )));
                }
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("md_edit")));
            }
        }

        // ---- 全宽编辑区（取消内置分屏预览，改为按钮调用网页预览）----
        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // 编辑区：按「语法高亮」开关决定是否启用 layouter。
                    // 关闭（默认）时用 TextEdit 原生渲染，光标 100% 精确（像记事本）；
                    // 开启时按 Markdown 着色（自定义 layouter 可能带来轻微光标偏差）。
                    let response = if self.highlight_enabled {
                        let editor_font = self.editor_font_id();
                        let layouter_font = editor_font.clone();
                        let mut layouter = {
                            move |ui: &egui::Ui, string: &str, wrap_width: f32| -> std::sync::Arc<egui::Galley> {
                                let mut job = highlight::highlight_markdown(string, layouter_font.clone());
                                job.wrap.max_width = wrap_width;
                                ui.fonts(|f| f.layout_job(job))
                            }
                        };
                        let ef = editor_font.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut self.buffer)
                                .id(egui::Id::new("md_edit"))
                                .font(ef)
                                .desired_width(f32::INFINITY)
                                .desired_rows(40)
                                .frame(false)
                                .lock_focus(true)
                                .layouter(&mut layouter),
                        )
                    } else {
                        let ef = self.editor_font_id();
                        ui.add(
                            egui::TextEdit::multiline(&mut self.buffer)
                                .id(egui::Id::new("md_edit"))
                                .font(ef)
                                .desired_width(f32::INFINITY)
                                .desired_rows(40)
                                .frame(false)
                                .lock_focus(true),
                        )
                    };
                    if response.changed() {
                        self.dirty = true;
                        self.status = "编辑中...".to_string();
                    }
                    // 记录当前光标 char index（供格式插入与行列统计）
                    if let Some(st) = egui::widgets::text_edit::TextEditState::load(
                        ui.ctx(),
                        egui::Id::new("md_edit"),
                    ) {
                        if let Some(range) = st.cursor.char_range() {
                            self.cursor_char = range.primary.index;
                        }
                    }
                });
        });

        // ---- 自动保存草稿（有改动则写盘）----
        if self.dirty {
            save_draft(&self.buffer);
        }

        ctx.request_repaint();
    }
}

impl MdEditorApp {
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // 延迟消费的格式操作（避免在 input 回调里弹文件对话框 / 阻塞事件循环）
        if let Some(f) = self.pending_format.take() {
            self.apply_format(f);
        }
        if let Some(d) = self.pending_font_delta.take() {
            self.font_size = (self.font_size + d).clamp(9.0, 40.0);
        }
        ctx.input_mut(|i| {
            let ctrl = i.modifiers.command;
            let shift = i.modifiers.shift;
            if ctrl {
                let s = i.key_pressed(egui::Key::S);
                let o = i.key_pressed(egui::Key::O);
                let n = i.key_pressed(egui::Key::N);
                let e = i.key_pressed(egui::Key::E);
                let p = i.key_pressed(egui::Key::P);
                let f = i.key_pressed(egui::Key::F);
                let h = i.key_pressed(egui::Key::H);
                let b = i.key_pressed(egui::Key::B);
                let it = i.key_pressed(egui::Key::I);
                let d = i.key_pressed(egui::Key::D);
                let k = i.key_pressed(egui::Key::K);
                let c = i.key_pressed(egui::Key::C);
                let l = i.key_pressed(egui::Key::L);
                let q = i.key_pressed(egui::Key::Q);
                let v = i.key_pressed(egui::Key::V);
                let plus = i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals);
                let minus = i.key_pressed(egui::Key::Minus);
                let num = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3]
                    .iter()
                    .position(|k| i.key_pressed(*k));
                if shift && s {
                    self.save_dialog();
                } else if shift && v {
                    self.paste_from_clipboard();
                } else if s {
                    self.save_command();
                } else if o {
                    self.open_dialog();
                } else if n {
                    self.buffer = String::new();
                    self.current_file = None;
                    self.dirty = false;
                    self.status = "已新建文档".to_string();
                } else if e {
                    self.export_dialog();
                } else if p {
                    self.preview_in_browser();
                } else if f || h {
                    self.show_find = true;
                } else if b {
                    self.pending_format = Some(Format::Bold);
                } else if it {
                    self.pending_format = Some(Format::Italic);
                } else if d {
                    self.pending_format = Some(Format::Strike);
                } else if k {
                    self.pending_format = Some(Format::InlineCode);
                } else if shift && c {
                    self.pending_format = Some(Format::CodeBlock);
                } else if shift && l {
                    self.pending_format = Some(Format::UnorderedList);
                } else if shift && q {
                    self.pending_format = Some(Format::Quote);
                } else if shift && it {
                    self.pending_format = Some(Format::Image);
                } else if plus {
                    self.pending_font_delta = Some(1.0);
                } else if minus {
                    self.pending_font_delta = Some(-1.0);
                } else if let Some(idx) = num {
                    let f = match idx {
                        1 => Format::Heading2,
                        2 => Format::Heading3,
                        _ => Format::Heading,
                    };
                    self.pending_format = Some(f);
                }
            }
            // F5：网页预览
            if i.key_pressed(egui::Key::F5) {
                self.pending_preview = true;
            }
        });
    }
}
