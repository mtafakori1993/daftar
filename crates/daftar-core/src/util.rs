//! Small shared helpers used across the application.

use sha2::{Digest, Sha256};

/// Current wall-clock time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Human-readable timestamp used in file names and manifests.
pub fn timestamp_slug(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp_millis(0).expect("epoch"))
        .format("%Y%m%d-%H%M%S")
        .to_string()
}

/// SHA-256 of arbitrary bytes as lower-case hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Count words across mixed scripts. Arabic/Persian text is whitespace-delimited, which
/// matches user expectations for reading-time estimates.
pub fn word_count(text: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    text.unicode_words().count()
}

/// Extract plain text from Markdown by removing the most common markdown punctuation.
/// This is used for search snippets and flashcard suggestions, not for rendering.
pub fn plain_text(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len());
    let mut in_code_fence = false;
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut chars = line.chars().peekable();
        let mut at_line_start = true;
        while let Some(c) = chars.next() {
            match c {
                '#' | '>' | '*' | '_' | '`' | '~' if at_line_start => {}
                '#' | '*' | '_' | '`' => {}
                '[' => {}
                ']' => {
                    if chars.peek() == Some(&'(') {
                        for next in chars.by_ref() {
                            if next == ')' {
                                break;
                            }
                        }
                    }
                }
                '!' => {}
                c => {
                    out.push(c);
                    if !c.is_whitespace() {
                        at_line_start = false;
                    }
                }
            }
        }
        out.push('\n');
    }
    // Collapse runs of blank lines.
    let mut collapsed = String::with_capacity(out.len());
    let mut blank = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        collapsed.push_str(line.trim_end());
        collapsed.push('\n');
    }
    collapsed.trim().to_string()
}

/// Create a URL/anchor-safe slug that preserves Unicode letters (including Persian/Arabic).
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_dash = false;
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

/// Normalise whitespace for fuzzy text matching (used by change-resilient anchors).
pub fn normalize_ws(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        }
    }
    out
}

/// Normalise for loose comparison: collapse whitespace and case-fold.
pub fn normalize_for_match(input: &str) -> String {
    normalize_ws(input).to_lowercase()
}

/// First N characters, respecting UTF-8 boundaries, with an ellipsis when truncated.
pub fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut iter = input.char_indices();
    if let Some((idx, _)) = iter.nth(max_chars) {
        let mut out = input[..idx].to_string();
        out.push('…');
        out
    } else {
        input.to_string()
    }
}

/// Sanitise a document title into a safe file stem while keeping Unicode.
pub fn safe_file_stem(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_alphanumeric() || matches!(ch, ' ' | '-' | '_' | '.' | '(' | ')' | '[' | ']') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        "untitled".to_string()
    } else {
        out
    }
}

/// Format milliseconds as `h:mm` / `m:ss` for study timers.
pub fn format_duration_ms(ms: i64) -> String {
    let total_seconds = (ms.max(0) / 1000) as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// Format an estimated number of minutes for roadmaps.
pub fn format_minutes(minutes: u32) -> String {
    if minutes < 60 {
        format!("{minutes} min")
    } else {
        let h = minutes / 60;
        let m = minutes % 60;
        if m == 0 {
            format!("{h} h")
        } else {
            format!("{h} h {m} min")
        }
    }
}
/// Convert a practical subset of TeX into readable Unicode for inline/block math fallback.
/// This is intentionally not a TeX engine; unsupported constructs remain legible and are
/// documented as a limitation.
pub fn tex_to_unicode(tex: &str) -> String {
    let mut out = String::new();
    let mut chars = tex.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                let mut name = String::new();
                while let Some(c) = chars.peek().copied() {
                    if c.is_ascii_alphabetic() {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if name.is_empty() {
                    if let Some(c) = chars.next() {
                        out.push_str(match c {
                            '{' => "", '}' => "", '%' => "%", '&' => " ", _ => "",
                        });
                    }
                    continue;
                }
                match name.as_str() {
                    "alpha" => out.push('α'),
                    "beta" => out.push('β'),
                    "gamma" => out.push('γ'),
                    "delta" => out.push('δ'),
                    "epsilon" => out.push('ε'),
                    "zeta" => out.push('ζ'),
                    "eta" => out.push('η'),
                    "theta" => out.push('θ'),
                    "iota" => out.push('ι'),
                    "kappa" => out.push('κ'),
                    "lambda" => out.push('λ'),
                    "mu" => out.push('μ'),
                    "nu" => out.push('ν'),
                    "xi" => out.push('ξ'),
                    "pi" => out.push('π'),
                    "rho" => out.push('ρ'),
                    "sigma" => out.push('σ'),
                    "tau" => out.push('τ'),
                    "upsilon" => out.push('υ'),
                    "phi" => out.push('φ'),
                    "chi" => out.push('χ'),
                    "psi" => out.push('ψ'),
                    "omega" => out.push('ω'),
                    "Gamma" => out.push('Γ'),
                    "Delta" => out.push('Δ'),
                    "Theta" => out.push('Θ'),
                    "Lambda" => out.push('Λ'),
                    "Pi" => out.push('Π'),
                    "Sigma" => out.push('Σ'),
                    "Phi" => out.push('Φ'),
                    "Psi" => out.push('Ψ'),
                    "Omega" => out.push('Ω'),
                    "sum" => out.push('∑'),
                    "prod" => out.push('∏'),
                    "int" => out.push('∫'),
                    "infty" => out.push('∞'),
                    "times" => out.push('×'),
                    "cdot" => out.push('·'),
                    "pm" => out.push('±'),
                    "mp" => out.push('∓'),
                    "le" | "leq" => out.push('≤'),
                    "ge" | "geq" => out.push('≥'),
                    "ne" | "neq" => out.push('≠'),
                    "approx" => out.push('≈'),
                    "equiv" => out.push('≡'),
                    "to" | "rightarrow" => out.push('→'),
                    "leftarrow" => out.push('←'),
                    "leftrightarrow" => out.push('↔'),
                    "partial" => out.push('∂'),
                    "nabla" => out.push('∇'),
                    "in" => out.push('∈'),
                    "notin" => out.push('∉'),
                    "subset" => out.push('⊂'),
                    "cup" => out.push('∪'),
                    "cap" => out.push('∩'),
                    "forall" => out.push('∀'),
                    "exists" => out.push('∃'),
                    "sqrt" => {
                        let inner = read_group(&mut chars);
                        out.push('√');
                        if inner.chars().count() > 1 {
                            out.push('(');
                            out.push_str(&tex_to_unicode(&inner));
                            out.push(')');
                        } else {
                            out.push_str(&tex_to_unicode(&inner));
                        }
                    }
                    "frac" => {
                        let a = read_group(&mut chars);
                        let b = read_group(&mut chars);
                        out.push('(');
                        out.push_str(&tex_to_unicode(&a));
                        out.push_str(")/(");
                        out.push_str(&tex_to_unicode(&b));
                        out.push(')');
                    }
                    "text" | "mathrm" | "mathbf" | "operatorname" => {
                        out.push_str(&read_group(&mut chars));
                    }
                    "quad" | "qquad" | "," | ";" | " " => out.push(' '),
                    _ => {
                        // Unknown command: keep it readable rather than raw backslashes.
                        out.push_str(&name);
                    }
                }
            }
            '^' => {
                let group = read_script_group(&mut chars);
                out.push_str(&super_script_text(&group));
            }
            '_' => {
                let group = read_script_group(&mut chars);
                out.push_str(&sub_script_text(&group));
            }
            '{' | '}' | '$' => {}
            '&' => out.push(' '),
            c => out.push(c),
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        tex.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

fn read_group(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut out = String::new();
    while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
        chars.next();
    }
    match chars.peek().copied() {
        Some('{') => {
            chars.next();
            let mut depth = 1;
            while let Some(c) = chars.next() {
                match c {
                    '{' => {
                        depth += 1;
                        out.push(c);
                    }
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        out.push(c);
                    }
                    _ => out.push(c),
                }
            }
        }
        Some(c) => {
            chars.next();
            out.push(c);
        }
        None => {}
    }
    out
}

fn read_script_group(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    read_group(chars)
}

fn super_script_text(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let mapped = match c {
            '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
            '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',
            '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾',
            'n' => 'ⁿ', 'i' => 'ⁱ', 'x' => 'ˣ', 'a' => 'ᵃ', 'b' => 'ᵇ',
            'c' => 'ᶜ', 'd' => 'ᵈ', 'e' => 'ᵉ', 'k' => 'ᵏ', 'm' => 'ᵐ',
            'o' => 'ᵒ', 'p' => 'ᵖ', 's' => 'ˢ', 't' => 'ᵗ', 'u' => 'ᵘ',
            'v' => 'ᵛ', 'w' => 'ʷ', 'y' => 'ʸ', 'z' => 'ᶻ',
            _ => {
                let converted = tex_to_unicode(text);
                return format!("^({converted})");
            }
        };
        out.push(mapped);
    }
    out
}

fn sub_script_text(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let mapped = match c {
            '0' => '₀', '1' => '₁', '2' => '₂', '3' => '₃', '4' => '₄',
            '5' => '₅', '6' => '₆', '7' => '₇', '8' => '₈', '9' => '₉',
            '+' => '₊', '-' => '₋', '=' => '₌', '(' => '₍', ')' => '₎',
            'a' => 'ₐ', 'e' => 'ₑ', 'i' => 'ᵢ', 'n' => 'ₙ', 'x' => 'ₓ',
            _ => {
                let converted = tex_to_unicode(text);
                return format!("_({converted})");
            }
        };
        out.push(mapped);
    }
    out
}


#[cfg(test)]
mod tex_tests {
    use super::tex_to_unicode;

    #[test]
    fn converts_greek_and_scripts() {
        let out = tex_to_unicode("e^{i\\pi} + 1 = 0");
        assert!(out.contains('π'));
        assert!(out.contains("e^"));
        let out = tex_to_unicode("x_1 + x^2");
        assert!(out.contains('₁'));
        assert!(out.contains('²'));
    }

    #[test]
    fn converts_fractions_and_roots() {
        assert_eq!(tex_to_unicode("\\frac{a}{b}"), "(a)/(b)");
        assert_eq!(tex_to_unicode("\\sqrt{x}"), "√x");
        assert_eq!(tex_to_unicode("\\alpha + \\beta"), "α + β");
    }

    #[test]
    fn leaves_plain_text_legible() {
        assert_eq!(tex_to_unicode("E = mc^2"), "E = mc²");
    }
}
