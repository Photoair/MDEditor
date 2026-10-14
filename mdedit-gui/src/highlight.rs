//! 编辑区 Markdown 语法高亮（轻量行级着色）。
//!
//! 重要约束：为了在开启高亮时不破坏 TextEdit 的光标定位，**每一行只追加一个
//! 颜色 span**（不做行内拆分），并用与 TextEdit 相同的等宽字体与字号。这样
//! galley 的每行结构与原生排版完全一致，光标与着色可同时正常工作。

use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};

/// 对 Markdown 源码进行轻量语法高亮，返回 LayoutJob 供编辑器绘制。
/// `font` 为当前编辑区字体（family + 字号），保证与光标定位一致。
pub fn highlight_markdown(src: &str, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut in_code_block = false;

    for line in src.split_inclusive('\n') {
        let line_no_nl = line.trim_end_matches('\n').trim_end_matches('\r');
        let has_nl = line.ends_with('\n');
        let trimmed = line_no_nl.trim_start();

        // 代码块定界符
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            append(&mut job, line_no_nl, &Style::CodeMarker, has_nl, &font);
            continue;
        }
        if in_code_block {
            append(&mut job, line_no_nl, &Style::Code, has_nl, &font);
            continue;
        }
        // 标题
        if let Some(_d) = heading_depth(line_no_nl) {
            append(&mut job, line_no_nl, &Style::Heading, has_nl, &font);
            continue;
        }
        // 引用
        if trimmed.starts_with('>') {
            append(&mut job, line_no_nl, &Style::Quote, has_nl, &font);
            continue;
        }
        // 列表标记 / 任务
        if list_marker_len(trimmed).is_some() {
            append(&mut job, line_no_nl, &Style::ListMarker, has_nl, &font);
            continue;
        }
        // 普通行
        append(&mut job, line_no_nl, &Style::Plain, has_nl, &font);
    }
    job
}

fn heading_depth(line: &str) -> Option<u8> {
    let t = line.trim_start();
    if t.starts_with('#') {
        let mut n = 0u8;
        for c in t.chars() {
            if c == '#' {
                n += 1;
            } else {
                break;
            }
        }
        if n >= 1 && n <= 6 {
            let after = &t[n as usize..];
            if after.chars().next().map(|c| c == ' ' || c == '\t').unwrap_or(true) {
                return Some(n);
            }
        }
    }
    None
}

/// 返回列表标记长度（在 trim 后的行上）。
fn list_marker_len(trimmed: &str) -> Option<usize> {
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    match first {
        '-' | '*' | '+' => {
            if chars.next().map(|c| c == ' ' || c == '\t').unwrap_or(false) {
                Some(1)
            } else {
                None
            }
        }
        d if d.is_ascii_digit() => {
            let mut n = 0usize;
            for c in trimmed.chars() {
                if c.is_ascii_digit() {
                    n += 1;
                } else {
                    break;
                }
            }
            let after = &trimmed[n..];
            if after.starts_with(". ") || after.starts_with(") ") {
                Some(n + 1)
            } else {
                None
            }
        }
        _ => None,
    }
}

enum Style {
    Heading,
    Code,
    CodeMarker,
    Quote,
    ListMarker,
    Plain,
}

fn append(job: &mut LayoutJob, text: &str, style: &Style, nl: bool, font: &FontId) {
    if !text.is_empty() {
        let (color, bg) = style_props(style);
        job.append(
            text,
            0.0,
            TextFormat {
                font_id: font.clone(),
                color,
                background: bg,
                ..Default::default()
            },
        );
    }
    if nl {
        job.append("\n", 0.0, TextFormat::default());
    }
}

fn style_props(s: &Style) -> (Color32, Color32) {
    match s {
        Style::Heading => (Color32::from_rgb(15, 50, 110), Color32::TRANSPARENT),
        Style::Code => (Color32::from_rgb(140, 60, 10), Color32::from_rgb(242, 243, 245)),
        Style::CodeMarker => (Color32::from_rgb(120, 70, 20), Color32::from_rgb(242, 243, 245)),
        Style::Quote => (Color32::from_rgb(0, 110, 75), Color32::from_rgb(238, 247, 243)),
        Style::ListMarker => (Color32::from_rgb(0, 90, 170), Color32::TRANSPARENT),
        Style::Plain => (Color32::from_rgb(25, 28, 34), Color32::TRANSPARENT),
    }
}
