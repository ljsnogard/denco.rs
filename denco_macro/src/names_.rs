//! `rename_all` 支持的大小写转换规则。

use zyn::{Span, syn};

pub(crate) fn apply_rename_all_(
    rule: &str,
    name: &str,
    span: Span,
) -> syn::Result<String> {
    let words = split_words_(name);
    let out = match rule {
        "lowercase" => words.join("").to_ascii_lowercase(),
        "UPPERCASE" => words.join("").to_ascii_uppercase(),
        "PascalCase" => words
            .iter()
            .map(|w| capitalize_(w))
            .collect::<Vec<_>>()
            .join(""),
        "camelCase" => {
            let mut out = String::new();
            for (i, word) in words.iter().enumerate() {
                if i == 0 {
                    out.push_str(&word.to_ascii_lowercase());
                } else {
                    out.push_str(&capitalize_(word));
                }
            }
            out
        }
        "snake_case" => words.join("_").to_ascii_lowercase(),
        "SCREAMING_SNAKE_CASE" => words.join("_").to_ascii_uppercase(),
        "kebab-case" => words.join("-").to_ascii_lowercase(),
        "SCREAMING-KEBAB-CASE" => words.join("-").to_ascii_uppercase(),
        _ => {
            return Err(syn::Error::new(
                span,
                format!(
                    "不支持的 rename_all 规则 `{rule}`；可用规则：lowercase、UPPERCASE、PascalCase、camelCase、snake_case、SCREAMING_SNAKE_CASE、kebab-case、SCREAMING-KEBAB-CASE"
                ),
            ));
        }
    };
    Ok(out)
}

fn capitalize_(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            out.extend(first.to_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
            out
        }
        None => String::new(),
    }
}

/// 把 Rust 标识符拆成单词：支持下划线分隔、驼峰边界与连续大写缩写。
fn split_words_(name: &str) -> Vec<String> {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let mut words = Vec::new();
    let mut current = String::new();
    let mut chars = name.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' {
            if !current.is_empty() {
                words.push(core::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_uppercase() && !current.is_empty() {
            let prev = current.chars().last().expect("non-empty");
            let next_is_lower = chars.peek().is_some_and(|next| next.is_lowercase());
            if prev.is_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_uppercase() && next_is_lower)
            {
                words.push(core::mem::take(&mut current));
            }
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    if words.is_empty() {
        words.push(name.to_owned());
    }
    words
}

#[cfg(test)]
mod tests_ {
    use super::*;

    fn apply_(rule: &str, name: &str) -> String {
        apply_rename_all_(rule, name, Span::call_site()).unwrap()
    }

    /// 测试常见 rename_all 规则；中英双语注释遵循项目测试规范。
    ///
    /// - 手段：对 `user_id` / `HTTPServer` 等标识符套用各规则。
    /// - 判断：输出与 serde 的命名约定一致。
    #[test]
    fn rename_all_rules_() {
        assert_eq!(apply_("snake_case", "userID"), "user_id");
        assert_eq!(apply_("camelCase", "user_id"), "userId");
        assert_eq!(apply_("PascalCase", "user_id"), "UserId");
        assert_eq!(apply_("SCREAMING_SNAKE_CASE", "userId"), "USER_ID");
        assert_eq!(apply_("kebab-case", "user_id"), "user-id");
        assert_eq!(apply_("snake_case", "HTTPServer"), "http_server");
        assert_eq!(apply_("PascalCase", "http_server"), "HttpServer");
    }
}
