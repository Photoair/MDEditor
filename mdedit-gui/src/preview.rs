//! Markdown → 富文本块，供导出/网页预览使用。

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// 把 Markdown 源码渲染为一组可绘制的富文本块。
pub struct PreviewBlock {
    pub kind: Kind,
    pub text: String,
    pub level: u8, // 标题级别
}

pub enum Kind {
    Heading,
    Paragraph,
    ListItem,
    Quote,
    Code,
    Table,
    Divider,
    Task,
    Image,
}

/// 简化渲染：逐事件构建文本块，把相邻行文本累积。
pub fn render(md: &str) -> Vec<PreviewBlock> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(md, options);
    let mut out: Vec<PreviewBlock> = Vec::new();
    let mut buf = String::new();
    let mut heading_level = 0u8;
    let mut in_code = false;
    let mut code_buf = String::new();
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut item_kind = Kind::Paragraph;

    let flush_para = |buf: &mut String, out: &mut Vec<PreviewBlock>, level: u8, kind: &mut Kind| {
        if !buf.trim().is_empty() {
            let final_kind = if level > 0 {
                Kind::Heading
            } else {
                // 保留 item 判断
                match kind {
                    Kind::Task => Kind::Task,
                    Kind::ListItem => Kind::ListItem,
                    _ => Kind::Paragraph,
                }
            };
            out.push(PreviewBlock {
                kind: final_kind,
                text: buf.trim().to_string(),
                level,
            });
        }
        buf.clear();
        *kind = Kind::Paragraph;
    };

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                heading_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
            }
            Event::End(TagEnd::Heading(_)) => {
                flush_para(&mut buf, &mut out, heading_level, &mut item_kind);
                heading_level = 0;
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
            }
            Event::Start(Tag::Item) => {
                item_kind = Kind::ListItem;
            }
            Event::End(TagEnd::Item) => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
                out.push(PreviewBlock { kind: Kind::ListItem, text: String::new(), level: 0 });
            }
            Event::Start(Tag::BlockQuote(_)) => {}
            Event::End(TagEnd::BlockQuote(_)) => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
                out.push(PreviewBlock { kind: Kind::Quote, text: String::new(), level: 0 });
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
                in_code = true;
                code_buf.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                out.push(PreviewBlock { kind: Kind::Code, text: code_buf.trim_end().to_string(), level: 0 });
            }
            Event::Text(t) => {
                if in_code {
                    code_buf.push_str(&t);
                } else {
                    buf.push_str(&escape_html(&t));
                }
            }
            Event::Code(t) => {
                buf.push_str(&format!("<code>{}</code>", escape_html(&t)));
            }
            Event::TaskListMarker(_checked) => {
                item_kind = Kind::Task;
            }
            Event::Start(Tag::Table(_)) => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
                in_table = true;
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                // table_rows[0]=表头行，其余=数据行；生成真正的 HTML 表格字符串
                let mut html = String::from("<table>");
                if let Some(head) = table_rows.first() {
                    html.push_str("<thead><tr>");
                    for cell in head {
                        html.push_str(&format!("<th>{}</th>", escape_html(cell)));
                    }
                    html.push_str("</tr></thead>");
                }
                if table_rows.len() > 1 {
                    html.push_str("<tbody>");
                    for row in &table_rows[1..] {
                        html.push_str("<tr>");
                        for cell in row {
                            html.push_str(&format!("<td>{}</td>", escape_html(cell)));
                        }
                        html.push_str("</tr>");
                    }
                    html.push_str("</tbody>");
                }
                html.push_str("</table>");
                out.push(PreviewBlock { kind: Kind::Table, text: html, level: 0 });
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                if in_table {
                    table_rows.push(Vec::new());
                }
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => {}
            Event::Start(Tag::Link { dest_url, .. }) => {
                buf.push_str(&format!(
                    "<a href=\"{}\" target=\"_blank\">",
                    escape_html(&dest_url)
                ));
            }
            Event::End(TagEnd::Link) => {
                buf.push_str("</a>");
            }
            Event::Start(Tag::Strong) => buf.push_str("<strong>"),
            Event::End(TagEnd::Strong) => buf.push_str("</strong>"),
            Event::Start(Tag::Emphasis) => buf.push_str("<em>"),
            Event::End(TagEnd::Emphasis) => buf.push_str("</em>"),
            Event::Start(Tag::Strikethrough) => buf.push_str("<del>"),
            Event::End(TagEnd::Strikethrough) => buf.push_str("</del>"),
            Event::Start(Tag::Image { .. }) => {}
            Event::End(TagEnd::Image) => {}
            Event::Rule => {
                flush_para(&mut buf, &mut out, 0, &mut item_kind);
                out.push(PreviewBlock { kind: Kind::Divider, text: String::new(), level: 0 });
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code {
                    code_buf.push('\n');
                } else {
                    buf.push('\n');
                }
            }
            _ => {}
        }
    }
    flush_para(&mut buf, &mut out, heading_level, &mut item_kind);
    out
}

/// HTML 转义（用于表格单元格等）。
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
