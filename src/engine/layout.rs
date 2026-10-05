use super::dom::{Document, Node, NodeKind};
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

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

/// Character styling carried from inline tags to the renderer. The text
/// backend has no stylesheet cascade, so emphasis is resolved to these flags
/// while walking the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub mono: bool,
}

impl TextStyle {
    /// Fold `other`'s emphasis into `self` (used for nested inline tags).
    fn merge(self, other: TextStyle) -> TextStyle {
        TextStyle {
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            underline: self.underline || other.underline,
            strike: self.strike || other.strike,
            mono: self.mono || other.mono,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutBox {
    pub rect: Rect,
    pub text: Option<String>,
    pub href: Option<String>,
    pub links: Vec<LinkSpan>,
    pub image: Option<ImageBox>,
    pub rule: bool,
    pub style: TextStyle,
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
    let mut lists: Vec<ListFrame> = Vec::new();
    let children = layout_children(
        &document.root.children,
        &mut cursor,
        &Ctx {
            width: viewport.width,
            images,
            link: None,
            style: TextStyle::default(),
            indent: 0,
        },
        &mut lists,
    );

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
        style: TextStyle::default(),
        children,
    }
}

/// Everything inherited while descending the tree.
struct Ctx<'a> {
    width: usize,
    images: &'a HashMap<String, ImageSpec>,
    link: Option<&'a str>,
    style: TextStyle,
    /// Base indent from an enclosing blockquote/list, in cells.
    indent: usize,
}

/// One open <ul>/<ol>. `counter` is the next ordinal for ordered lists.
struct ListFrame {
    ordered: bool,
    counter: usize,
}

/// Tags that introduce a blank line after their content.
fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag,
        "div"
            | "section"
            | "article"
            | "main"
            | "header"
            | "footer"
            | "nav"
            | "aside"
            | "figure"
            | "figcaption"
            | "address"
            | "details"
            | "summary"
            | "fieldset"
            | "legend"
            | "form"
            | "center"
            | "caption"
    )
}

/// Tags that never contribute rendered text.
fn is_skipped_tag(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "script" | "style" | "noscript" | "template" | "title" | "meta" | "link"
            | "base" | "col" | "colgroup" | "thead" | "tbody" | "tfoot" | "datalist"
            | "optgroup" | "option" | "select" | "map" | "area" | "audio" | "video"
            | "canvas" | "svg" | "iframe" | "object" | "embed" | "param" | "source"
            | "track" | "wbr" | "basefont" | "bgsound" | "frame" | "frameset"
    )
}

/// Tags laid out as their own block instead of contributing inline runs.
/// Everything else (emphasis, links, unknown elements) joins the current run.
fn is_flow_tag(tag: &str) -> bool {
    matches!(
        tag,
        "html"
            | "body"
            | "br"
            | "hr"
            | "img"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "pre"
            | "listing"
            | "plaintext"
            | "textarea"
            | "ul"
            | "ol"
            | "menu"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "blockquote"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "input"
            | "button"
            | "label"
            | "select"
            | "sup"
            | "sub"
    ) || is_block_tag(tag)
}

/// Styling implied by a specific inline tag.
fn style_for_tag(tag: &str) -> TextStyle {
    match tag {
        "b" | "strong" | "th" | "dt" | "mark" => TextStyle {
            bold: true,
            ..TextStyle::default()
        },
        "i" | "em" | "cite" | "var" | "dfn" | "address" => TextStyle {
            italic: true,
            ..TextStyle::default()
        },
        "u" | "ins" => TextStyle {
            underline: true,
            ..TextStyle::default()
        },
        "s" | "strike" | "del" => TextStyle {
            strike: true,
            ..TextStyle::default()
        },
        "code" | "kbd" | "samp" | "tt" => TextStyle {
            mono: true,
            ..TextStyle::default()
        },
        _ => TextStyle::default(),
    }
}

/// How a run of inline content should be rendered by `layout_flow`.
#[derive(Debug, Default, Clone)]
struct Flow {
    /// Extra cells to indent past the inherited indent.
    indent: usize,
    /// Text inserted before the first word, e.g. a list marker.
    prefix: String,
    /// Force a style on everything in the run, e.g. a heading.
    style: Option<TextStyle>,
    /// Uppercase the run, as headings do.
    upper: bool,
}

/// Lay out a node list the way a browser flows content: inline nodes
/// accumulate into shared text runs, while block tags take a line of their
/// own and reset the run.
fn layout_children(
    nodes: &[Node],
    cursor: &mut Cursor,
    ctx: &Ctx,
    lists: &mut Vec<ListFrame>,
) -> Vec<LayoutBox> {
    layout_flow(nodes, cursor, ctx, lists, &Flow::default())
}

fn layout_flow(
    nodes: &[Node],
    cursor: &mut Cursor,
    ctx: &Ctx,
    lists: &mut Vec<ListFrame>,
    flow: &Flow,
) -> Vec<LayoutBox> {
    let mut boxes = Vec::new();
    let mut runs: Vec<(String, Option<String>, TextStyle)> = Vec::new();
    let mut pending = 0usize;

    for node in nodes {
        let element = match &node.kind {
            NodeKind::Element(element) => {
                if is_skipped_tag(&element.tag_name) {
                    continue;
                }
                Some(element)
            }
            NodeKind::Text(_) => None,
        };

        let Some(element) = element else {
            collect_inline_runs(
                node,
                &mut runs,
                ctx.link.map(str::to_string),
                flow.style.unwrap_or(ctx.style),
            );
            pending += 1;
            continue;
        };

        let tag = element.tag_name.as_str();
        if !is_flow_tag(tag) {
            // Inline or unknown: keep it in the current run, merging its
            // emphasis and picking up an <a> href along the way.
            let style = flow
                .style
                .unwrap_or(ctx.style)
                .merge(style_for_tag(tag));
            collect_inline_runs(node, &mut runs, ctx.link.map(str::to_string), style);
            pending += 1;
            continue;
        }

        flush_flow(&mut boxes, &mut runs, &mut pending, cursor, ctx, flow);
        boxes.extend(layout_element(node, element, cursor, ctx, lists));
    }

    flush_flow(&mut boxes, &mut runs, &mut pending, cursor, ctx, flow);
    boxes
}

/// Emit the accumulated inline runs, then hand the cursor to the next block.
fn flush_flow(
    boxes: &mut Vec<LayoutBox>,
    runs: &mut Vec<(String, Option<String>, TextStyle)>,
    pending: &mut usize,
    cursor: &mut Cursor,
    ctx: &Ctx,
    flow: &Flow,
) {
    if *pending == 0 {
        return;
    }
    if !flow.prefix.is_empty() {
        match runs.first_mut() {
            Some(first) => first.0.insert_str(0, &flow.prefix),
            None => runs.push((
                flow.prefix.clone(),
                ctx.link.map(str::to_string),
                flow.style.unwrap_or(ctx.style),
            )),
        }
    }
    if flow.upper {
        for (text, _, _) in runs.iter_mut() {
            *text = text.to_uppercase();
        }
    }

    let indent = ctx.indent + flow.indent;
    let width = ctx.width.saturating_sub(indent).max(1);
    boxes.extend(layout_runs_wrapped(runs, cursor, width, indent));
    runs.clear();
    *pending = 0;
}

fn layout_element(
    node: &Node,
    element: &super::dom::Element,
    cursor: &mut Cursor,
    ctx: &Ctx,
    lists: &mut Vec<ListFrame>,
) -> Vec<LayoutBox> {
    let tag = element.tag_name.as_str();

    match tag {
        "html" | "body" => layout_children(&node.children, cursor, ctx, lists),

        "img" => layout_image(element, cursor, ctx),

        "br" => {
            cursor.y += 1;
            Vec::new()
        }

        "hr" => {
            let rule = LayoutBox {
                rect: Rect {
                    x: 0,
                    y: cursor.y,
                    width: ctx.width,
                    height: 1,
                },
                text: None,
                href: None,
                links: Vec::new(),
                image: None,
                rule: true,
                style: TextStyle::default(),
                children: Vec::new(),
            };
            cursor.y += 2;
            vec![rule]
        }

        "h1" => {
            let boxes = layout_flow(
                &node.children,
                cursor,
                ctx,
                lists,
                &Flow {
                    indent: 1,
                    upper: true,
                    style: Some(TextStyle {
                        bold: true,
                        ..TextStyle::default()
                    }),
                    ..Flow::default()
                },
            );
            cursor.y += 1;
            boxes
        }
        "h2" | "h3" | "h4" | "h5" | "h6" => {
            let boxes = layout_flow(
                &node.children,
                cursor,
                ctx,
                lists,
                &Flow {
                    indent: 1,
                    style: Some(TextStyle {
                        bold: true,
                        ..TextStyle::default()
                    }),
                    ..Flow::default()
                },
            );
            cursor.y += 1;
            boxes
        }

        "p" => {
            let boxes = layout_flow(&node.children, cursor, ctx, lists, &Flow::default());
            cursor.y += 1;
            boxes
        }

        "pre" | "listing" | "plaintext" => layout_pre(node, cursor, ctx),
        "textarea" => layout_textarea(element, cursor, ctx),

        "ul" | "ol" | "menu" => layout_list(node, element, cursor, ctx, lists, tag == "ol"),

        "li" => layout_list_item(node, cursor, ctx, lists),

        "dl" => layout_children(&node.children, cursor, ctx, lists),
        "dt" => {
            let boxes = layout_flow(
                &node.children,
                cursor,
                ctx,
                lists,
                &Flow {
                    style: Some(TextStyle {
                        bold: true,
                        ..TextStyle::default()
                    }),
                    ..Flow::default()
                },
            );
            cursor.y += 1;
            boxes
        }
        "dd" => {
            let indent = ctx.indent + 4;
            let child_ctx = Ctx { indent, ..*ctx };
            let boxes = layout_children(&node.children, cursor, &child_ctx, lists);
            cursor.y += 1;
            boxes
        }

        "blockquote" => {
            let indent = ctx.indent + 4;
            let child_ctx = Ctx { indent, ..*ctx };
            let boxes = layout_children(&node.children, cursor, &child_ctx, lists);
            cursor.y += 1;
            boxes
        }

        "table" => layout_table(node, cursor, ctx),
        "tr" | "td" | "th" => layout_table_cell(node, cursor, ctx),

        "input" | "button" => layout_form_control(element, cursor, ctx),
        "label" | "select" => layout_children(&node.children, cursor, ctx, lists),

        "sup" => layout_script(node, cursor, ctx, true),
        "sub" => layout_script(node, cursor, ctx, false),

        _ => {
            // Remaining block containers just pass their children through.
            let boxes = layout_children(&node.children, cursor, ctx, lists);
            cursor.y += 1;
            boxes
        }
    }
}

/// <sup>/<sub> shift the baseline. In a fixed cell grid we approximate by
/// rendering the text normally, wrapped so it reads as an annotation.
fn layout_script(node: &Node, cursor: &mut Cursor, ctx: &Ctx, above: bool) -> Vec<LayoutBox> {
    let marker = if above { "^" } else { "_" };
    layout_inline_flow(node, cursor, ctx, 0, false, marker, None)
}

/// Build the bullet or ordinal for the current list item.
fn list_marker(lists: &[ListFrame], ordered: bool) -> String {
    if ordered {
        match lists.last() {
            Some(frame) => format!("{}. ", frame.counter),
            None => "1. ".to_string(),
        }
    } else {
        // Alternate bullet glyphs by depth so nesting is visible.
        let depth = lists.len().saturating_sub(1);
        match depth % 3 {
            0 => "* ".to_string(),
            1 => "- ".to_string(),
            _ => "\u{2022} ".to_string(),
        }
    }
}

fn layout_list(
    node: &Node,
    element: &super::dom::Element,
    cursor: &mut Cursor,
    ctx: &Ctx,
    lists: &mut Vec<ListFrame>,
    ordered: bool,
) -> Vec<LayoutBox> {
    // `start` lets authors resume numbering, as in <ol start="5">.
    let start = element
        .attributes
        .get("start")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(1);
    lists.push(ListFrame {
        ordered,
        counter: start,
    });

    let indent = ctx.indent + 2;
    let child_ctx = Ctx { indent, ..*ctx };
    let boxes = layout_children(&node.children, cursor, &child_ctx, lists);

    lists.pop();
    // A little breathing room after the list, but not after a nested one
    // whose parent continues right after it.
    if lists.is_empty() {
        cursor.y += 1;
    }
    boxes
}

fn layout_list_item(
    node: &Node,
    cursor: &mut Cursor,
    ctx: &Ctx,
    lists: &mut Vec<ListFrame>,
) -> Vec<LayoutBox> {
    let ordered = lists.last().map(|frame| frame.ordered).unwrap_or(false);
    // Read the counter before advancing it, so the first item uses `start`.
    let marker = list_marker(lists, ordered);
    if let Some(frame) = lists.last_mut() {
        frame.counter = frame.counter.saturating_add(1);
    }
    // Markers hang to the left of the item's own indent; a nested list inside
    // the item starts on the following line.
    let boxes = layout_flow(
        &node.children,
        cursor,
        ctx,
        lists,
        &Flow {
            indent: 2,
            prefix: marker,
            ..Flow::default()
        },
    );
    if !node.children.is_empty() {
        cursor.y += 1;
    }
    boxes
}

/// A <td>/<th> reached outside a <table> still needs to render its text.
fn layout_table_cell(node: &Node, cursor: &mut Cursor, ctx: &Ctx) -> Vec<LayoutBox> {
    let mut lists = Vec::new();
    layout_children(&node.children, cursor, ctx, &mut lists)
}

/// <pre>: emit the text verbatim, one box per source line, keeping leading
/// indentation. Long lines are hard-wrapped rather than reflowed.
fn layout_pre(node: &Node, cursor: &mut Cursor, ctx: &Ctx) -> Vec<LayoutBox> {
let style = TextStyle {
        mono: true,
        ..TextStyle::default()
    };
    let mut runs = Vec::new();
    collect_inline_runs(
        node,
        &mut runs,
        ctx.link.map(str::to_string),
        style,
    );
    // Join the runs back into the verbatim source text, remembering where each
    // link starts so spans survive the line split.
    let mut raw = String::new();
    let mut spans: Vec<LinkSpan> = Vec::new();
    for (text, href, _) in &runs {
        let start = raw.len();
        raw.push_str(text);
        if let Some(href) = href {
            if let Some(last) = spans.last_mut()
                && last.end == start
                && last.href == *href
            {
                last.end = raw.len();
                continue;
            }
            spans.push(LinkSpan {
                start,
                end: raw.len(),
                href: href.clone(),
            });
        }
    }

    let indent = ctx.indent;
    let available = ctx.width.saturating_sub(indent).max(1);
    let mut boxes = Vec::new();
    let mut base = 0usize;

    for segment in raw.split('\n') {
        let line = segment.strip_suffix('\r').unwrap_or(segment);
        let mut consumed = 0usize;
        for piece in hard_wrap(line, available) {
            let trimmed = piece.trim_end();
            let start = base + consumed;
            let end = start + piece.len();
            consumed += piece.len();
            let links: Vec<LinkSpan> = spans
                .iter()
                .filter_map(|span| {
                    let from = span.start.max(start);
                    let to = span.end.min(end);
                    (from < to).then(|| LinkSpan {
                        start: from - start,
                        end: to - start,
                        href: span.href.clone(),
                    })
                })
                .collect();
            boxes.push(LayoutBox {
                rect: Rect {
                    x: indent,
                    y: cursor.y,
                    width: trimmed.width(),
                    height: 1,
                },
                text: Some(trimmed.to_string()),
                href: if links.is_empty() {
                    ctx.link.map(str::to_string)
                } else {
                    None
                },
                links,
                image: None,
                rule: false,
                style,
                children: Vec::new(),
            });
            cursor.y += 1;
        }
        base += segment.len() + 1;
    }

    if boxes.is_empty() {
        cursor.y += 1;
    }
    cursor.y += 1;
    boxes
}
/// Break a line at an exact cell budget, preserving all interior spacing.
fn hard_wrap(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for ch in line.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if current_width + ch_width > width && !current.is_empty() {
            pieces.push(std::mem::take(&mut current));
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

fn layout_textarea(
    element: &super::dom::Element,
    cursor: &mut Cursor,
    ctx: &Ctx,
) -> Vec<LayoutBox> {
    let value = element.attributes.get("value").cloned().unwrap_or_default();
    if value.is_empty() {
        return Vec::new();
    }
    let style = TextStyle {
        mono: true,
        ..TextStyle::default()
    };
    let mut boxes = Vec::new();
    for line in value.split('\n') {
        boxes.push(LayoutBox {
            rect: Rect {
                x: ctx.indent,
                y: cursor.y,
                width: line.width(),
                height: 1,
            },
            text: Some(line.to_string()),
            href: None,
            links: Vec::new(),
            image: None,
            rule: false,
            style,
            children: Vec::new(),
        });
        cursor.y += 1;
    }
    boxes
}

/// Render form controls as a readable bracketed token.
fn layout_form_control(
    element: &super::dom::Element,
    cursor: &mut Cursor,
    ctx: &Ctx,
) -> Vec<LayoutBox> {
    let tag = element.tag_name.as_str();
    let kind = element
        .attributes
        .get("type")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "text".to_string());

    let label = match tag {
        "button" => element
            .attributes
            .get("value")
            .cloned()
            .unwrap_or_else(|| "[button]".to_string()),
        _ => match kind.as_str() {
            "checkbox" => "[ ]".to_string(),
            "radio" => "( )".to_string(),
            "submit" => "[submit]".to_string(),
            "password" => "[password]".to_string(),
            "hidden" => return Vec::new(),
            _ => "[text field]".to_string(),
        },
    };

    let width = label.width();
    let boxes = vec![LayoutBox {
        rect: Rect {
            x: ctx.indent,
            y: cursor.y,
            width,
            height: 1,
        },
        text: Some(label),
        href: None,
        links: Vec::new(),
        image: None,
        rule: false,
        style: TextStyle::default(),
        children: Vec::new(),
    }];
    cursor.y += 1;
    boxes
}

/// One <td>/<th>: its inline runs plus the width they need on a single line.
struct TableCell {
    runs: Vec<(String, Option<String>, TextStyle)>,
    width: usize,
}

/// Collapse the whitespace inside a cell so wrapped lines stay tidy, then
/// measure the single-line width. Returns 0 for an empty cell.
fn normalize_cell_runs(runs: &mut Vec<(String, Option<String>, TextStyle)>) -> usize {
    let mut out: Vec<(String, Option<String>, TextStyle)> = Vec::new();
    let mut words = 0usize;
    let mut width = 0usize;

    for (text, href, style) in runs.drain(..) {
        for word in text.split_whitespace() {
            if words > 0 {
                width += 1;
            }
            width += word.width();
            words += 1;
            match out.last_mut() {
                Some(last) if last.1 == href && last.2 == style => {
                    last.0.push(' ');
                    last.0.push_str(word);
                }
                _ => out.push((word.to_string(), href.clone(), style)),
            }
        }
    }

    *runs = out;
    width
}

fn layout_table(node: &Node, cursor: &mut Cursor, ctx: &Ctx) -> Vec<LayoutBox> {
    let mut rows: Vec<Vec<TableCell>> = Vec::new();
    collect_table_rows(&node.children, &mut rows);

    if rows.is_empty() {
        return Vec::new();
    }

    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }

    // Column width is the widest cell in that column, plus one space of gutter.
    let gutter = 2usize;
    let mut widths: Vec<usize> = vec![0; columns];
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            if index < columns {
                widths[index] = widths[index].max(cell.width);
            }
        }
    }

    // Shrink to fit: take cells away from the widest column first.
    let available = ctx.width.saturating_sub(ctx.indent).max(1);
    let total: usize = widths.iter().sum::<usize>() + gutter * columns.saturating_sub(1);
    if total > available {
        let mut over = total - available;
        while over > 0 {
            let Some((index, _)) = widths
                .iter()
                .enumerate()
                .max_by_key(|(_, width)| **width)
            else {
                break;
            };
            if widths[index] <= 3 {
                break;
            }
            let take = over.min(widths[index] - 3);
            widths[index] -= take;
            over -= take;
        }
        for width in widths.iter_mut() {
            *width = (*width).max(3);
        }
    }

    let mut boxes = Vec::new();
    for row in &rows {
        let row_start = cursor.y;
        for (index, cell) in row.iter().enumerate().take(columns) {
            let x = ctx.indent + widths[..index].iter().sum::<usize>() + index * gutter;
            // Cells share the row's first line, so render into a scratch cursor
            // and keep whichever column needed the most rows.
            let mut cell_cursor = Cursor { y: row_start };
            boxes.extend(layout_runs_wrapped(
                &cell.runs,
                &mut cell_cursor,
                widths[index],
                x,
            ));
            cursor.y = cursor.y.max(cell_cursor.y);
        }
        if cursor.y == row_start {
            cursor.y = row_start + 1;
        }
        // Rows after the first get a separator so columns stay readable.
        if rows.len() > 1 {
            cursor.y += 1;
        }
    }

    cursor.y += 1;
    boxes
}

fn collect_table_rows(nodes: &[Node], rows: &mut Vec<Vec<TableCell>>) {
    for node in nodes {
        let NodeKind::Element(element) = &node.kind else {
            continue;
        };
        match element.tag_name.as_str() {
            "tr" => {
                let mut row = Vec::new();
                collect_table_cells(&node.children, &mut row);
                rows.push(row);
            }
            // Row-group wrappers are transparent.
            _ => collect_table_rows(&node.children, rows),
        }
    }
}

fn collect_table_cells(nodes: &[Node], row: &mut Vec<TableCell>) {
    for node in nodes {
        let NodeKind::Element(element) = &node.kind else {
            continue;
        };
        match element.tag_name.as_str() {
            "td" | "th" => {
                // Table text is monospaced so columns line up; <th> is bold.
                let base = TextStyle {
                    mono: true,
                    ..TextStyle::default()
                }
                .merge(style_for_tag(element.tag_name.as_str()));
                let mut runs = Vec::new();
                collect_inline_runs(node, &mut runs, None, base);
                let width = normalize_cell_runs(&mut runs);
                row.push(TableCell { runs, width });
            }
            _ => collect_table_cells(&node.children, row),
        }
    }
}

/// Flatten inline content to (text, href, style) runs.
fn collect_inline_runs(
    node: &Node,
    out: &mut Vec<(String, Option<String>, TextStyle)>,
    link: Option<String>,
    style: TextStyle,
) {
    match &node.kind {
        NodeKind::Text(text) => out.push((text.clone(), link, style)),
        NodeKind::Element(element) => {
            let tag = element.tag_name.as_str();
            if is_skipped_tag(tag) {
                return;
            }
            if tag == "img" {
                let alt = element
                    .attributes
                    .get("alt")
                    .cloned()
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "[image]".to_string());
                out.push((alt, link, style));
                return;
            }
            // A block element inside inline content contributes its text, but
            // must not swallow the caller's list of runs.
            let child_link = if tag == "a" {
                element
                    .attributes
                    .get("href")
                    .cloned()
                    .filter(|value| !value.is_empty())
                    .or(link)
            } else {
                link
            };
            let child_style = style.merge(style_for_tag(tag));
            for child in &node.children {
                collect_inline_runs(child, out, child_link.clone(), child_style);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_inline_flow(
    node: &Node,
    cursor: &mut Cursor,
    ctx: &Ctx,
    indent: usize,
    upper: bool,
    prefix: &str,
    force_style: Option<TextStyle>,
) -> Vec<LayoutBox> {
    let base_style = force_style.unwrap_or(ctx.style);
    let mut runs = Vec::new();
    collect_inline_runs(node, &mut runs, ctx.link.map(str::to_string), base_style);
    if !prefix.is_empty() {
        match runs.first_mut() {
            Some(first) => first.0.insert_str(0, prefix),
            None => runs.push((prefix.to_string(), ctx.link.map(str::to_string), base_style)),
        }
    }
    if upper {
        for (text, _, _) in &mut runs {
            *text = text.to_uppercase();
        }
    }

    let total_indent = ctx.indent + indent;
    let content_width = ctx.width.saturating_sub(total_indent).max(1);
    layout_runs_wrapped(&runs, cursor, content_width, total_indent)
}

/// Wrap `runs` to `width` cells and append the boxes, advancing the cursor past
/// every emitted line. Shared by inline flow and table cells.
fn layout_runs_wrapped(
    runs: &[(String, Option<String>, TextStyle)],
    cursor: &mut Cursor,
    width: usize,
    indent: usize,
) -> Vec<LayoutBox> {
    let mut boxes = Vec::new();
    // Pending words awaiting a flush. The flag records whether the source put
    // whitespace before the word, so `</a>.` does not become `word .`.
    let mut line_words: Vec<(String, Option<String>, TextStyle, bool)> = Vec::new();
    let mut line_len = 0usize;
    // Whitespace at the end of one run separates it from the next run's first
    // word, e.g. the space in `plain <b>bold</b>`.
    let mut carried_space = false;

    for (text, href, style) in runs {
        let href = href.clone();
        let style = *style;
        let leading_space = carried_space
            || text.chars().next().is_some_and(char::is_whitespace);
        carried_space = text.chars().last().is_some_and(char::is_whitespace);
        for (position, word) in text.split_whitespace().enumerate() {
            let word_width = word.width();
            let space_before = position > 0 || leading_space;
            if word_width > width {
                // A single word wider than the column has to be hard-broken.
                if !line_words.is_empty() {
                    flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
                    cursor.y += 1;
                    line_len = 0;
                }
                for fragment in hard_wrap(word, width) {
                    let fragment_bytes = fragment.len();
                    let fragment_width = fragment.width();
                    boxes.push(LayoutBox {
                        rect: Rect {
                            x: indent,
                            y: cursor.y,
                            width: fragment_width,
                            height: 1,
                        },
                        text: Some(fragment),
                        href: href.clone(),
                        links: span_list(0, fragment_bytes, href.as_deref()),
                        image: None,
                        rule: false,
                        style,
                        children: Vec::new(),
                    });
                    cursor.y += 1;
                }
                continue;
            }

            let separator = usize::from(space_before && !line_words.is_empty());
            if line_len + separator + word_width > width && !line_words.is_empty() {
                flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
                cursor.y += 1;
                line_len = 0;
            }
            line_len += separator + word_width;
            line_words.push((word.to_string(), href.clone(), style, space_before));
        }
    }

    if !line_words.is_empty() {
        flush_inline_line(&mut boxes, &mut line_words, cursor.y, indent);
        cursor.y += 1;
    }

    boxes
}

/// Emit one box per contiguous same-style group on the line, so mixed
/// emphasis keeps its styling while still sharing a line.
fn flush_inline_line(
    boxes: &mut Vec<LayoutBox>,
    words: &mut Vec<(String, Option<String>, TextStyle, bool)>,
    y: usize,
    indent: usize,
) {
    // Split the line into maximal runs of identical style, in visual order.
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (index, (_, _, style, _)) in words.iter().enumerate() {
        let same = runs
            .last()
            .and_then(|run| run.last())
            .map(|last| words[*last].2 == *style)
            .unwrap_or(false);
        if !same {
            runs.push(Vec::new());
        }
        runs.last_mut().unwrap().push(index);
    }

    let mut cursor_x = indent;
    for (run_index, run) in runs.iter().enumerate() {
        // Leave the cell the source's whitespace asked for before this run.
        if run_index > 0 && words[run[0]].3 {
            cursor_x += 1;
        }

        let (_, first_href, style, _) = &words[run[0]];
        let style = *style;
        // When every word in the run points at the same place, the whole box
        // can carry one href; mixed runs still get per-word spans.
        let uniform_href = run
            .iter()
            .all(|index| words[*index].1 == *first_href)
            .then(|| first_href.clone())
            .flatten();

        let mut text = String::new();
        let mut links: Vec<LinkSpan> = Vec::new();
        for (position, word_index) in run.iter().enumerate() {
            if position > 0 && words[*word_index].3 {
                text.push(' ');
            }
            let (word, href, _, _) = &words[*word_index];
            let start = text.len();
            text.push_str(word);
            if let Some(href) = href {
                if let Some(last) = links.last_mut()
                    && last.end == start
                    && last.href == *href
                {
                    last.end = text.len();
                    continue;
                }
                links.push(LinkSpan {
                    start,
                    end: text.len(),
                    href: href.clone(),
                });
            }
        }

        let width = text.width();
        boxes.push(LayoutBox {
            rect: Rect {
                x: cursor_x,
                y,
                width,
                height: 1,
            },
            text: Some(text),
            href: uniform_href,
            links,
            image: None,
            rule: false,
            style,
            children: Vec::new(),
        });
        cursor_x += width;
    }
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
    ctx: &Ctx,
) -> Vec<LayoutBox> {
    let src = element.attributes.get("src").cloned().unwrap_or_default();

    if let Some(spec) = ctx.images.get(&src) {
        let box_ = LayoutBox {
            rect: Rect {
                x: ctx.indent,
                y: cursor.y,
                width: spec.cell_width,
                height: spec.cell_height,
            },
            text: None,
            href: ctx.link.map(str::to_string),
            links: Vec::new(),
            image: Some(ImageBox {
                source: spec.key.clone(),
                width_px: spec.pixel_width,
                height_px: spec.pixel_height,
            }),
            rule: false,
            style: TextStyle::default(),
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
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "[image]".to_string());
    layout_inline_flow(&Node::text(alt), cursor, ctx, 0, false, "", None)
}

#[derive(Debug, Default)]
struct Cursor {
    y: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::parse_html;

    fn layout_of(html: &str, width: usize) -> LayoutBox {
        let document = parse_html(html);
        layout_document(
            &document,
            Viewport {
                width,
                height: 40,
            },
            &HashMap::new(),
        )
    }

    fn texts(layout: &LayoutBox) -> Vec<String> {
        layout
            .children
            .iter()
            .filter_map(|child| child.text.clone())
            .collect()
    }

    #[test]
    fn lays_out_heading_and_wrapped_paragraph() {
        let layout = layout_of("<h1>Hello</h1><p>one two three four five</p>", 12);
        assert_eq!(layout.children[0].text.as_deref(), Some("HELLO"));
        assert!(layout.children.iter().any(|b| b.rect.y > 1));
    }

    #[test]
    fn keeps_full_content_height_for_scrolling() {
        let layout = layout_of("<p>one two three four five six seven eight nine ten eleven twelve</p>", 10);
        assert!(layout.rect.height > 2);
    }

    #[test]
    fn lays_out_horizontal_rule() {
        let layout = layout_of("<p>top</p><hr><p>bottom</p>", 30);
        let rules: Vec<_> = layout.children.iter().filter(|b| b.rule).collect();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rect.width, 30);
        assert_eq!(rules[0].rect.y, 2);
        assert!(layout.children.iter().any(|b| b.text.as_deref() == Some("bottom")));
    }

    #[test]
    fn marks_link_text_with_href() {
        let layout = layout_of("<a href=\"https://example.com\">click me</a>", 40);
        assert_eq!(layout.children[0].text.as_deref(), Some("click me"));
        assert_eq!(layout.children[0].links[0].href, "https://example.com");
    }

    #[test]
    fn marks_inline_link_inside_paragraph() {
        let layout = layout_of("<p>Try <a href=\"https://example.com\">clicking</a> here</p>", 40);
        let text_box = layout
            .children
            .iter()
            .find(|c| c.text.is_some())
            .expect("paragraph text box");
        assert_eq!(text_box.text.as_deref(), Some("Try clicking here"));
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
            Viewport { width: 20, height: 20 },
            &images,
        );
        let image = &layout.children[0];
        assert!(image.image.is_some());
        assert_eq!(image.rect.height, 5);
        assert_eq!(image.image.as_ref().unwrap().source, "/tmp/pic.png");
    }

    #[test]
    fn falls_back_to_alt_text_when_image_missing() {
        let layout = layout_of("<img src=\"missing.png\" alt=\"broken picture\">", 40);
        assert!(layout.children[0].image.is_none());
        assert!(layout.children[0].text.as_deref().unwrap_or("").contains("broken"));
    }

    #[test]
    fn wraps_emoji_at_correct_display_width() {
        let layout = layout_of("<p>\u{1F600}\u{1F601}\u{1F602}\u{1F600}</p>", 4);
        assert_eq!(layout.children[0].rect.y, 0);
    }

    #[test]
    fn hard_wrap_handles_emoji_correctly() {
        assert_eq!(hard_wrap("\u{1F600}\u{1F601}\u{1F602}\u{1F603}", 4).len(), 2);
    }

    #[test]
    fn hard_wrap_handles_mixed_text_and_emoji() {
        assert!(!hard_wrap("hello \u{1F600} world", 12).is_empty());
    }

    // ---- new coverage ----

    #[test]
    fn headings_are_bold() {
        let layout = layout_of("<h1>Hi</h1>", 20);
        assert!(layout.children[0].style.bold);
    }

    #[test]
    fn unordered_list_gets_bullets() {
        let layout = layout_of("<ul><li>one</li><li>two</li></ul>", 40);
        let lines = texts(&layout);
        assert_eq!(lines, vec!["* one".to_string(), "* two".to_string()]);
    }

    #[test]
    fn ordered_list_numbers_sequentially() {
        let layout = layout_of("<ol><li>a</li><li>b</li><li>c</li></ol>", 40);
        let lines = texts(&layout);
        assert_eq!(lines, vec!["1. a", "2. b", "3. c"]);
    }

    #[test]
    fn ordered_list_honours_start_attribute() {
        let layout = layout_of("<ol start=\"5\"><li>a</li><li>b</li></ol>", 40);
        assert_eq!(texts(&layout), vec!["5. a", "6. b"]);
    }

    #[test]
    fn nested_lists_indent_further_and_change_bullet() {
        let layout = layout_of("<ul><li>top<ul><li>deep</li></ul></li></ul>", 40);
        let deep = layout
            .children
            .iter()
            .find(|b| b.text.as_deref().map(|t| t.contains("deep")).unwrap_or(false))
            .expect("nested item");
        let top = layout
            .children
            .iter()
            .find(|b| b.text.as_deref().map(|t| t.contains("top")).unwrap_or(false))
            .expect("outer item");
        assert!(deep.rect.x > top.rect.x, "nested should indent more");
        assert!(deep.text.as_deref().unwrap().starts_with("- "));
    }

    #[test]
    fn list_items_keep_link_targets() {
        let layout = layout_of("<ul><li><a href=\"https://example.com\">go</a></li></ul>", 40);
        let item = layout
            .children
            .iter()
            .find(|b| b.text.as_deref().map(|t| t.contains("go")).unwrap_or(false))
            .expect("item");
        assert!(item
            .links
            .iter()
            .any(|span| span.href == "https://example.com"));
    }

    #[test]
    fn bold_and_italic_split_into_styled_runs() {
        let layout = layout_of("<p>plain <b>bold</b> <i>it</i></p>", 40);
        let bold = layout
            .children
            .iter()
            .find(|b| b.style.bold)
            .expect("bold run");
        assert_eq!(bold.text.as_deref(), Some("bold"));
        let italic = layout
            .children
            .iter()
            .find(|b| b.style.italic)
            .expect("italic run");
        assert_eq!(italic.text.as_deref(), Some("it"));
    }

    #[test]
    fn underline_and_strike_are_flagged() {
        let layout = layout_of("<p><u>u</u><s>s</s></p>", 40);
        assert!(layout.children.iter().any(|b| b.style.underline));
        assert!(layout.children.iter().any(|b| b.style.strike));
    }

    #[test]
    fn code_is_monospaced() {
        let layout = layout_of("<p><code>x = 1</code></p>", 40);
        assert!(layout.children.iter().any(|b| b.style.mono));
    }

    #[test]
    fn pre_preserves_leading_indentation_and_blank_lines() {
        let layout = layout_of("<pre>  indented\n\n    deeper</pre>", 40);
        let lines = texts(&layout);
        assert_eq!(lines[0], "  indented");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "    deeper");
        assert!(layout.children.iter().all(|b| b.style.mono));
    }

    #[test]
    fn pre_long_lines_hard_wrap_without_reflowing() {
        let layout = layout_of("<pre>aaaa bbbb cccc</pre>", 6);
        let lines = texts(&layout);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.width() <= 6));
    }

    #[test]
    fn table_cells_line_up_in_columns() {
        let layout = layout_of(
            "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Ada</td><td>36</td></tr></table>",
            40,
        );
        let cells: Vec<_> = layout.children.iter().filter(|b| b.text.is_some()).collect();
        assert!(cells.len() >= 4, "expected 4 cells, got {}", cells.len());
        // Header and body cells in the same column share an x offset.
        let name_cells: Vec<_> = cells
            .iter()
            .filter(|c| c.text.as_deref() == Some("Name") || c.text.as_deref() == Some("Ada"))
            .collect();
        assert_eq!(name_cells[0].rect.x, name_cells[1].rect.x);
        // Different columns do not.
        let age = cells.iter().find(|c| c.text.as_deref() == Some("Age")).unwrap();
        assert_ne!(age.rect.x, name_cells[0].rect.x);
    }

    #[test]
    fn table_cells_are_marked_bold_for_headers() {
        let layout = layout_of("<table><tr><th>H</th></tr><tr><td>D</td></tr></table>", 30);
        assert!(layout.children.iter().any(|b| b.text.as_deref() == Some("H") && b.style.bold));
        assert!(layout.children.iter().any(|b| b.text.as_deref() == Some("D") && !b.style.bold));
    }

    #[test]
    fn tables_shrink_to_fit_narrow_viewports() {
        let layout = layout_of(
            "<table><tr><td>alpha</td><td>bravo</td><td>charlie</td></tr></table>",
            16,
        );
        let rightmost = layout
            .children
            .iter()
            .filter(|b| b.text.is_some())
            .map(|b| b.rect.x + b.rect.width)
            .max()
            .unwrap_or(0);
        assert!(rightmost <= 16, "table overflowed to {rightmost}");
    }

    #[test]
    fn blockquote_indents_its_content() {
        let layout = layout_of("<blockquote>quoted</blockquote>", 40);
        let quoted = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("quoted"))
            .expect("quote");
        assert!(quoted.rect.x >= 4);
    }

    #[test]
    fn definition_lists_bold_terms_and_indent_definitions() {
        let layout = layout_of("<dl><dt>Term</dt><dd>Meaning</dd></dl>", 40);
        let term = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("Term"))
            .expect("term");
        let meaning = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("Meaning"))
            .expect("definition");
        assert!(term.style.bold);
        assert!(meaning.rect.x > term.rect.x);
    }

    #[test]
    fn form_controls_render_as_tokens() {
        let layout = layout_of("<form><input type=\"checkbox\"><input type=\"text\"></form>", 40);
        let joined = texts(&layout).join("|");
        assert!(joined.contains("[ ]"), "got {joined}");
        assert!(joined.contains("[text field]"), "got {joined}");
    }

    #[test]
    fn hidden_inputs_are_skipped() {
        let layout = layout_of("<form><input type=\"hidden\" value=\"x\"></form>", 40);
        assert!(texts(&layout).is_empty());
    }

    #[test]
    fn nested_inline_styles_merge() {
        let layout = layout_of("<p><b>bold <i>both</i></b></p>", 40);
        let both = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("both"))
            .expect("merged run");
        assert!(both.style.bold && both.style.italic);
    }

    #[test]
    fn inline_nodes_share_one_line_inside_a_block() {
        let layout = layout_of("<div>plain <b>bold</b> and <i>italic</i> tail</div>", 40);
        let first = layout
            .children
            .iter()
            .filter_map(|child| child.text.clone())
            .next()
            .expect("first run");
        assert_eq!(first, "plain");
        assert_eq!(layout.children[1].rect.y, 0);
        assert_eq!(layout.children[2].rect.y, 0);
        assert_eq!(layout.children[3].rect.y, 0);
    }

    #[test]
    fn styled_runs_do_not_overlap_on_one_line() {
        let layout = layout_of("<p>plain <b>bold</b> tail</p>", 40);
        let plain = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("plain"))
            .expect("plain run");
        let bold = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("bold"))
            .expect("bold run");
        assert_eq!(plain.rect.y, bold.rect.y);
        assert_eq!(bold.rect.x, plain.rect.x + plain.rect.width + 1);
    }

    #[test]
    fn block_inside_a_list_item_starts_its_own_line() {
        let layout = layout_of("<ul><li>first<p>para</p></li></ul>", 40);
        let item = layout
            .children
            .iter()
            .find(|b| b.text.as_deref().map(|t| t.contains("first")).unwrap_or(false))
            .expect("item");
        let para = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("para"))
            .expect("nested paragraph");
        assert_eq!(item.text.as_deref(), Some("* first"));
        assert!(para.rect.y > item.rect.y);
    }

    #[test]
    fn table_cells_keep_link_targets() {
        let layout = layout_of(
            "<table><tr><td><a href=\"https://example.com\">go</a></td><td>x</td></tr></table>",
            40,
        );
        let cell = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("go"))
            .expect("link cell");
        assert!(cell
            .links
            .iter()
            .any(|span| span.href == "https://example.com"));
    }

    #[test]
    fn table_rows_grow_to_their_tallest_cell() {
        let layout = layout_of(
            "<table><tr><td>a very long cell value here</td><td>short</td></tr></table>",
            40,
        );
        let left = layout
            .children
            .iter()
            .find(|b| b.text.as_deref().map(|t| t.starts_with("a very")).unwrap_or(false))
            .expect("wrapped cell");
        let right = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("short"))
            .expect("short cell");
        assert_eq!(left.rect.y, right.rect.y);
    }

    #[test]
    fn pre_keeps_link_spans_across_lines() {
        let layout = layout_of("<pre>see\n<a href=\"https://example.com\">docs</a></pre>", 40);
        let link = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("docs"))
            .expect("linked line");
        assert_eq!(link.links.len(), 1);
        assert_eq!(link.links[0].start, 0);
        assert_eq!(link.links[0].end, 4);
    }

    #[test]
    fn text_after_a_link_keeps_its_punctuation_attached() {
        let layout = layout_of("<p>see <a href=\"https://example.com\">this</a>.</p>", 40);
        let box_ = layout
            .children
            .iter()
            .find(|b| b.text.as_deref() == Some("see this."))
            .expect("joined line");
        assert_eq!(box_.links.len(), 1);
        assert_eq!(box_.links[0].start, 4);
        assert_eq!(box_.links[0].end, 8);
    }

    #[test]
    fn whitespace_between_inline_elements_is_kept() {
        let layout = layout_of("<p><i>a</i> <b>b</b></p>", 40);
        assert_eq!(texts(&layout), vec!["a", "b"]);
        assert_eq!(layout.children[0].rect.x + layout.children[0].rect.width + 1, layout.children[1].rect.x);
    }

    #[test]
    fn adjacent_inline_elements_have_no_gap() {
        let layout = layout_of("<p><i>a</i><b>b</b></p>", 40);
        assert_eq!(
            layout.children[0].rect.x + layout.children[0].rect.width,
            layout.children[1].rect.x
        );
    }
}

