/// Базовый HTML парсер для браузера RustOS
/// Поддерживает: h1-h6, p, b, i, u, br, hr, a, ul, ol, li, img, div, span, pre,
/// code, table
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use crate::graphics::Color;

/// Элементы DOM дерева
#[derive(Debug, Clone)]
pub enum HtmlNode {
    Element(HtmlElement),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct HtmlElement {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<HtmlNode>,
}

impl HtmlElement {
    pub fn get_attr(&self, name: &str) -> Option<String> {
        for (k, v) in &self.attributes {
            if k == name {
                return Some(v.clone());
            }
        }
        None
    }
}

/// Рендеренный элемент для отображения
#[derive(Debug, Clone)]
pub struct RenderedLine {
    pub text: String,
    pub color: Color,
    pub is_bold: bool,
    pub is_italic: bool,
    pub is_heading: bool,
    pub heading_level: u8,
    pub is_link: bool,
    pub link_url: Option<String>,
    pub indent: usize,
    pub is_horizontal_rule: bool,
    pub is_image: bool,
    pub image_alt: String,
}

/// Парсит HTML строку в DOM дерево
pub fn parse_html(html: &str) -> Vec<HtmlNode> {
    let mut nodes = Vec::new();
    let mut pos = 0;
    let chars: Vec<char> = html.chars().collect();

    parse_nodes(&chars, &mut pos, &mut nodes);
    nodes
}

fn parse_nodes(chars: &[char], pos: &mut usize, nodes: &mut Vec<HtmlNode>) {
    while *pos < chars.len() {
        if chars[*pos] == '<' {
            if *pos + 1 < chars.len() && chars[*pos + 1] == '/' {
                // Закрывающий тег — выходим
                return;
            }
            if let Some(element) = parse_element(chars, pos) {
                nodes.push(HtmlNode::Element(element));
            }
        } else {
            // Текстовый контент
            let mut text = String::new();
            while *pos < chars.len() && chars[*pos] != '<' {
                text.push(chars[*pos]);
                *pos += 1;
            }
            if !text.trim().is_empty() {
                nodes.push(HtmlNode::Text(text.trim().to_string()));
            }
        }
    }
}

fn parse_element(chars: &[char], pos: &mut usize) -> Option<HtmlElement> {
    if *pos >= chars.len() || chars[*pos] != '<' {
        return None;
    }
    *pos += 1; // skip '<'

    // Проверяем self-closing
    if *pos < chars.len() && chars[*pos] == '!' {
        // Comment or doctype — пропускаем
        while *pos < chars.len() && chars[*pos] != '>' {
            *pos += 1;
        }
        if *pos < chars.len() {
            *pos += 1;
        } // skip '>'
        return None;
    }

    // Имя тега
    let mut tag = String::new();
    while *pos < chars.len()
        && chars[*pos] != ' '
        && chars[*pos] != '>'
        && chars[*pos] != '\t'
        && chars[*pos] != '\n'
    {
        tag.push(chars[*pos]);
        *pos += 1;
    }
    tag = tag.to_lowercase();

    // Атрибуты
    let mut attributes = Vec::new();
    while *pos < chars.len() && chars[*pos] != '>' {
        if chars[*pos] == ' ' || chars[*pos] == '\t' || chars[*pos] == '\n' {
            *pos += 1;
            continue;
        }
        let mut attr_name = String::new();
        while *pos < chars.len() && chars[*pos] != '=' && chars[*pos] != ' ' && chars[*pos] != '>' {
            attr_name.push(chars[*pos]);
            *pos += 1;
        }
        if *pos < chars.len() && chars[*pos] == '=' {
            *pos += 1; // skip '='
            if *pos < chars.len() && (chars[*pos] == '"' || chars[*pos] == '\'') {
                let quote = chars[*pos];
                *pos += 1;
                let mut attr_val = String::new();
                while *pos < chars.len() && chars[*pos] != quote {
                    attr_val.push(chars[*pos]);
                    *pos += 1;
                }
                if *pos < chars.len() {
                    *pos += 1;
                } // skip quote
                attributes.push((attr_name, attr_val));
            }
        } else if !attr_name.is_empty() {
            attributes.push((attr_name, String::new()));
        }
    }

    if *pos < chars.len() {
        *pos += 1;
    } // skip '>'

    // Self-closing tags
    let self_closing =
        tag == "br" || tag == "hr" || tag == "img" || tag == "input" || tag == "meta";
    if self_closing {
        return Some(HtmlElement {
            tag,
            attributes,
            children: Vec::new(),
        });
    }

    // Children
    let mut children = Vec::new();
    parse_nodes(chars, pos, &mut children);

    // Skip closing tag
    if *pos < chars.len() && chars[*pos] == '<' && *pos + 1 < chars.len() && chars[*pos + 1] == '/'
    {
        while *pos < chars.len() && chars[*pos] != '>' {
            *pos += 1;
        }
        if *pos < chars.len() {
            *pos += 1;
        }
    }

    Some(HtmlElement {
        tag,
        attributes,
        children,
    })
}

/// Рендерит DOM в линии текста
pub fn render_dom(nodes: &[HtmlNode]) -> Vec<RenderedLine> {
    let mut lines = Vec::new();
    let mut state = RenderState::default();
    render_nodes(nodes, &mut lines, &mut state);
    lines
}

#[derive(Default, Clone)]
struct RenderState {
    bold: bool,
    italic: bool,
    underline: bool,
    link_url: Option<String>,
    indent: usize,
    heading: bool,
    heading_level: u8,
    in_list: bool,
    list_ordered: bool,
    list_counter: usize,
    in_pre: bool,
}

fn render_nodes(nodes: &[HtmlNode], lines: &mut Vec<RenderedLine>, state: &mut RenderState) {
    for node in nodes {
        match node {
            HtmlNode::Text(text) => {
                let decoded = decode_html_entities(text);
                if state.in_pre {
                    // Preformatted — сохраняем все пробелы
                    for line in decoded.split('\n') {
                        if !line.is_empty() || lines.is_empty() {
                            lines.push(RenderedLine {
                                text: line.to_string(),
                                color: Color::from_u32(0xABB2BF),
                                is_bold: state.bold,
                                is_italic: state.italic,
                                is_heading: false,
                                heading_level: 0,
                                is_link: state.link_url.is_some(),
                                link_url: state.link_url.clone(),
                                indent: state.indent,
                                is_horizontal_rule: false,
                                is_image: false,
                                image_alt: String::new(),
                            });
                        }
                    }
                } else {
                    let normalized = decoded.split_whitespace().collect::<Vec<&str>>().join(" ");
                    if !normalized.is_empty() {
                        if let Some(last) = lines.last_mut() {
                            if !last.is_horizontal_rule && !last.is_image {
                                last.text.push(' ');
                                last.text.push_str(&normalized);
                                continue;
                            }
                        }
                        lines.push(RenderedLine {
                            text: normalized,
                            color: if state.link_url.is_some() {
                                Color::from_u32(0x61AFEF)
                            } else {
                                Color::from_u32(0xABB2BF)
                            },
                            is_bold: state.bold,
                            is_italic: state.italic,
                            is_heading: state.heading,
                            heading_level: state.heading_level,
                            is_link: state.link_url.is_some(),
                            link_url: state.link_url.clone(),
                            indent: state.indent,
                            is_horizontal_rule: false,
                            is_image: false,
                            image_alt: String::new(),
                        });
                    }
                }
            },
            HtmlNode::Element(el) => {
                render_element(el, lines, state);
            },
        }
    }
}

fn render_element(el: &HtmlElement, lines: &mut Vec<RenderedLine>, state: &mut RenderState) {
    match el.tag.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = el
                .tag
                .chars()
                .nth(1)
                .unwrap_or('1')
                .to_digit(10)
                .unwrap_or(1) as u8;
            let old_heading = state.heading;
            let old_level = state.heading_level;
            state.heading = true;
            state.heading_level = level;
            render_nodes(&el.children, lines, state);
            state.heading = old_heading;
            state.heading_level = old_level;
            lines.push(RenderedLine {
                text: String::new(),
                color: Color::from_u32(0xABB2BF),
                is_bold: false,
                is_italic: false,
                is_heading: false,
                heading_level: 0,
                is_link: false,
                link_url: None,
                indent: 0,
                is_horizontal_rule: false,
                is_image: false,
                image_alt: String::new(),
            });
        },
        "p" => {
            if !lines.is_empty() {
                lines.push(RenderedLine {
                    text: String::new(),
                    color: Color::from_u32(0xABB2BF),
                    is_bold: false,
                    is_italic: false,
                    is_heading: false,
                    heading_level: 0,
                    is_link: false,
                    link_url: None,
                    indent: 0,
                    is_horizontal_rule: false,
                    is_image: false,
                    image_alt: String::new(),
                });
            }
            render_nodes(&el.children, lines, state);
            lines.push(RenderedLine {
                text: String::new(),
                color: Color::from_u32(0xABB2BF),
                is_bold: false,
                is_italic: false,
                is_heading: false,
                heading_level: 0,
                is_link: false,
                link_url: None,
                indent: 0,
                is_horizontal_rule: false,
                is_image: false,
                image_alt: String::new(),
            });
        },
        "b" | "strong" => {
            let old = state.bold;
            state.bold = true;
            render_nodes(&el.children, lines, state);
            state.bold = old;
        },
        "i" | "em" => {
            let old = state.italic;
            state.italic = true;
            render_nodes(&el.children, lines, state);
            state.italic = old;
        },
        "u" => {
            render_nodes(&el.children, lines, state);
        },
        "a" => {
            let old_link = state.link_url.clone();
            state.link_url = el.get_attr("href");
            render_nodes(&el.children, lines, state);
            state.link_url = old_link;
        },
        "br" => {
            lines.push(RenderedLine {
                text: String::new(),
                color: Color::from_u32(0xABB2BF),
                is_bold: false,
                is_italic: false,
                is_heading: false,
                heading_level: 0,
                is_link: false,
                link_url: None,
                indent: 0,
                is_horizontal_rule: false,
                is_image: false,
                image_alt: String::new(),
            });
        },
        "hr" => {
            lines.push(RenderedLine {
                text: String::new(),
                color: Color::from_u32(0xABB2BF),
                is_bold: false,
                is_italic: false,
                is_heading: false,
                heading_level: 0,
                is_link: false,
                link_url: None,
                indent: 0,
                is_horizontal_rule: true,
                is_image: false,
                image_alt: String::new(),
            });
        },
        "ul" => {
            let old_list = state.in_list;
            let old_ordered = state.list_ordered;
            state.in_list = true;
            state.list_ordered = false;
            render_nodes(&el.children, lines, state);
            state.in_list = old_list;
            state.list_ordered = old_ordered;
        },
        "ol" => {
            let old_list = state.in_list;
            let old_ordered = state.list_ordered;
            let old_counter = state.list_counter;
            state.in_list = true;
            state.list_ordered = true;
            state.list_counter = 1;
            render_nodes(&el.children, lines, state);
            state.in_list = old_list;
            state.list_ordered = old_ordered;
            state.list_counter = old_counter;
        },
        "li" => {
            let prefix = if state.list_ordered {
                let mut s = String::new();
                push_u64(&mut s, state.list_counter as u64);
                s.push_str(". ");
                state.list_counter += 1;
                s
            } else {
                String::from("• ")
            };
            let old_indent = state.indent;
            state.indent += 2;
            render_nodes(&el.children, lines, state);
            if let Some(last) = lines.last_mut() {
                if !last.text.is_empty() {
                    last.text = format!("{}{}", prefix, last.text);
                }
            }
            state.indent = old_indent;
        },
        "img" => {
            let alt = el
                .get_attr("alt")
                .unwrap_or_else(|| String::from("[Image]"));
            lines.push(RenderedLine {
                text: alt.clone(),
                color: Color::from_u32(0xC678DD),
                is_bold: false,
                is_italic: false,
                is_heading: false,
                heading_level: 0,
                is_link: false,
                link_url: None,
                indent: 0,
                is_horizontal_rule: false,
                is_image: true,
                image_alt: alt,
            });
        },
        "pre" | "code" => {
            let old_pre = state.in_pre;
            state.in_pre = true;
            render_nodes(&el.children, lines, state);
            state.in_pre = old_pre;
        },
        "div" | "span" | "section" | "article" | "header" | "footer" | "nav" | "main" => {
            render_nodes(&el.children, lines, state);
        },
        "title" => {
            // Title goes to first line as heading
            let mut title_state = state.clone();
            title_state.heading = true;
            title_state.heading_level = 1;
            render_nodes(&el.children, lines, &mut title_state);
        },
        "script" | "style" | "head" | "meta" | "link" => {
            // Skip
        },
        "table" => {
            render_table(el, lines, state);
        },
        _ => {
            render_nodes(&el.children, lines, state);
        },
    }
}

fn render_table(el: &HtmlElement, lines: &mut Vec<RenderedLine>, state: &mut RenderState) {
    for child in &el.children {
        if let HtmlNode::Element(row) = child {
            if row.tag == "tr" {
                let mut cell_texts = Vec::new();
                for cell in &row.children {
                    if let HtmlNode::Element(td) = cell {
                        if td.tag == "td" || td.tag == "th" {
                            let mut cell_lines = Vec::new();
                            render_nodes(&td.children, &mut cell_lines, state);
                            let text = cell_lines
                                .iter()
                                .map(|l| l.text.as_str())
                                .collect::<Vec<_>>()
                                .join(" ");
                            cell_texts.push(text);
                        }
                    }
                }
                lines.push(RenderedLine {
                    text: format!("| {} |", cell_texts.join(" | ")),
                    color: Color::from_u32(0xABB2BF),
                    is_bold: false,
                    is_italic: false,
                    is_heading: false,
                    heading_level: 0,
                    is_link: false,
                    link_url: None,
                    indent: 0,
                    is_horizontal_rule: false,
                    is_image: false,
                    image_alt: String::new(),
                });
            }
        }
    }
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&copy;", "(c)")
        .replace("&reg;", "(R)")
}

fn push_u64(s: &mut String, mut v: u64) {
    if v == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut len = 0usize;
    while v > 0 {
        buf[len] = (v % 10) as u8;
        v /= 10;
        len += 1;
    }
    for i in (0..len).rev() {
        s.push((b'0' + buf[i]) as char);
    }
}

/// Рендерит HTML напрямую в строку (для простого отображения)
pub fn html_to_text(html: &str) -> String {
    let nodes = parse_html(html);
    let rendered = render_dom(&nodes);
    let mut result = String::new();
    for line in rendered {
        if line.is_horizontal_rule {
            result.push_str("────────────────────────────────────\n");
        } else if line.is_image {
            result.push_str(&format!("[🖼 {}]\n", line.image_alt));
        } else if !line.text.is_empty() {
            let indent = "  ".repeat(line.indent);
            if line.is_heading {
                result.push_str(&indent);
                result.push_str(&line.text.to_uppercase());
                result.push('\n');
            } else {
                result.push_str(&indent);
                result.push_str(&line.text);
                result.push('\n');
            }
        } else {
            result.push('\n');
        }
    }
    result
}
