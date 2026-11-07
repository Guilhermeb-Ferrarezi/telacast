//! Markdown do chat do Telacast (subconjunto do que o Discord entende), escrito à mão, sem dependências.
//!
//! O servidor guarda o texto cru; este crate só o interpreta para mostrar. Blocos: parágrafo, título
//! (`#`, `##`, `###`), subtexto (`-# `), citação (`> ` e `>>> `), lista (`- `, `* `, `1. `) e bloco de código
//! (três crases). Dentro dos blocos: negrito, itálico, sublinhado, tachado, código, spoiler, link
//! (`[texto](url)` e URL solta), menções (`<@id>`, `<#id>`) e carimbo de tempo (`<t:unix:estilo>`).

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Em(Vec<Inline>),
    Under(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Spoiler { id: u32, kids: Vec<Inline> },
    Link { kids: Vec<Inline>, url: String },
    User(i64),
    Channel(i64),
    Time { unix: i64, style: char },
    /// `@everyone` ou `@here`.
    Broadcast(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Subtext(Vec<Inline>),
    Quote(Vec<Block>),
    /// `start`: número do primeiro item numa lista numerada.
    List { start: Option<u64>, items: Vec<ListItem> },
    Code { lang: String, text: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub depth: u8,
    pub kids: Vec<Inline>,
}

const MAX_DEPTH: usize = 10;
const SCHEMES: [&str; 4] = ["http://", "https://", "mailto:", "tel:"];

/// Interpreta uma mensagem inteira.
pub fn parse(text: &str) -> Vec<Block> {
    let mut ids = 0u32;
    parse_blocks(text, true, &mut ids)
}

fn is_lang(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-' | '.' | '#'))
}

fn parse_blocks(text: &str, allow_quote: bool, ids: &mut u32) -> Vec<Block> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut i = 0;

    fn flush(para: &mut Vec<&str>, out: &mut Vec<Block>, ids: &mut u32) {
        if !para.is_empty() {
            let text = para.join("\n");
            out.push(Block::Paragraph(parse_inline(&text, 0, ids)));
            para.clear();
        }
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();

        // bloco de código: ``` ... ``` (pode abrir e fechar na mesma linha)
        if trimmed.starts_with("```") {
            let rest_lines = lines[i..].join("\n");
            let after = rest_lines.trim_start_matches(' ')[3..].to_string();
            if let Some(end) = after.find("```") {
                let body = &after[..end];
                let (lang, code) = match body.split_once('\n') {
                    Some((first, rest)) if is_lang(first.trim()) => (first.trim().to_string(), rest),
                    _ => (String::new(), body),
                };
                let code = code.trim_matches('\n');
                if !code.is_empty() {
                    flush(&mut para, &mut out, ids);
                    out.push(Block::Code { lang, text: code.to_string() });
                    // continua depois das crases de fechamento
                    let consumed_lines = after[..end + 3].matches('\n').count();
                    let tail = &after[end + 3..];
                    i += consumed_lines;
                    if !tail.trim().is_empty() {
                        // texto depois do fechamento, na mesma linha, vira um parágrafo
                        let tail_line = tail.split('\n').next().unwrap_or("");
                        if !tail_line.trim().is_empty() {
                            out.push(Block::Paragraph(parse_inline(tail_line.trim_start(), 0, ids)));
                        }
                    }
                    i += 1;
                    continue;
                }
            }
        }

        // >>> consome o resto da mensagem
        if allow_quote && trimmed.starts_with(">>> ") {
            flush(&mut para, &mut out, ids);
            let mut rest = vec![&trimmed[4..]];
            rest.extend_from_slice(&lines[i + 1..]);
            out.push(Block::Quote(parse_blocks(&rest.join("\n"), false, ids)));
            break;
        }
        // > citação (precisa de espaço depois do >)
        if allow_quote && trimmed.starts_with("> ") {
            flush(&mut para, &mut out, ids);
            let mut inner = Vec::new();
            while i < lines.len() {
                let t = lines[i].trim_start_matches(' ');
                if let Some(r) = t.strip_prefix("> ") {
                    inner.push(r);
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Block::Quote(parse_blocks(&inner.join("\n"), false, ids)));
            continue;
        }
        // título: 1 a 3 # e espaço
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=3).contains(&hashes) && trimmed[hashes..].starts_with(' ') && !trimmed[hashes..].trim().is_empty() {
            flush(&mut para, &mut out, ids);
            let body = trimmed[hashes..].trim().trim_end_matches('#').trim_end();
            out.push(Block::Heading(hashes as u8, parse_inline(body, 0, ids)));
            i += 1;
            continue;
        }
        // subtexto
        if let Some(r) = trimmed.strip_prefix("-# ") {
            if !r.trim().is_empty() {
                flush(&mut para, &mut out, ids);
                out.push(Block::Subtext(parse_inline(r.trim(), 0, ids)));
                i += 1;
                continue;
            }
        }
        // lista
        if list_marker(trimmed).is_some() {
            flush(&mut para, &mut out, ids);
            let mut items = Vec::new();
            let mut start = None;
            while i < lines.len() {
                let l = lines[i];
                let t = l.trim_start_matches(' ');
                let Some((num, body)) = list_marker(t) else { break };
                if items.is_empty() {
                    start = num;
                }
                let depth = (((l.len() - t.len()) / 2) as u8).min(MAX_DEPTH as u8 - 1);
                items.push(ListItem { depth, kids: parse_inline(body, 0, ids) });
                i += 1;
            }
            out.push(Block::List { start, items });
            continue;
        }
        let _ = indent;
        para.push(line);
        i += 1;
    }
    flush(&mut para, &mut out, ids);
    // parágrafos totalmente vazios (linhas em branco soltas) não viram bloco
    out.retain(|b| !matches!(b, Block::Paragraph(k) if k.iter().all(|n| matches!(n, Inline::Text(t) if t.trim().is_empty()))));
    out
}

/// `- x`, `* x`, `1. x`: devolve o número (se numerada) e o texto do item.
fn list_marker(t: &str) -> Option<(Option<u64>, &str)> {
    if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
        if !r.trim().is_empty() && !t.starts_with("-# ") {
            return Some((None, r));
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) && t[digits..].starts_with(". ") {
        let n: u64 = t[..digits].parse().ok()?;
        let r = &t[digits + 2..];
        if !r.trim().is_empty() {
            return Some((Some(n), r));
        }
    }
    None
}

fn push_text(out: &mut Vec<Inline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(last)) = out.last_mut() {
        last.push_str(s);
    } else {
        out.push(Inline::Text(s.to_string()));
    }
}

fn find_close(s: &str, delim: &str, min_inner: usize) -> Option<usize> {
    let mut from = delim.len() + min_inner;
    while from <= s.len() {
        let p = s.get(from..)?.find(delim)? + from;
        return Some(p);
    }
    None
}

/// Posição do `*` que fecha um itálico aberto em `rest[0]`.
fn find_star_close(rest: &str) -> Option<usize> {
    let mut from = 1;
    while let Some(p) = rest[from..].find('*').map(|p| p + from) {
        let sep_before = rest[..p].ends_with([' ', '\n']);
        let star_before = rest[..p].ends_with('*');
        let star_after = rest[p + 1..].starts_with('*');
        if p > 1 && !sep_before && !star_before && !star_after {
            return Some(p);
        }
        from = p + 1;
    }
    None
}

fn parse_inline(s: &str, depth: usize, ids: &mut u32) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    if depth >= MAX_DEPTH {
        push_text(&mut out, s);
        return out;
    }
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let c = rest.chars().next().unwrap();
        let prev = s[..i].chars().next_back();

        // escape
        if c == '\\' {
            if let Some(n) = rest[1..].chars().next() {
                if !n.is_alphanumeric() && !n.is_whitespace() {
                    push_text(&mut out, &n.to_string());
                    i += 1 + n.len_utf8();
                    continue;
                }
            }
        }
        // spoiler
        if rest.starts_with("||") {
            if let Some(end) = find_close(rest, "||", 1) {
                let id = *ids;
                *ids += 1;
                out.push(Inline::Spoiler { id, kids: parse_inline(&rest[2..end], depth + 1, ids) });
                i += end + 2;
                continue;
            }
        }
        // código em linha: crases simétricas
        if c == '`' {
            let n = rest.chars().take_while(|&c| c == '`').count();
            if n < 3 {
                let fence = &rest[..n];
                if let Some(end) = rest[n..].find(fence).map(|p| p + n) {
                    // o fechamento não pode fazer parte de uma sequência maior de crases
                    if end > n && rest[end + n..].chars().next() != Some('`') {
                        let body = &rest[n..end];
                        let body = body.strip_prefix(' ').unwrap_or(body);
                        let body = body.strip_suffix(' ').unwrap_or(body);
                        out.push(Inline::Code(body.to_string()));
                        i += end + n;
                        continue;
                    }
                }
            }
        }
        // negrito + itálico, negrito
        if rest.starts_with("***") {
            if let Some(end) = find_close(rest, "***", 1) {
                out.push(Inline::Strong(vec![Inline::Em(parse_inline(&rest[3..end], depth + 1, ids))]));
                i += end + 3;
                continue;
            }
        }
        if rest.starts_with("**") {
            if let Some(end) = find_close(rest, "**", 1) {
                if rest[end + 2..].chars().next() != Some('*') {
                    out.push(Inline::Strong(parse_inline(&rest[2..end], depth + 1, ids)));
                    i += end + 2;
                    continue;
                }
            }
        }
        // sublinhado (não é negrito)
        if rest.starts_with("__") {
            if let Some(end) = find_close(rest, "__", 1) {
                if rest[end + 2..].chars().next() != Some('_') {
                    out.push(Inline::Under(parse_inline(&rest[2..end], depth + 1, ids)));
                    i += end + 2;
                    continue;
                }
            }
        }
        // tachado
        if rest.starts_with("~~") {
            if let Some(end) = find_close(rest, "~~", 1) {
                out.push(Inline::Strike(parse_inline(&rest[2..end], depth + 1, ids)));
                i += end + 2;
                continue;
            }
        }
        // itálico: *x* (sem espaço colado ao delimitador)
        if c == '*' && !rest[1..].starts_with(['*', ' ', '\n']) {
            if let Some(p) = find_star_close(rest) {
                out.push(Inline::Em(parse_inline(&rest[1..p], depth + 1, ids)));
                i += p + 1;
                continue;
            }
        }
        if c == '_' && !prev.is_some_and(|p| p.is_alphanumeric()) && !rest[1..].starts_with(['_', ' ', '\n']) {
            let mut from = 1;
            let mut matched = false;
            while let Some(p) = rest[from..].find('_').map(|p| p + from) {
                let after = rest[p + 1..].chars().next();
                if p > 1 && !rest[..p].ends_with([' ', '\n', '_']) && after != Some('_') && !after.is_some_and(|a| a.is_alphanumeric()) {
                    out.push(Inline::Em(parse_inline(&rest[1..p], depth + 1, ids)));
                    i += p + 1;
                    matched = true;
                    break;
                }
                from = p + 1;
            }
            if matched {
                continue;
            }
        }
        // link mascarado [texto](url)
        if c == '[' {
            if let Some(close) = rest.find("](") {
                if let Some(paren) = rest[close + 2..].find(')') {
                    let url = &rest[close + 2..close + 2 + paren];
                    if SCHEMES.iter().any(|sc| url.starts_with(sc)) && !url.chars().any(|c| c.is_control() || c == ' ') {
                        let label = &rest[1..close];
                        if !label.is_empty() && !label.contains('[') {
                            out.push(Inline::Link { kids: parse_inline(label, depth + 1, ids), url: url.to_string() });
                            i += close + 2 + paren + 1;
                            continue;
                        }
                    }
                }
            }
        }
        // URL solta
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest.find(|c: char| c.is_whitespace() || c == '<').unwrap_or(rest.len());
            let mut url = &rest[..end];
            while url.ends_with(['.', ',', ':', ';', '"', '\'', ']', '!', '?']) || (url.ends_with(')') && !url.contains('(')) {
                url = &url[..url.len() - 1];
            }
            if url.len() > "https://".len() {
                out.push(Inline::Link { kids: vec![Inline::Text(url.to_string())], url: url.to_string() });
                i += url.len();
                continue;
            }
        }
        // tokens entre ângulos
        if c == '<' {
            if let Some(end) = rest.find('>') {
                let tok = &rest[1..end];
                let node = if let Some(id) = tok.strip_prefix("@!").or_else(|| tok.strip_prefix('@')).and_then(|d| d.parse::<i64>().ok()) {
                    Some(Inline::User(id))
                } else if let Some(id) = tok.strip_prefix('#').and_then(|d| d.parse::<i64>().ok()) {
                    Some(Inline::Channel(id))
                } else if let Some(t) = tok.strip_prefix("t:") {
                    let (num, style) = t.split_once(':').unwrap_or((t, "f"));
                    match (num.parse::<i64>(), style.chars().next()) {
                        (Ok(unix), Some(st)) if style.len() == 1 && "tTdDfFsSR".contains(st) => Some(Inline::Time { unix, style: st }),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(n) = node {
                    out.push(n);
                    i += end + 1;
                    continue;
                }
            }
        }
        // @everyone / @here
        if let Some(b) = ["@everyone", "@here"].iter().find(|b| rest.starts_with(**b)) {
            if !prev.is_some_and(|p| p.is_alphanumeric()) {
                out.push(Inline::Broadcast(b.to_string()));
                i += b.len();
                continue;
            }
        }
        push_text(&mut out, &c.to_string());
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests;
