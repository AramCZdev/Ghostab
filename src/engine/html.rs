use super::dom::{Document, Node, NodeKind};
use std::collections::HashMap;

pub fn parse_html(source: &str) -> Document {
    let mut parser = Parser::new(source);
    Document::new(Node::element("document", parser.parse_nodes(None)))
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
    /// Depth of open <pre>/<textarea> elements. Inside them, whitespace is
    /// significant and must survive instead of being collapsed.
    preformatted: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            position: 0,
            preformatted: 0,
        }
    }

    fn parse_nodes(&mut self, closing_tag: Option<&str>) -> Vec<Node> {
        let mut nodes = Vec::new();

        while !self.eof() {
            if self.starts_with("<!--") {
                self.consume_comment();
                continue;
            }

            if self.starts_with("<!") {
                self.consume_declaration();
                continue;
            }

            if self.starts_with("</") {
                let tag_name = self.consume_closing_tag();
                if closing_tag.is_none() || closing_tag == Some(tag_name.as_str()) {
                    break;
                }
                continue;
            }

            if self.eof() {
                break;
            }

            let node = if self.next_char() == '<' {
                self.parse_element()
            } else {
                self.parse_text()
            };

            // Whitespace between elements is kept: collapsed to one space it
            // still separates inline content, and block layout ignores it.
            if !is_empty_text(&node) || is_text(&node) {
                nodes.push(node);
            }
        }

        nodes
    }

    fn parse_element(&mut self) -> Node {
        self.consume_char();
        let tag_name = self.consume_tag_name();
        let attributes = self.parse_attributes();
        let self_closing = !self.eof() && self.next_char() == '/';
        if self_closing {
            self.consume_char();
        }
        self.skip_until('>');
        if !self.eof() {
            self.consume_char();
        }

        let children = if self_closing {
            Vec::new()
        } else if is_ignored_content_element(&tag_name) {
            self.consume_until_closing_tag(&tag_name);
            Vec::new()
        } else if is_void_element(&tag_name) {
            Vec::new()
        } else {
            if is_preformatted_element(&tag_name) {
                self.preformatted += 1;
            }
            let children = self.parse_nodes(Some(&tag_name));
            if is_preformatted_element(&tag_name) {
                self.preformatted = self.preformatted.saturating_sub(1);
            }
            children
        };

        Node::element_with_attributes(tag_name, attributes, children)
    }

    fn parse_attributes(&mut self) -> HashMap<String, String> {
        let mut attributes = HashMap::new();

        loop {
            self.consume_whitespace();
            if self.eof() || self.starts_with(">") || self.starts_with("/>") {
                break;
            }

            let name = self.consume_while(|ch| !ch.is_whitespace() && ch != '=' && ch != '>' && ch != '/');
            if name.is_empty() {
                self.consume_char();
                continue;
            }

            let value = self.parse_attribute_value();
            attributes.insert(name.to_ascii_lowercase(), value);
        }

        attributes
    }

    fn parse_attribute_value(&mut self) -> String {
        self.consume_whitespace();
        if self.eof() || self.next_char() != '=' {
            return String::new();
        }

        self.consume_char();

        self.consume_whitespace();
        if self.eof() {
            return String::new();
        }

        let quote = self.next_char();
        if quote == '"' || quote == '\'' {
            self.consume_char();
            let value = self.consume_while(|ch| ch != quote);
            if !self.eof() {
                self.consume_char();
            }
            decode_entities(&value)
        } else {
            decode_entities(&self.consume_while(|ch| !ch.is_whitespace() && ch != '>'))
        }
    }

    fn parse_text(&mut self) -> Node {
        let text = self.consume_while(|ch| ch != '<');
        // Inside <pre>, runs of spaces and newlines are meaningful.
        if self.preformatted > 0 {
            Node::text(decode_entities(&text))
        } else {
            Node::text(collapse_whitespace(&decode_entities(&text)))
        }
    }

    fn consume_closing_tag(&mut self) -> String {
        self.consume_char();
        self.consume_char();
        let tag_name = self.consume_tag_name();
        self.skip_until('>');
        if !self.eof() {
            self.consume_char();
        }
        tag_name
    }

    fn consume_comment(&mut self) {
        self.position += "<!--".len();
        while !self.eof() && !self.starts_with("-->") {
            self.consume_char();
        }
        if self.starts_with("-->") {
            self.position += "-->".len();
        }
    }

    fn consume_declaration(&mut self) {
        self.consume_char();
        self.consume_char();
        self.skip_until('>');
        if !self.eof() {
            self.consume_char();
        }
    }

    fn consume_until_closing_tag(&mut self, tag_name: &str) {
        let closing = format!("</{tag_name}");
        while !self.eof() {
            let rest = &self.input[self.position..];
            if rest.len() >= closing.len()
                && rest[..closing.len()].eq_ignore_ascii_case(&closing)
            {
                break;
            }
            self.consume_char();
        }
        if !self.eof() {
            self.consume_closing_tag();
        }
    }

    fn consume_tag_name(&mut self) -> String {
        self.consume_while(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
            .to_ascii_lowercase()
    }

    fn consume_whitespace(&mut self) {
        self.consume_while(char::is_whitespace);
    }

    fn skip_until(&mut self, target: char) {
        self.consume_while(|ch| ch != target);
    }

    fn consume_while(&mut self, test: impl Fn(char) -> bool) -> String {
        let mut result = String::new();
        while !self.eof() && test(self.next_char()) {
            result.push(self.consume_char());
        }
        result
    }

    fn consume_char(&mut self) -> char {
        let ch = self.next_char();
        self.position += ch.len_utf8();
        ch
    }

    fn next_char(&self) -> char {
        self.input[self.position..].chars().next().unwrap_or('\0')
    }

    fn starts_with(&self, pattern: &str) -> bool {
        self.input[self.position..].starts_with(pattern)
    }

    fn eof(&self) -> bool {
        self.position >= self.input.len()
    }
}

fn is_empty_text(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Text(text) if text.trim().is_empty())
}

fn is_text(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Text(_))
}

/// Elements whose whitespace is significant.
fn is_preformatted_element(tag_name: &str) -> bool {
    matches!(tag_name, "pre" | "textarea" | "listing" | "plaintext")
}

fn is_void_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "br" | "hr" | "img" | "input" | "meta" | "link" | "area" | "base" | "col" | "embed"
            | "param" | "source" | "track" | "wbr"
    )
}

fn is_ignored_content_element(tag_name: &str) -> bool {
    matches!(tag_name, "script" | "style" | "noscript" | "template")
}

fn collapse_whitespace(text: &str) -> String {
    // Runs of whitespace become one space, but a space that touched either end
    // of the text node has to survive: it is the only thing separating
    // `plain <b>bold</b>` or marking that `</a>` was followed by a space.
    if text.trim().is_empty() {
        return if text.is_empty() {
            String::new()
        } else {
            " ".to_string()
        };
    }
    let mut out = String::new();
    if text.starts_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    if text.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        match decode_one_entity(rest) {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            // A bare '&' that starts nothing valid stays literal.
            None => {
                out.push('&');
                rest = &rest['&'.len_utf8()..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decode one entity starting at `text` (which begins with '&').
/// Returns the character and how many bytes to skip, or None if invalid.
fn decode_one_entity(text: &str) -> Option<(char, usize)> {
    // Entities are short in practice; cap the scan so stray '&' followed by
    // lots of prose is not rescanned repeatedly.
    let limit = text.len().min(34);
    let body_end = text[..limit].find(';')?;
    let body = &text[1..body_end];
    let total = body_end + 1;

    // Numeric: &#1234; or &#x1F600;
    if let Some(digits) = body.strip_prefix('#') {
        let code = if let Some(hex) = digits.strip_prefix('x').or_else(|| digits.strip_prefix('X')) {
            if hex.is_empty() {
                return None;
            }
            u32::from_str_radix(hex, 16).ok()?
        } else {
            if digits.is_empty() {
                return None;
            }
            digits.parse::<u32>().ok()?
        };
        // Reject out-of-range and surrogate values rather than emitting junk.
        let ch = char::from_u32(code).filter(|c| !is_surrogate(*c))?;
        return Some((ch, total));
    }

    let ch = named_entity(body)?;
    Some((ch, total))
}

fn is_surrogate(ch: char) -> bool {
    let code = ch as u32;
    (0xD800..=0xDFFF).contains(&code)
}

/// The common named entities. Rendering a space for &nbsp; rather than U+00A0
/// keeps widths predictable in a monospace grid.
fn named_entity(name: &str) -> Option<char> {
    let ch = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ensp" | "emsp" | "thinsp" => ' ',
        "copy" => '\u{a9}',
        "reg" => '\u{ae}',
        "trade" => '\u{2122}',
        "hellip" => '\u{2026}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "sbquo" => '\u{201a}',
        "bdquo" => '\u{201e}',
        "prime" => '\u{2032}',
        "Prime" => '\u{2033}',
        "bull" => '\u{2022}',
        "middot" => '\u{b7}',
        "deg" => '\u{b0}',
        "plusmn" => '\u{b1}',
        "times" => '\u{d7}',
        "divide" => '\u{f7}',
        "frac12" => '\u{bd}',
        "frac14" => '\u{bc}',
        "frac34" => '\u{be}',
        "laquo" => '\u{ab}',
        "raquo" => '\u{bb}',
        "euro" => '\u{20ac}',
        "pound" => '\u{a3}',
        "yen" => '\u{a5}',
        "cent" => '\u{a2}',
        "sect" => '\u{a7}',
        "para" => '\u{b6}',
        "dagger" => '\u{2020}',
        "larr" | "leftarrow" => '\u{2190}',
        "rarr" | "rightarrow" => '\u{2192}',
        "harr" | "leftrightarrow" => '\u{2194}',
        "lArr" | "Leftarrow" => '\u{21d0}',
        "rArr" | "Rightarrow" => '\u{21d2}',
        "ne" => '\u{2260}',
        "le" | "leq" => '\u{2264}',
        "ge" | "geq" => '\u{2265}',
        "sim" => '\u{223c}',
        "infin" => '\u{221e}',
        "micro" => '\u{b5}',
        "sup2" => '\u{b2}',
        "sup3" => '\u{b3}',
        "frac13" => '\u{2153}',
        "frac23" => '\u{2154}',
        "permil" | "pertenk" => '\u{2030}',
        _ => return None,
    };
    Some(ch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dom::NodeKind;

    #[test]
    fn parses_nested_elements_and_text() {
        let document = parse_html("<body><h1>Hello</h1><p>small &amp; steady</p></body>");
        let body = &document.root.children[0];

        assert_eq!(body.children.len(), 2);
        assert_eq!(
            &body.children[0].children[0].kind,
            &NodeKind::Text("Hello".to_string())
        );
        assert_eq!(
            &body.children[1].children[0].kind,
            &NodeKind::Text("small & steady".to_string())
        );
    }

    #[test]
    fn parses_element_attributes() {
        let document = parse_html(
            "<a href=\"https://example.com\" target='_blank'>go</a><img src=pic.png alt=\"Pic\">",
        );

        let link = &document.root.children[0];
        if let NodeKind::Element(element) = &link.kind {
            assert_eq!(element.tag_name, "a");
            assert_eq!(element.attributes.get("href").unwrap(), "https://example.com");
            assert_eq!(element.attributes.get("target").unwrap(), "_blank");
        } else {
            panic!("expected element");
        }

        let image = &document.root.children[1];
        if let NodeKind::Element(element) = &image.kind {
            assert_eq!(element.attributes.get("src").unwrap(), "pic.png");
            assert_eq!(element.attributes.get("alt").unwrap(), "Pic");
        } else {
            panic!("expected element");
        }
    }

    #[test]
    fn decodes_named_entities() {
        assert_eq!(named_entity("amp"), Some('&'));
        assert_eq!(named_entity("mdash"), Some('\u{2014}'));
        assert_eq!(named_entity("hellip"), Some('\u{2026}'));
        assert_eq!(named_entity("nope"), None);
    }

    #[test]
    fn curly_quotes_are_distinct() {
        assert_eq!(named_entity("lsquo"), Some('\u{2018}'));
        assert_eq!(named_entity("rsquo"), Some('\u{2019}'));
        assert_eq!(named_entity("ldquo"), Some('\u{201c}'));
        assert_eq!(named_entity("rdquo"), Some('\u{201d}'));
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(named_entity("ne"), Some('\u{2260}'));
        assert_eq!(decode_one_entity("&#8212;rest"), Some(('\u{2014}', 7)));
        assert_eq!(decode_one_entity("&#x2014;rest"), Some(('\u{2014}', 8)));
    }
}
