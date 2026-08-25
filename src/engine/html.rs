use super::dom::{Document, Node, NodeKind};
use std::collections::HashMap;

pub fn parse_html(source: &str) -> Document {
    let mut parser = Parser::new(source);
    Document::new(Node::element("document", parser.parse_nodes(None)))
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn parse_nodes(&mut self, closing_tag: Option<&str>) -> Vec<Node> {
        let mut nodes = Vec::new();

        while !self.eof() {
            self.consume_whitespace();

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

            if !is_empty_text(&node) {
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
            self.parse_nodes(Some(&tag_name))
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
        Node::text(collapse_whitespace(&decode_entities(&text)))
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
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
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
}
