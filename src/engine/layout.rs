use super::dom::{Document, Node, NodeKind};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSpan {
    pub start: usize,
    pub end: usize,
    pub href: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutBox {
    pub rect: Rect,
    pub text: Option<String>,
    pub href: Option<String>,
    pub links: Vec<LinkSpan>,
    pub image: Option<ImageBox>,
    pub rule: bool,
    pub children: Vec<LayoutBox>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBox {
    pub source: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSpec {
    pub key: String,
    pub cell_width: usize,
    pub cell_height: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

pub fn layout_document(
    document: &Document,
    viewport: Viewport,
    images: &HashMap<String, ImageSpec>,
) -> LayoutBox {
    let mut cursor = Cursor { y: 0 };
    let children = layout_children(&document.root.children, &mut cursor, viewport.width, images, None);

    LayoutBox {
        rect: Rect {
            x: 0,
            y: 0,
            width: viewport.width,
            height: cursor.y,
        },
        text: None,
        href: None,
        links: Vec::new(),
        image: None,
        rule: false,
        children,
    }
}

fn layout_children(
    nodes: &[Node],
    cursor: &mut Cursor,
    width: usize,
    images: &HashMap<String, ImageSpec>,
    link: Option<&str>,
) -> Vec<LayoutBox> {
    let mut boxes = Vec::new();

    for node in nodes {
        match &node.kind {
            NodeKind::Element(element) if element.tag_name == "head" => {}
            NodeKind::Element(element) if element.tag_name == "html" || element.tag_name == "body" => {
                boxes.extend(layout_children(&node.children, cursor, width, images, link));
            }
            NodeKind::Element(element) if element.tag_name == "a" => {
                let href = element
                    .attributes
                    .get("href")
                    .cloned()
                    .filter(|value| !value.is_empty());
                boxes.extend(layout_children(&node.children, cursor, width, images, href.as_deref()));
            }
            NodeKind::Element(element) if element.tag_name == "img" => {
                boxes.extend(layout_image(element, cursor, width, images, link));
            }
            NodeKind::Element(element) if element.tag_name == "h1" => {
                boxes.extend(layout_inline_flow(node, cursor, width, 1, true, "", link));
                cursor.y += 1;
            }
            NodeKind::Element(element)
                if matches!(element.tag_name.as_str(), "h2" | "h3" | "h4" | "h5" | "h6") =>
            {
                boxes.extend(layout_inline_flow(node, cursor, width, 1, false, "", link));
                cursor.y += 1;
            }
            NodeKind::Element(element) if element.tag_name == "p" => {
                boxes.extend(layout_inline_flow(node, cursor, width, 2, false, "", link));
                cursor.y += 1;
            }
            NodeKind::Element(element) if element.tag_name == "li" => {
                boxes.extend(layout_inline_flow(node, cursor, width, 2, false, "* ", link));
            }
            NodeKind::Element(element) if element.tag_name == "br" => {
                cursor.y += 1;
            }
            NodeKind::Element(element) if element.tag_name == "hr" => {
                boxes.push(LayoutBox {
                    rect: Rect {
                        x: 0,
                        y: cursor.y,
                        width,
                        height: 1,
                    },
                    text: None,
                    href: None,
                    links: Vec::new(),
                    image: None,
                    rule: true,
                    children: Vec::new(),
                });
                cursor.y += 2;
            }
            NodeKind::Element(_) => {
                boxes.extend(layout_children(&node.children, cursor, width, images, link));
            }
            NodeKind::Text(_) => {
                boxes.extend(layout_inline_flow(
                    node,
                    cursor,
                    width,
                    0,
                    false,
                    "",
                    link,
                ));
            }
        }
    }

    boxes
}

fn collect_inline_runs(
    node: &Node,
    out: &mut Vec<(String, Option<String>)>,
    link: Option<&str>,
) {
    match &node.kind {
        NodeKind::Text(text) => out.push((text.clone(), link.map(str::to_string))),
        NodeKind::Element(element) => {
            if element.tag_name == "a" {
                let href = element
                    .attributes
                    .get("href")
                    .cloned()
                    .filter(|value| !value.is_empty());
                for child in &node.children {
                    collect_inline_runs(child, out, href.as_deref().or(link));
                }
            } else if element.tag_name == "img" {
                let alt = element
                    .attributes
                    .get("alt")
                    .cloned()
                    .unwrap_or_else(|| "[image]".to_string());
                out.push((alt, link.map(str::to_string)));
            } else {
                for child in &node.children {
                    collect_inline_runs(child, out, link);
                }
            }
        }
    }
}

fn layout_inline_flow(
    node: &Node,
    cursor: &mut Cursor,
    width: usize,
    indent: usize,
    upper: bool,
    prefix: &str,
    link: Option<&str>,
) -> Vec<LayoutBox> {
    let mut runs = Vec::new();
    if !prefix.is_empty() {
        runs.push((prefix.to_string(), link.map(str::to_string)));
    }
    collect_inline_runs(node, &mut runs, link);
    if upper {
        for (text, _) in &mut runs {
            *text = text.to_uppercase();
        }
    }

    let content_width = width.saturating_sub(indent).max(1);
    let mut boxes = Vec::new();
    let mut line_words: Vec<(String, Option<String>)> = Vec::new();
    let mut line_len = 0usize;

    for (text, href) in runs {
        for word in text.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > content_width {
                if !line_words.is_empty() {
                    flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
                    cursor.y += 1;
                    line_len = 0;
                }
                let chars: Vec<char> = word.chars().collect();
                let mut start = 0;
                while start < chars.len() {
                    let end = usize::min(start + content_width, chars.len());
                    let fragment: String = chars[start..end].iter().collect();
                    boxes.push(LayoutBox {
                        rect: Rect {
                            x: indent,
                            y: cursor.y,
                            width: fragment.len(),
                            height: 1,
                        },
                        text: Some(fragment),
                        href: href.clone(),
                        links: span_list(0, end - start, href.as_deref()),
                        image: None,
                        rule: false,
                        children: Vec::new(),
                    });
                    cursor.y += 1;
                    start = end;
                }
                continue;
            }

            let separator = usize::from(!line_words.is_empty());
            if line_len + separator + word_len > content_width && !line_words.is_empty() {
                flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
                cursor.y += 1;
                line_len = 0;
            }
            if line_len != 0 {
                line_len += 1;
            }
            line_len += word_len;
            line_words.push((word.to_string(), href.clone()));
        }
    }

    if !line_words.is_empty() {
        flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
        cursor.y += 1;
    }

    boxes
}

fn flush_inline_line(
    boxes: &mut Vec<LayoutBox>,
    words: &mut Vec<(String, Option<String>)>,
    y: usize,
    indent: usize,
) {
    let mut text = String::new();
    let mut links: Vec<LinkSpan> = Vec::new();
    let mut offset = 0usize;
    for (word, href) in words.iter() {
        if !text.is_empty() {
            text.push(' ');
            offset += 1;
        }
        let start = offset;
        text.push_str(word);
        offset += word.len();
        if let Some(href) = href {
            if let Some(last) = links.last_mut() {
                if last.end + 1 == start && last.href == *href {
                    last.end = offset;
                    continue;
                }
            }
            links.push(LinkSpan {
                start,
                end: offset,
                href: href.clone(),
            });
        }
    }
    boxes.push(LayoutBox {
        rect: Rect {
            x: indent,
            y,
            width: text.len(),
            height: 1,
        },
        text: Some(text),
        href: None,
        links,
        image: None,
        rule: false,
        children: Vec::new(),
    });
    words.clear();
}

fn span_list(start: usize, end: usize, href: Option<&str>) -> Vec<LinkSpan> {
    match href {
        Some(href) => vec![LinkSpan {
            start,
            end,
            href: href.to_string(),
        }],
        None => Vec::new(),
    }
}

fn layout_image(
    element: &super::dom::Element,
    cursor: &mut Cursor,
    width: usize,
    images: &HashMap<String, ImageSpec>,
    link: Option<&str>,
) -> Vec<LayoutBox> {
    let src = element.attributes.get("src").cloned().unwrap_or_default();

    if let Some(spec) = images.get(&src) {
        let box_ = LayoutBox {
            rect: Rect {
                x: 0,
                y: cursor.y,
                width: spec.cell_width,
                height: spec.cell_height,
            },
            text: None,
            href: link.map(str::to_string),
            links: Vec::new(),
            image: Some(ImageBox {
                source: spec.key.clone(),
                width_px: spec.pixel_width,
                height_px: spec.pixel_height,
            }),
            rule: false,
            children: Vec::new(),
        };

        cursor.y += spec.cell_height;
        cursor.y += 1;
        return vec![box_];
    }

    let alt = element
        .attributes
        .get("alt")
        .cloned()
        .unwrap_or_else(|| "[image]".to_string());
    layout_text_block(alt, cursor, width, 0, link)
}

fn layout_text_block(
    text: String,
    cursor: &mut Cursor,
    width: usize,
    indent: usize,
    link: Option<&str>,
) -> Vec<LayoutBox> {
    let content_width = width.saturating_sub(indent).max(1);
    let mut boxes = Vec::new();

    for line in wrap_text(&text, content_width) {
        boxes.push(LayoutBox {
            rect: Rect {
                x: indent,
                y: cursor.y,
                width: line.len(),
                height: 1,
            },
            text: Some(line),
            href: link.map(str::to_string),
            links: Vec::new(),
            image: None,
            rule: false,
            children: Vec::new(),
        });
        cursor.y += 1;
    }

    boxes
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        // If the word itself is longer than the width, break it into chunks.
        if word.chars().count() > width {
            if !current.is_empty() {
                lines.push(current);
                current = String::new();
            }

            let chars: Vec<char> = word.chars().collect();
            let mut start = 0;
            while start < chars.len() {
                let end = usize::min(start + width, chars.len());
                lines.push(chars[start..end].iter().collect());
                start = end;
            }
            continue;
        }

        let separator = usize::from(!current.is_empty());
        if current.chars().count() + separator + word.chars().count() > width && !current.is_empty() {
            lines.push(current);
            current = String::new();
        }

        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

#[derive(Debug, Default)]
struct Cursor {
    y: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::parse_html;

    #[test]
    fn lays_out_heading_and_wrapped_paragraph() {
        let document = parse_html("<h1>Hello</h1><p>one two three four five</p>");
        let layout = layout_document(
            &document,
            Viewport {
                width: 12,
                height: 20,
            },
            &HashMap::new(),
        );

        assert_eq!(layout.children[0].text.as_deref(), Some("HELLO"));
        assert!(layout.children.iter().any(|box_| box_.rect.y > 1));
    }

    #[test]
    fn keeps_full_content_height_for_scrolling() {
        let document = parse_html(
            "<p>one two three four five six seven eight nine ten eleven twelve</p>",
        );
        let layout = layout_document(
            &document,
            Viewport {
                width: 10,
                height: 2,
            },
            &HashMap::new(),
        );

        assert!(layout.rect.height > 2);
    }

    #[test]
    fn lays_out_horizontal_rule() {
        let document = parse_html("<p>top</p><hr><p>bottom</p>");
        let layout = layout_document(
            &document,
            Viewport {
                width: 30,
                height: 20,
            },
            &HashMap::new(),
        );

        let rules: Vec<_> = layout
            .children
            .iter()
            .filter(|box_| box_.rule)
            .collect();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rect.width, 30);
        assert_eq!(rules[0].rect.y, 2);
        assert!(layout.children.iter().any(|box_| box_.text.as_deref() == Some("bottom")));
    }

    #[test]
    fn marks_link_text_with_href() {
        let document = parse_html("<a href=\"https://example.com\">click me</a>");
        let layout = layout_document(
            &document,
            Viewport {
                width: 40,
                height: 10,
            },
            &HashMap::new(),
        );

        assert_eq!(layout.children[0].text.as_deref(), Some("click me"));
        assert_eq!(
            layout.children[0].links[0].href,
            "https://example.com"
        );
    }

    #[test]
    fn marks_inline_link_inside_paragraph() {
        let document = parse_html(
            "<p>Try <a href=\"https://example.com\">clicking</a> here</p>",
        );
        let layout = layout_document(
            &document,
            Viewport {
                width: 40,
                height: 10,
            },
            &HashMap::new(),
        );

        let text_box = layout
            .children
            .iter()
            .find(|child| child.text.is_some())
            .expect("paragraph text box");
        assert_eq!(
            text_box.text.as_deref(),
            Some("Try clicking here")
        );
        assert_eq!(text_box.links.len(), 1);
        let span = &text_box.links[0];
        assert_eq!(span.href, "https://example.com");
        assert_eq!(&text_box.text.as_deref().unwrap()[span.start..span.end], "clicking");
    }

    #[test]
    fn reserves_space_for_images() {
        let document = parse_html("<img src=\"pic.png\" alt=\"fallback\">");
        let mut images = HashMap::new();
        images.insert(
            "pic.png".to_string(),
            ImageSpec {
                key: "/tmp/pic.png".to_string(),
                cell_width: 10,
                cell_height: 5,
                pixel_width: 80,
                pixel_height: 40,
            },
        );

        let layout = layout_document(
            &document,
            Viewport {
                width: 20,
                height: 20,
            },
            &images,
        );

        let image = &layout.children[0];
        assert!(image.image.is_some());
        assert_eq!(image.rect.height, 5);
        assert_eq!(image.image.as_ref().unwrap().source, "/tmp/pic.png");
    }

    #[test]
    fn falls_back_to_alt_text_when_image_missing() {
        let document = parse_html("<img src=\"missing.png\" alt=\"broken picture\">");
        let layout = layout_document(
            &document,
            Viewport {
                width: 40,
                height: 10,
            },
            &HashMap::new(),
        );

        assert!(layout.children[0].image.is_none());
        assert!(layout.children[0].text.as_deref().unwrap_or("").contains("broken"));
    }
}
