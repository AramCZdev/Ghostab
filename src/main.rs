/// Converts a file:// URL into a local filesystem path. Accepts the empty
/// authority form (file:///path) and "localhost"; any other host is rejected.
/// Windows drive paths (file:///C:/...) lose their leading slash, and
/// percent-encoded segments are decoded. Dot-segments like `.` and `..` are
/// rejected so a local page cannot traverse outside its directory tree.
fn file_path_from_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;
    let (authority, tail) = rest.split_once('/')?;
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let mut path = percent_decode(&format!("/{tail}"));
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        path.remove(0);
    }
    if path.is_empty() || path == "/" {
        return None;
    }
    if contains_path_traversal(&path) {
        return None;
    }
    Some(path)
}

fn contains_path_traversal(path: &str) -> bool {
    path.split('/').any(|part| part == "." || part == "..")
}
