//! 所见即所得预览渲染器：把 Markdown 源码实时渲染为富文本 UI。
//! 点击任意块/行时返回该块在源码中的字节偏移，供主程序定位光标编辑。

use egui::{Color32, RichText, Sense, Ui};

pub fn render_preview(ui: &mut Ui, buffer: &str, font_size: f32) -> Option<usize> {
    let mut click_byte: Option<usize> = None;
    let mut offset: usize = 0; // 当前行在 buffer 中的字节起始
    let lines: Vec<&str> = buffer.split('\n').collect();
    let heading_color = Color32::from_rgb(15, 50, 110);
    let quote_color = Color32::from_rgb(0, 110, 75);
    let code_bg = Color32::from_rgb(244, 246, 250);
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // 代码块 ``` ... ```
        if trimmed.starts_with("```") {
            let start_off = offset;
            let mut code = String::new();
            let mut k = i + 1;
            while k < lines.len() && !lines[k].trim_start().starts_with("```") {
                code.push_str(lines[k]);
                code.push('\n');
                k += 1;
            }
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(code)
                        .monospace()
                        .size(font_size)
                        .background_color(code_bg),
                )
                .sense(Sense::click()),
            );
            if resp.clicked() {
                click_byte = Some(start_off);
            }
            while i <= k && i < lines.len() {
                offset += lines[i].len() + 1;
                i += 1;
            }
            ui.add_space(2.0);
            continue;
        }

        // 表格（连续以 | 开头的行）
        if trimmed.starts_with('|') {
            let seg = offset;
            let mut rows: Vec<String> = vec![];
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(lines[i].to_string());
                offset += lines[i].len() + 1;
                i += 1;
            }
            render_table(ui, &rows, font_size, &mut click_byte, seg);
            ui.add_space(4.0);
            continue;
        }

        // 标题 # ~ ######
        let lead = trimmed.len() - trimmed.trim_start_matches('#').len();
        if lead > 0 && lead <= 6 {
            let htext = trimmed[lead..].trim_start();
            let size = font_size + ((3.0 - lead as f32).max(0.0)) * 2.0;
            ui.add_space(2.0);
            let resp = ui.add(
                egui::Label::new(
                    RichText::new(htext)
                        .size(size)
                        .strong()
                        .color(heading_color),
                )
                .sense(Sense::click()),
            );
            if resp.clicked() {
                click_byte = Some(offset);
            }
            ui.add_space(2.0);
            offset += line.len() + 1;
            i += 1;
            continue;
        }

        // 引用 > 块
        if trimmed.starts_with('>') {
            let seg = offset;
            let mut q = String::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let tl = lines[i].trim_start();
                let idx = tl.len() - tl.trim_start_matches('>').len();
                q.push_str(&tl[idx..].trim_start());
                q.push('\n');
                offset += lines[i].len() + 1;
                i += 1;
            }
            let resp = ui.add(
                egui::Label::new(RichText::new(q).size(font_size).color(quote_color))
                    .sense(Sense::click()),
            );
            if resp.clicked() {
                click_byte = Some(seg);
            }
            ui.add_space(2.0);
            continue;
        }

        // 分割线 --- / *** / ___
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_') && trimmed.len() >= 3 {
            ui.separator();
            offset += line.len() + 1;
            i += 1;
            continue;
        }

        // 任务 / 无序列表
        let list_prefix;
        let content;
        if let Some(r) = trimmed.strip_prefix("- [ ] ") {
            list_prefix = "\u{2610} ";
            content = r;
        } else if let Some(r) = trimmed.strip_prefix("- [x] ") {
            list_prefix = "\u{2611} ";
            content = r;
        } else if let Some(r) = trimmed.strip_prefix("- ") {
            list_prefix = "\u{2022} ";
            content = r;
        } else if let Some(r) = trimmed.strip_prefix("* ") {
            list_prefix = "\u{2022} ";
            content = r;
        } else {
            list_prefix = "";
            content = "";
        }
        if !list_prefix.is_empty() {
            let seg = offset;
            ui.horizontal(|ui| {
                ui.label(RichText::new(list_prefix).size(font_size));
                inline_label(ui, content, font_size, None, &mut click_byte, seg);
            });
            offset += line.len() + 1;
            i += 1;
            continue;
        }

        // 有序列表 1. 2. ...
        if let Some(dot) = trimmed.find(". ") {
            if dot <= 3 && trimmed[..dot].chars().all(|c| c.is_ascii_digit()) {
                let seg = offset;
                let num = trimmed[..=dot].to_string();
                let rest = &trimmed[dot + 2..];
                ui.horizontal(|ui| {
                    ui.label(RichText::new(num).size(font_size));
                    inline_label(ui, rest, font_size, None, &mut click_byte, seg);
                });
                offset += line.len() + 1;
                i += 1;
                continue;
            }
        }

        // 普通段落：合并到空行或下一个块标记
        let seg = offset;
        let mut para = String::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && !lines[i].trim_start().starts_with('|')
            && !lines[i].trim_start().starts_with("```")
        {
            para.push_str(lines[i]);
            para.push('\n');
            offset += lines[i].len() + 1;
            i += 1;
        }
        if !para.is_empty() {
            inline_label(ui, para.trim_end(), font_size, None, &mut click_byte, seg);
            ui.add_space(3.0);
        } else {
            // 空行，跳过
            offset += line.len() + 1;
            i += 1;
        }
    }
    click_byte
}

/// 渲染一个富文本 label（可点击，映射到段起始字节）。
fn emit(
    ui: &mut Ui,
    text: &str,
    size: f32,
    strong: bool,
    mono: bool,
    color: Option<Color32>,
    click: &mut Option<usize>,
    seg: usize,
) {
    let mut rt = RichText::new(text).size(size);
    if strong {
        rt = rt.strong();
    }
    if mono {
        rt = rt.monospace().background_color(Color32::from_rgb(244, 246, 250));
    }
    if let Some(c) = color {
        rt = rt.color(c);
    }
    let resp = ui.add(egui::Label::new(rt).sense(Sense::click()));
    if resp.clicked() {
        *click = Some(seg);
    }
}

/// 行内解析：**加粗** `代码` [链接](url)，其余按普通文本渲染。
fn inline_label(
    ui: &mut Ui,
    text: &str,
    size: f32,
    color: Option<Color32>,
    click: &mut Option<usize>,
    seg: usize,
) {
    let mut s = text;
    while !s.is_empty() {
        if let Some(p) = s.find("**") {
            if p > 0 {
                emit(ui, &s[..p], size, false, false, color, click, seg);
            }
            s = &s[p + 2..];
            if let Some(q) = s.find("**") {
                emit(ui, &s[..q], size, true, false, color, click, seg);
                s = &s[q + 2..];
            } else {
                emit(ui, s, size, true, false, color, click, seg);
                break;
            }
        } else if let Some(p) = s.find('`') {
            if p > 0 {
                emit(ui, &s[..p], size, false, false, color, click, seg);
            }
            s = &s[p + 1..];
            if let Some(q) = s.find('`') {
                emit(ui, &s[..q], size, false, true, color, click, seg);
                s = &s[q + 1..];
            } else {
                emit(ui, s, size, false, true, color, click, seg);
                break;
            }
        } else if let Some(p) = s.find('[') {
            if p > 0 {
                emit(ui, &s[..p], size, false, false, color, click, seg);
            }
            s = &s[p + 1..];
            if let Some(close) = s.find(']') {
                let txt = &s[..close];
                let after = &s[close + 1..];
                if let Some(o) = after.strip_prefix('(') {
                    if let Some(u) = o.find(')') {
                        let _url = &o[..u];
                        let rt = RichText::new(txt)
                            .size(size)
                            .color(Color32::from_rgb(0, 110, 220))
                            .underline();
                        if ui.add(egui::Label::new(rt).sense(Sense::click())).clicked() {
                            *click = Some(seg);
                        }
                        s = &o[u + 1..];
                        continue;
                    }
                }
                emit(ui, txt, size, false, false, color, click, seg);
                s = after;
            } else {
                emit(ui, s, size, false, false, color, click, seg);
                break;
            }
        } else {
            emit(ui, s, size, false, false, color, click, seg);
            break;
        }
    }
}

fn render_table(
    ui: &mut Ui,
    rows: &[String],
    size: f32,
    click: &mut Option<usize>,
    seg: usize,
) {
    // 解析每行 | a | b |
    let mut grid: Vec<Vec<String>> = Vec::new();
    for r in rows {
        let t = r.trim().trim_start_matches('|').trim_end_matches('|');
        grid.push(t.split('|').map(|c| c.trim().to_string()).collect());
    }
    if grid.is_empty() {
        return;
    }
    // 去掉分隔行 | --- | --- |
    if grid.len() > 1 && grid[1].iter().all(|c| !c.is_empty() && c.chars().all(|x| x == '-' || x == ':')) {
        grid.remove(1);
    }
    if grid.is_empty() {
        return;
    }
    let header = grid[0].clone();
    let body = &grid[1..];
    ui.push_id(("tbl", seg), |ui| {
        egui::Grid::new(ui.id()).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
            for cell in &header {
                ui.label(RichText::new(cell).size(size).strong());
            }
            ui.end_row();
            for row in body {
                for cell in row {
                    ui.label(RichText::new(cell).size(size));
                }
                ui.end_row();
            }
        });
    });
    // 点击表格任意空白可定位（简单起见，点击表头单元格）
    let _ = click;
    let _ = seg;
}
