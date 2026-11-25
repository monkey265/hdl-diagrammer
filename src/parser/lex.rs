/// Remove `--` comments; VHDL strings are `"..."` (a `--` inside one is kept).
pub fn strip_comments_vhdl(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_str = false;
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        if in_str {
            out.push(c);
            if c == '"' { in_str = false; }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '-' && it.peek() == Some(&'-') {
            while let Some(&n) = it.peek() {
                if n == '\n' { break; }
                it.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Remove `//` and `/* */` comments, keeping newlines so line numbers survive.
pub fn strip_comments_sv(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_str = false;
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(n) = it.next() { out.push(n); }
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && it.peek() == Some(&'/') {
            while let Some(&n) = it.peek() {
                if n == '\n' { break; }
                it.next();
            }
        } else if c == '/' && it.peek() == Some(&'*') {
            it.next();
            let mut prev = '\0';
            for n in it.by_ref() {
                if n == '\n' { out.push('\n'); }
                if prev == '*' && n == '/' { break; }
                prev = n;
            }
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// Split on `sep` ignoring separators nested in `()`, `[]`, `{}`; pieces are trimmed.
pub fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            c if c == sep && depth <= 0 => {
                parts.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(s[start..].trim());
    parts
}

/// Text between the `(` at byte index `open` and its matching `)` (exclusive), if balanced.
pub fn balanced_parens(s: &str, open: usize) -> Option<&str> {
    let mut depth = 0i32;
    for (i, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vhdl_comment_hides_keyword() {
        assert!(!strip_comments_vhdl("-- entity foo\nx").to_lowercase().contains("entity"));
        assert!(strip_comments_vhdl("-- c\nx").ends_with("x"));
    }

    #[test]
    fn sv_block_comment_multiline() {
        let o = strip_comments_sv("a /* module\nfoo */ b // module bar\nc");
        assert!(!o.contains("module"));
        assert_eq!(o.matches('\n').count(), 2); // line count survives
        assert!(o.contains('a') && o.contains('b') && o.contains('c'));
    }

    #[test]
    fn sv_string_with_slashes_kept() {
        assert!(strip_comments_sv(r#"x = "http://a"; // gone"#).contains("http://a"));
    }

    #[test]
    fn split_ignores_nested_commas() {
        assert_eq!(split_top_level("a(1,2),b", ','), vec!["a(1,2)", "b"]);
    }

    #[test]
    fn balanced_parens_nested_and_unbalanced() {
        assert_eq!(balanced_parens("x(a(b),c) d", 1), Some("a(b),c"));
        assert_eq!(balanced_parens("x(a(b", 1), None);
    }
}
