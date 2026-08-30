use std::path::Path;
use std::sync::OnceLock;

use syntect::easy::ScopeRegionIterator;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

const LIMIT: usize = 200;

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    Comment,
    Quoted,
    Number,
    Constant,
    Keyword,
    Type,
    Function,
}

pub struct Piece {
    pub tone: Tone,
    pub text: String,
}

pub fn lines(path: &Path, source: &[String]) -> Vec<Vec<Piece>> {
    match parse(path, source) {
        Some(parsed) => parsed,
        None => source.iter().map(|line| plain(line)).collect(),
    }
}

fn parse(path: &Path, source: &[String]) -> Option<Vec<Vec<Piece>>> {
    let set = syntaxes();
    let name = path.file_name().and_then(|name| name.to_str());
    let extension = path.extension().and_then(|ext| ext.to_str());

    let syntax = extension
        .and_then(|ext| set.find_syntax_by_extension(ext))
        .or_else(|| name.and_then(|name| set.find_syntax_by_extension(name)))
        .or_else(|| {
            source
                .first()
                .and_then(|line| set.find_syntax_by_first_line(line))
        })?;

    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Vec::with_capacity(source.len());

    for line in source {
        if out.len() >= LIMIT {
            out.push(plain(line));
            continue;
        }

        let text = format!("{line}\n");
        let ops = state.parse_line(&text, set).ok()?;
        let mut pieces: Vec<Piece> = Vec::new();

        for (part, op) in ScopeRegionIterator::new(&ops, &text) {
            stack.apply(op).ok()?;
            let part = part.strip_suffix('\n').unwrap_or(part);
            if part.is_empty() {
                continue;
            }

            let tone = tone(&stack);
            match pieces.last_mut() {
                Some(last) if last.tone == tone => last.text.push_str(part),
                _ => pieces.push(Piece {
                    tone,
                    text: part.to_string(),
                }),
            }
        }
        out.push(pieces);
    }

    Some(out)
}

fn plain(line: &str) -> Vec<Piece> {
    if line.is_empty() {
        return Vec::new();
    }
    vec![Piece {
        tone: Tone::Plain,
        text: line.to_string(),
    }]
}

fn tone(stack: &ScopeStack) -> Tone {
    for (selector, tone) in rules() {
        if stack
            .as_slice()
            .iter()
            .any(|scope| selector.is_prefix_of(*scope))
        {
            return *tone;
        }
    }
    Tone::Plain
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn rules() -> &'static [(Scope, Tone)] {
    static RULES: OnceLock<Vec<(Scope, Tone)>> = OnceLock::new();
    RULES.get_or_init(|| {
        [
            ("comment", Tone::Comment),
            ("string", Tone::Quoted),
            ("constant.numeric", Tone::Number),
            ("constant", Tone::Constant),
            ("entity.name.function", Tone::Function),
            ("support.function", Tone::Function),
            ("variable.function", Tone::Function),
            ("keyword.operator", Tone::Plain),
            ("keyword", Tone::Keyword),
            ("storage", Tone::Keyword),
            ("entity.name", Tone::Type),
            ("support.type", Tone::Type),
            ("support.class", Tone::Type),
        ]
        .into_iter()
        .filter_map(|(name, tone)| Some((Scope::new(name).ok()?, tone)))
        .collect()
    })
}
