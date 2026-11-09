use log::trace;

use crate::StringID;
use crate::il::{
    self, BinOp, CmpOp, ILABITyp, ILDataItem, ILDataLayoutKind, ILFunctionData, ILGlobalData,
    ILLinkage, ILModule, ILModuleContext, ILSignatureData, ILTemp, UnOp,
};
use crate::il::{ILDataLayout, ILType};
use crate::parse::ParseError::UnexpectedToken;
use crate::target::CompilationTarget;
pub struct Tokenize<'s> {
    source: &'s str,
    loc: TokenizerLocation,
}

impl<'s> Iterator for Tokenize<'s> {
    type Item = Token;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub span: TokenSpan,
    pub kind: TokenKind,
}

impl Token {
    pub fn to_string(self) -> String {
        let binding = self.kind.to_string();
        let kind = match self.kind {
            TokenKind::Ident(i) => crate::intern::unintern_string(i).unwrap(),
            TokenKind::Comment(i) => crate::intern::unintern_string(i).unwrap(),
            _ => binding,
        };
        if matches!(self.kind, TokenKind::Comment(_) | TokenKind::Ident(_)) {
            format!(
                "({}:{}|{})({kind:?})",
                self.span.line, self.span.col, self.span.len
            )
        } else {
            format!(
                "({}:{}|{})({kind})",
                self.span.line, self.span.col, self.span.len
            )
        }
    }
}
impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.kind.display_err(),
            self.span.line,
            self.span.col
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub enum TokenKind {
    Ws,
    At,
    Percent,
    Dollar,
    Colon,

    Function,
    Type,
    Data,
    Export,
    Thread,

    Phi,

    Ret,
    Jmp,
    Jnz,
    Hlt,

    Call,
    DotDotDot,
    Comma,
    Env,

    Assign,

    LitInt(u64),
    LitDouble(u64),
    LitFloat(u32),
    LitString(StringID),

    Ident(StringID),

    CurlyOpen,
    CurlyClose,
    ParenOpen,
    ParenClose,
    Comment(StringID),
    Attribute(StringID),
}

impl TokenKind {
    fn display_err(&self) -> String {
        'is_literal: {
            let literal = match self {
                TokenKind::Ws => "[whitespace]",
                TokenKind::At => "@",
                TokenKind::Percent => "%",
                TokenKind::Dollar => "$",
                TokenKind::Colon => ":",
                TokenKind::Function => "function",
                TokenKind::Type => "type",
                TokenKind::Data => "data",
                TokenKind::Export => "export",
                TokenKind::Thread => "thread",
                TokenKind::Phi => "phi",
                TokenKind::Ret => "ret",
                TokenKind::Jmp => "jmp",
                TokenKind::Jnz => "jnz",
                TokenKind::Hlt => "hlt",
                TokenKind::Call => "call",
                TokenKind::DotDotDot => "...",
                TokenKind::Comma => ",",
                TokenKind::Env => "env",
                TokenKind::Assign => "=",
                TokenKind::CurlyOpen => "{",
                TokenKind::CurlyClose => "}",
                TokenKind::ParenOpen => "(",
                TokenKind::ParenClose => ")",
                _ => break 'is_literal,
            };
            return literal.to_string();
        }
        match self {
            TokenKind::LitInt(i) => i.to_string(),
            TokenKind::LitDouble(f) => f64::from_bits(*f).to_string(),
            TokenKind::LitFloat(f) => f32::from_bits(*f).to_string(),
            TokenKind::Comment(string_id)
            | TokenKind::Attribute(string_id)
            | TokenKind::Ident(string_id) => crate::intern::unintern_string(*string_id).unwrap(),
            _ => unreachable!(),
        }
    }
}

/// The location that a [`Tokenizer`] is at
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TokenizerLocation {
    idx: usize,
    line: usize,
    col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpan {
    start: usize,
    pub line: usize,
    pub col: usize,
    len: usize,
}

impl TokenSpan {
    pub fn view(self, source: &str) -> Option<&str> {
        source.get(self.start..self.start + self.len)
    }
    pub fn view_lines_around(self, source: &str, around: usize) -> String {
        let line_start = self.line.saturating_sub(around);
        let line_end = self.line + around;
        let line_diff = line_end - line_start + 1;
        let to_fmt_lines: Vec<_> = source
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l))
            .skip(line_start)
            .take(line_diff)
            .collect();
        let mut lines_fmted = vec![String::from("###:------------")];
        for (i, line) in to_fmt_lines {
            lines_fmted.push(format!("{:3}: {line}", i));
            if i == self.line {
                lines_fmted.push(format!(
                    "###:{}{}",
                    "_".repeat(self.col),
                    "^".repeat(self.len.min(line.len()))
                ));
                lines_fmted.push(format!("###:{} *here*", " ".repeat(self.col - 1),));
            }
        }
        lines_fmted.push(String::from("###:------------"));
        lines_fmted.join("\n")
    }
}

impl PartialOrd for TokenizerLocation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.idx.cmp(&other.idx))
    }
}
impl Ord for TokenizerLocation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.idx.cmp(&other.idx)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unexpected EOF")]
    UnexpectedEOF,
    #[error("unexpected char: '{0}'")]
    UnexpectedChar(char),
    #[error("unexpected Token: {0}")]
    UnexpectedToken(Token),
    #[error("unknown keyword: {0}")]
    UnknownKeyWord(Box<str>),
    #[error("invalid escape-character `{0}`")]
    InvalidEscapeChar(char),
}

impl TokenizerLocation {
    pub const fn new(idx: usize, line: usize, col: usize) -> Self {
        Self { idx, line, col }
    }
    pub const fn start() -> Self {
        Self {
            idx: 0,
            line: 1,
            col: 1,
        }
    }
    pub fn order(self, other: Self) -> (Self, Self) {
        if self < other {
            (self, other)
        } else {
            (other, self)
        }
    }
    /// assumes `self < other`
    pub const fn to_span(&self, other: Self) -> TokenSpan {
        TokenSpan {
            start: self.idx,
            line: self.line,
            col: self.col,
            len: other.idx - self.idx,
        }
    }
}

macro_rules! emit_token {
    ($s:expr, $loc:expr, $k:path) => {
        Some(Token {
            span: $loc.to_span($s.loc),
            kind: $k,
        })
    };
    ($s:expr, $loc:expr, $k:path :$l:ident) => {
        Some(Token {
            span: $loc.to_span($s.loc),
            kind: $k($l),
        })
    };
}
impl<'s> Tokenize<'s> {
    pub fn advance_ident_str(&mut self) -> Result<Option<&str>, ParseError> {
        let start = self.loc;
        loop {
            if let Some('a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$' | '.') = self.peek() {
                self.advance()?;
            } else {
                break;
            }
        }
        let end = self.loc;
        Ok(self.span_between_states(start, end))
    }
    pub fn advance_lit_str(&mut self) -> Result<(TokenSpan, &str), ParseError> {
        let start = self.loc;
        let (span, end) = loop {
            match self.peek() {
                Some('\\') => {
                    self.advance()?;
                    match self.peek() {
                        Some('"' | '\\' | '\'' | 'n' | 't' | '0') => self.advance()?,
                        Some(c) => return Err(ParseError::InvalidEscapeChar(c)),
                        None => return Err(ParseError::UnexpectedEOF),
                    }
                }
                Some('"') => {
                    let end = self.loc;
                    self.advance()?;
                    let span = start.to_span(end);
                    break (span, end);
                }
                Some(_) => {
                    self.advance()?;
                }
                None => return Err(ParseError::UnexpectedEOF),
            }
        };
        Ok((span, self.span_between_states(start, end).unwrap()))
    }
}

impl<'s> Tokenize<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            loc: TokenizerLocation::start(),
        }
    }

    const fn idx(&self) -> usize {
        self.loc.idx
    }
    const fn src_len(&self) -> usize {
        self.source.len()
    }
    fn peek(&self) -> Option<char> {
        match self.source.chars().nth(self.idx()) {
            Some(c) => Some(c),
            None => {
                std::hint::cold_path();
                None
            }
        }
    }
    fn span_between_states(&self, s1: TokenizerLocation, s2: TokenizerLocation) -> Option<&str> {
        span_between(self.source, s1, s2)
    }
    fn span_between(&self, s: TokenizerLocation) -> Option<&str> {
        span_between(self.source, self.loc, s)
    }

    fn advance(&mut self) -> Result<(), ParseError> {
        let c = self.peek().ok_or(ParseError::UnexpectedEOF)?;
        if self.idx() < self.src_len() {
            self.loc.idx += 1;
            if c == '\n' {
                self.loc.line += 1;
                self.loc.col = 1;
            } else {
                self.loc.col += 1;
            }
        }
        Ok(())
    }
    fn advance_while_char<F>(&mut self, f: F) -> Result<&'s str, ParseError>
    where
        F: Fn(char) -> bool,
    {
        let start: usize = self.idx();
        loop {
            let c = self.peek().ok_or(ParseError::UnexpectedEOF)?;
            if !f(c) {
                break;
            }
            self.advance()?;
        }
        Ok(&self.source[start..self.idx()])
    }

    fn ws(&mut self) -> Result<(), ParseError> {
        self.advance_while_char(char::is_whitespace).map(|_| ())
    }

    pub fn next_token(&mut self) -> Option<Token> {
        let locstart = self.loc;
        let c = self.peek()?;
        self.advance().ok()?;
        match c {
            '%' => emit_token!(self, locstart, TokenKind::Percent),
            '$' => emit_token!(self, locstart, TokenKind::Dollar),
            ':' => emit_token!(self, locstart, TokenKind::Colon),
            '@' => emit_token!(self, locstart, TokenKind::At),
            '=' => emit_token!(self, locstart, TokenKind::Assign),
            ',' => emit_token!(self, locstart, TokenKind::Comma),
            '{' => emit_token!(self, locstart, TokenKind::CurlyOpen),
            '}' => emit_token!(self, locstart, TokenKind::CurlyClose),
            '(' => emit_token!(self, locstart, TokenKind::ParenOpen),
            ')' => Some(Token {
                span: locstart.to_span(self.loc),
                kind: TokenKind::ParenClose,
            }),
            c if c.is_whitespace() => {
                self.ws().ok()?;
                Some(Token {
                    span: locstart.to_span(self.loc),
                    kind: TokenKind::Ws,
                })
            }
            '0'..='9' => self.parse_literal_int(locstart),
            '#' => self.parse_line_comment(locstart),
            '"' => {
                self.advance().ok()?;
                let (span, s) = self.advance_lit_str().ok()?;

                Some(Token {
                    span,
                    kind: TokenKind::LitString(crate::intern::intern_str(s)),
                })
            }
            _ => self.next_multi_token(c, locstart),
        }
    }

    fn next_multi_token(&mut self, c: char, locstart: TokenizerLocation) -> Option<Token> {
        let c1 = self.peek()?;
        match (c, c1) {
            ('s', '_') => return self.parse_literal_float(),
            ('d', '_') => return self.parse_literal_double(),
            ('#', '[') => {
                self.advance().ok()?;
                return self.parse_attribute();
            }
            _ => {}
        };
        self.advance_ident_str().ok()?;
        let s = self.span_between(locstart)?;
        match s {
            "function" => return emit_token!(self, locstart, TokenKind::Function),
            "type" => return emit_token!(self, locstart, TokenKind::Type),
            "data" => return emit_token!(self, locstart, TokenKind::Data),
            "phi" => return emit_token!(self, locstart, TokenKind::Phi),
            "call" => return emit_token!(self, locstart, TokenKind::Call),
            "..." => return emit_token!(self, locstart, TokenKind::DotDotDot),
            "export" => return emit_token!(self, locstart, TokenKind::Export),
            "thread" => return emit_token!(self, locstart, TokenKind::Thread),
            "ret" => return emit_token!(self, locstart, TokenKind::Ret),
            "jmp" => return emit_token!(self, locstart, TokenKind::Jmp),
            "jnz" => return emit_token!(self, locstart, TokenKind::Jnz),
            "hlt" => return emit_token!(self, locstart, TokenKind::Hlt),
            "env" => return emit_token!(self, locstart, TokenKind::Env),
            _ => {}
        }
        let s = s.to_string();
        let ident = crate::intern::intern_string(s);
        Some(Token {
            span: locstart.to_span(self.loc),
            kind: (TokenKind::Ident(ident)),
        })
    }

    fn parse_literal_int(&mut self, locstart: TokenizerLocation) -> Option<Token> {
        self.loc = locstart;
        let s = self.advance_while_char(|c| char::is_digit(c, 10)).ok()?;
        u64::from_str_radix(s, 10).ok().map(|n| Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::LitInt(n),
        })
    }

    fn parse_literal_float(&mut self) -> Option<Token> {
        let locstart = self.loc;
        self.advance().ok()?;
        let s = self.advance_while_char(|c| !c.is_whitespace()).ok()?;
        s.parse::<f32>().ok().map(|f| Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::LitFloat(f.to_bits()),
        })
    }
    fn parse_literal_double(&mut self) -> Option<Token> {
        let locstart = self.loc;
        self.advance().ok()?;
        let s = self.advance_while_char(|c| !c.is_whitespace()).ok()?;
        s.parse::<f64>().ok().map(|f| Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::LitDouble(f.to_bits()),
        })
    }

    fn parse_line_comment(&mut self, locstart: TokenizerLocation) -> Option<Token> {
        loop {
            let c = self.peek();
            if c.is_none() {
                break;
            } else if c == Some('\n') {
                break;
            } else {
                self.advance().ok()?;
                continue;
            }
        }
        let s = self.span_between(locstart).unwrap().to_string();
        // skip the newline
        self.advance().ok()?;
        let id = crate::intern::intern_string(s);
        Some(Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::Comment(id),
        })
    }

    fn parse_attribute(&mut self) -> Option<Token> {
        let loc = self.loc;
        let s = self.advance_while_char(|c| c != ']').ok()?.to_string();
        let s = crate::intern::intern_string(s);
        let kind = TokenKind::Attribute(s);
        Some(Token {
            span: loc.to_span(self.loc),
            kind,
        })
    }
}

fn span_between(source: &str, s1: TokenizerLocation, s2: TokenizerLocation) -> Option<&str> {
    let (s1, s2) = s1.order(s2);
    source.get(s1.idx..s2.idx)
}

pub struct Parser {
    tokens: Vec<Token>,
    idx: usize,
    tables: ILModuleContext,
    target: CompilationTarget,
}

/// expect a single token,
///
/// `exp_tok(self, int)` returns [`Option<u64>`],
/// `exp_tok(self, id)` returns [`Option<StringID>`],
/// `exp_tok(self, idstr)` returns [`Option<String>`] by uninterning the [`StringID`]
///
///
/// other tokens can be specified by a char-literal, or by typing the keyword out:
///
/// ```ignore
/// exp_tok!(self, '(')?;
/// exp_tok(self, function)?;
/// ```
macro_rules! exp_tok {
    ($self:expr, int) => {
        if let Ok(tok) = $self.peek_res() {
            if let TokenKind::LitInt(i) = tok.kind {
                $self.idx += 1;
                Ok(i)
            } else {
                Err(ParseError::UnexpectedToken(tok))
            }
        } else {
            Err(ParseError::UnexpectedEOF)
        }
    };
    ($self:expr, id) => {
        if let Ok(tok) = $self.peek_res() {
            if let TokenKind::Ident(i) = tok.kind {
                $self.idx += 1;
                Ok(i)
            } else {
                Err(ParseError::UnexpectedToken(tok))
            }
        } else {
            Err(ParseError::UnexpectedEOF)
        }
    };
    ($self:expr, str) => {
        if let Ok(tok) = $self.peek_res() {
            if let TokenKind::LitString(s) = tok.kind {
                $self.idx += 1;
                Ok(s)
            } else {
                Err(ParseError::UnexpectedToken(tok))
            }
        } else {
            Err(ParseError::UnexpectedEOF)
        }
    };
    ($self:expr, idstr) => {
        if let Ok(tok) = $self.peek_res() {
            if let TokenKind::Ident(i) = tok.kind {
                $self.idx += 1;
                let s = crate::intern::unintern_boxstr(i).unwrap();
                Ok(s)
            } else {
                Err(ParseError::UnexpectedToken(tok))
            }
        } else {
            Err(ParseError::UnexpectedEOF)
        }
    };
    ($self:expr, $c:tt) => {
        if let Ok(tok) = $self.peek_res() {
            if let tokenchar_kind!($c) = tok.kind {
                $self.idx += 1;
                Ok(())
            } else {
                Err(ParseError::UnexpectedToken(tok))
            }
        } else {
            Err(ParseError::UnexpectedEOF)
        }
    };
}
/// Parse a sequence of tokens
///
/// If the sequence can be parsed, then the
/// parser state is advanced to the end of the sequence and the specified tokens are consumed,
///
/// if the sequence cannot be fully parsed,
/// then the state is reset to right before the first token,
/// and control flow returns from the function (this macro inserts return statements).
///
/// A sequence of patterns must be supplied, any bindings within these patterns
/// can be used later, for example a label or identifier.
///
/// # Example
///
/// ```ignore
/// tok_seq!(self;
///   TokenKind::Percent,
///   TokenKind::Ident(i),
///   TokenKind::Assign,
/// );
/// let temp = ILTemp::NonSSA(i);
/// let typ = self.parse_basetype()?;
/// return Some((temp,typ));
/// ```
macro_rules! tok_seq {
    ($self:expr; $t0:ident := $p0:pat $(, $t:ident := $p:pat)* $(,)?) => {
        let idx_start = $self.idx;
        let Some($t0 @ tok) = $self.peek() else {
            $self.idx = idx_start;
            return Err(ParseError::UnexpectedEOF);
        };
        let $p0 = tok.kind else {
            return Err(ParseError::UnexpectedToken(tok));
        };
        $self.idx += 1;
        $(
        let Some($t @ tok) = $self.peek() else {
            $self.idx = idx_start;
            return Err(ParseError::UnexpectedEOF);
        };
        let $p = tok.kind else {
            $self.idx = idx_start;
            return Err(ParseError::UnexpectedToken(tok));
        };
        $self.idx += 1;
        )*
        let _ = idx_start;
    };
    ($self:expr; rst $rst:expr; $p0:pat $(, $p:pat)* $(,)?) => {
        let idx_start = $rst;
        let tok = $self.peek_res()?;
        let $p0 = tok.kind else {
            $self.idx = idx_start;
            return Err(ParseError::UnexpectedToken(tok));
        };
        $self.idx += 1;
        $(
        let tok = $self.peek_res()?;
        let $p = tok.kind else {
            $self.idx = idx_start;
            return Err(ParseError::UnexpectedToken(tok));
        };
        $self.idx += 1;
        )*
        let _ = idx_start;
    };

}

macro_rules! tokenchar_kind {
    ('%') => {
        TokenKind::Percent
    };
    ('$') => {
        TokenKind::Dollar
    };
    ('@') => {
        TokenKind::At
    };
    (':') => {
        TokenKind::Colon
    };
    (',') => {
        TokenKind::Comma
    };
    ('(') => {
        TokenKind::ParenOpen
    };
    (')') => {
        TokenKind::ParenClose
    };
    ('{') => {
        TokenKind::CurlyOpen
    };
    ('}') => {
        TokenKind::CurlyClose
    };
    ('=') => {
        TokenKind::Assign
    };
    (',') => {
        TokenKind::Assign
    };
    (phi) => {
        TokenKind::Phi
    };
    (call) => {
        TokenKind::Call
    };
    (function) => {
        TokenKind::Function
    };
    (export) => {
        TokenKind::Export
    };
    (thread) => {
        TokenKind::Thread
    };
    (dotdotdot) => {
        TokenKind::DotDotDot
    };
}

impl Parser {
    pub fn new(source: &str, target: CompilationTarget) -> Self {
        let mut t = Tokenize::new(source);
        let mut tokens = vec![];
        while let Some(tok) = t.next_token() {
            match tok.kind {
                TokenKind::Ws | TokenKind::Comment(_) => continue,
                _ => {
                    tokens.push(tok);
                }
            }
        }
        let tables = ILModuleContext::with_capacity(32);
        Self {
            tables,
            tokens: tokens,
            idx: 0,
            target,
        }
    }
}
impl Parser {
    pub fn parse_module(mut self) -> Result<ILModule, ParseError> {
        trace!("commencing parse");
        let start = std::time::Instant::now();
        loop {
            let f = dbg!(self.parse_function());
            if self.peek() == None {
                break;
            }
            match f {
                Ok(_) => {}
                Err(_) => {
                    let d = dbg!(self.parse_datadef());
                    if self.peek() == None {
                        break;
                    }
                    match d {
                        Ok(_) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
        }
        let end = start.elapsed();
        trace!("finished parse in {} microseconds", end.as_micros());
        Ok(ILModule { ctx: self.tables })
    }
    pub fn peek(&self) -> Option<Token> {
        self.tokens.get(self.idx).copied()
    }
    pub fn peek_res(&self) -> Result<Token, ParseError> {
        self.tokens
            .get(self.idx)
            .copied()
            .ok_or(ParseError::UnexpectedEOF)
    }

    fn parse_itemp(&mut self) -> Result<StringID, ParseError> {
        tok_seq!(self; rst self.idx;
            TokenKind::Percent,
            TokenKind::Ident(id)
        );
        Ok(id)
    }
    fn parse_iglob(&mut self) -> Result<StringID, ParseError> {
        tok_seq!(self; rst self.idx;
            TokenKind::Dollar,
            TokenKind::Ident(id)
        );
        Ok(id)
    }
    fn parse_ilabel(&mut self) -> Result<StringID, ParseError> {
        tok_seq!(self;rst self.idx;
            TokenKind::At,
            TokenKind::Ident(id)
        );
        Ok(id)
    }
    fn parse_itype(&mut self) -> Result<StringID, ParseError> {
        tok_seq!(self;rst self.idx;
            TokenKind::Colon,
            TokenKind::Ident(id)
        );
        Ok(id)
    }

    pub fn parse_datadef(&mut self) -> Result<il::ILGlobal, ParseError> {
        let restore = self.idx;
        let linkage = exp_tok!(self, export)
            .map(|_| ILLinkage::Export)
            .unwrap_or(ILLinkage::Internal);

        tok_seq!(self; rst restore;
            TokenKind::Data,
            TokenKind::Dollar,
            TokenKind::Ident(name),
            TokenKind::Assign,
        );
        let align = exp_tok!(self, int).map(|i| i as usize).ok();

        tok_seq!(self; rst restore; TokenKind::CurlyOpen);
        let (layout, items) = self.parse_datadef_items()?;
        tok_seq!(self; rst restore; TokenKind::CurlyClose);

        let data = ILGlobalData {
            linkage,
            layout,
            align,
            name,
            items,
        };

        Ok(self.tables.globals.get_or_intern(data))
    }

    pub fn parse_function(&mut self) -> Result<il::ILFunction, ParseError> {
        let restore = self.idx;

        let linkage = exp_tok!(self, export)
            .map(|_| ILLinkage::Export)
            .unwrap_or(ILLinkage::Internal);

        exp_tok!(self, function)?;
        let returns = self
            .parse_abitype(restore)
            .unwrap_or(self.tables.get_type_Zero());
        tok_seq!(self;rst restore;
            TokenKind::Dollar,
            TokenKind::Ident(name),
            TokenKind::ParenOpen,
        );
        let mut param_types = Vec::new();
        let mut param_temporaries = Vec::new();
        loop {
            let Ok(typ) = self.parse_abitype(restore) else {
                break;
            };
            let arg = ILTemp::NonSSA(self.parse_itemp()?);
            param_types.push(typ);
            param_temporaries.push(arg);
            let status = exp_tok!(self, ',');
            match status {
                Err(ParseError::UnexpectedToken(_)) => break,
                Err(e @ ParseError::UnexpectedEOF) => return Err(e),
                _ => (),
            }
        }
        exp_tok!(self, ')')?;
        let signature = ILSignatureData {
            param_types: param_types.into_boxed_slice(),
            param_temporaries: param_temporaries.into_boxed_slice(),
            returns,
        };
        exp_tok!(self, '{')?;
        let mut blocks = vec![];
        while let Ok(block) = dbg!(self.parse_block()) {
            blocks.push(block);
        }
        dbg!(exp_tok!(self, '}'))?;

        let data = ILFunctionData {
            linkage,
            signature,
            name,
            blocks,
        };
        let data = self.tables.functions.push(data);
        Ok(data)
    }

    fn parse_block(&mut self) -> Result<il::ILBlockData, ParseError> {
        let label = self.parse_ilabel()?;
        let mut phis = Vec::new();
        let mut items = Vec::new();

        while let Ok(phi) = self.parse_phi_assign() {
            dbg!(&phi);
            phis.push(phi);
        }
        while let Ok(instr) = dbg!(self.parse_block_item()) {
            items.push(instr);
        }

        let terminator = match self.parse_terminator() {
            Ok(t) => t,
            Err(ParseError::UnexpectedToken(Token {
                span: _,
                kind: TokenKind::At,
            })) => il::ILTerminator::FallThrough,
            Err(e) => return Err(e),
        };

        Ok(il::ILBlockData {
            label,
            phis,
            items,
            terminator,
        })
    }

    fn parse_phi_assign(&mut self) -> Result<(il::ILTemp, il::ILType, il::ILPhiNode), ParseError> {
        let restore = self.idx;
        tok_seq!(self;rst restore;
            TokenKind::Percent,
            TokenKind::Ident(phi_name),
            TokenKind::Assign
        );
        let temp = ILTemp::NonSSA(phi_name);
        let typ = self.parse_basetype(restore)?;

        tok_seq!(self; rst restore; TokenKind::Phi);

        let mut incoming = Vec::new();

        if let Ok(label) = self.parse_ilabel() {
            match self.parse_operand() {
                Ok(val) => {
                    incoming.push((label, val));
                }
                Err(e) => {
                    self.idx = restore;
                    return Err(e);
                }
            }
        }

        loop {
            if let Err(_e) = exp_tok!(self, ',') {
                break;
            }
            let label = self.parse_ilabel()?;
            let val = self.parse_operand()?;
            incoming.push((label, val));
        }
        let phi = il::ILPhiNode { incoming };
        Ok((temp, typ, phi))
    }

    fn parse_operand(&mut self) -> Result<il::ILOperand, ParseError> {
        if let Ok(t) = self.parse_itemp() {
            Ok(il::ILOperand::Temp(ILTemp::NonSSA(t)))
        } else if let Ok(glob) = self.parse_iglob() {
            Ok(il::ILOperand::Global(glob))
        } else if let Ok(i) = exp_tok!(self, int) {
            Ok(il::ILOperand::ConstInt(i))
        } else {
            Err(UnexpectedToken(self.peek().unwrap()))
        }
    }
    /// Parse either an assignment, a call or a store
    fn parse_block_item(&mut self) -> Result<il::ILBlockItem, ParseError> {
        let start = self.idx;

        dbg!(self.idx, self.peek());
        if let Ok((temp, typ, address)) = dbg!(self.parse_block_item_store()) {
            return Ok(il::ILBlockItem::Store(typ, temp, address));
        } else {
            trace!("not a store");
            self.idx = start;
        };
        dbg!(self.idx, self.peek());
        if let Ok(call) = dbg!(self.parse_block_item_call()) {
            return Ok(il::ILBlockItem::Call(call));
        } else {
            trace!("not a call");
            self.idx = start;
        };
        dbg!(self.idx, self.peek());

        if let Ok((temp, typ, instr)) = dbg!(self.parse_block_item_assign()) {
            return Ok(il::ILBlockItem::AssignInstr(temp, typ, instr));
        } else {
            trace!("not an assign");
            self.idx = start;
        }
        let tok = self.peek_res();
        match tok {
            Ok(t) => Err(ParseError::UnexpectedToken(t)),
            Err(e) => Err(e),
        }
    }

    fn parse_block_item_call(&mut self) -> Result<il::ILCall, ParseError> {
        let restore = self.idx;
        let assigns_to = if self.peek_res()?.kind == TokenKind::Percent {
            tok_seq!(self; rst restore;
                TokenKind::Percent,
                TokenKind::Ident(temp),
                TokenKind::Assign,
            );
            let typ = self.parse_basetype(restore)?;
            Some((ILTemp::NonSSA(temp), typ))
        } else {
            None
        };

        tok_seq!(self;rst restore;
            TokenKind::Call,
            TokenKind::Dollar,
            TokenKind::Ident(func),
            TokenKind::ParenOpen,
        );

        let mut args = Vec::new();
        let mut is_variadic: bool = false;
        loop {
            let typ = match self.parse_abitype(restore) {
                Ok(typ) => typ,
                Err(e) => {
                    trace!("dotdotdot");
                    let t = exp_tok!(self, dotdotdot);
                    if t.is_ok() {
                        is_variadic = true;
                        break;
                    } else {
                        return Err(e);
                    }
                }
            };
            let arg = self.parse_operand()?;
            args.push((typ, arg));
            if let Err(_) = exp_tok!(self, ',') {
                break;
            }
        }
        tok_seq!(self;rst restore; TokenKind::ParenClose);
        Ok(il::ILCall {
            assignee: assigns_to,
            func,
            args: args.into_boxed_slice(),
            is_variadic,
        })
    }
    fn parse_block_item_assign(
        &mut self,
    ) -> Result<(il::ILTemp, il::ILType, il::ILInstruction), ParseError> {
        let restore = self.idx;
        trace!("percent name");
        let idx_start = restore;
        let tok = self.peek_res()?;
        let TokenKind::Percent = tok.kind else {
            trace!("exp percent got {tok}");
            self.idx = idx_start;
            return Err(ParseError::UnexpectedToken(tok));
        };
        self.idx += 1;
        let tok = self.peek_res()?;
        let TokenKind::Ident(temp) = tok.kind else {
            self.idx = idx_start;
            return Err(ParseError::UnexpectedToken(tok));
        };
        self.idx += 1;
        let _ = idx_start;
        trace!("is");
        exp_tok!(self, '=')?;
        let typ = dbg!(self.parse_basetype(restore)?);
        let instr = dbg!(self.parse_instr()?);
        Ok((ILTemp::NonSSA(temp), typ, instr))
    }
    fn parse_block_item_store(
        &mut self,
    ) -> Result<(il::ILOperand, il::ILType, il::ILOperand), ParseError> {
        let kw = exp_tok!(self, idstr)?;
        let typ = match kw.as_ref() {
            "storew" => self.tables.typs.get_or_intern(self.target.type_word()),
            "storeh" => self.tables.typs.get_or_intern(self.target.type_halfword()),
            "storel" => self.tables.typs.get_or_intern(self.target.type_long()),
            "stores" => self.tables.typs.get_or_intern(self.target.type_single()),
            "stored" => self.tables.typs.get_or_intern(self.target.type_double()),
            "storeb" => self.tables.typs.get_or_intern(self.target.type_byte()),
            _ => {
                return Err(ParseError::UnknownKeyWord(kw));
            }
        };
        let val = self.parse_operand()?;
        exp_tok!(self, ',')?;
        let addr = self.parse_operand()?;
        Ok((val, typ, addr))
    }

    fn parse_terminator(&mut self) -> Result<il::ILTerminator, ParseError> {
        let restore = self.idx;
        let tok = self.peek_res()?;
        match tok.kind {
            TokenKind::Ret => {
                self.idx += 1;
                if let Ok(val) = self.parse_operand() {
                    Ok(il::ILTerminator::ReturnVal(val))
                } else {
                    Ok(il::ILTerminator::Return)
                }
            }
            TokenKind::Jmp => {
                self.idx += 1;
                let label = self.parse_ilabel()?;
                Ok(il::ILTerminator::Jmp(label))
            }
            TokenKind::Jnz => {
                self.idx += 1;
                let val = self.parse_operand()?;
                tok_seq!(self;rst restore;
                    TokenKind::Comma,
                    TokenKind::At,
                    TokenKind::Ident(btrue),
                    TokenKind::Comma,
                    TokenKind::At,
                    TokenKind::Ident(bfalse),
                );
                Ok(il::ILTerminator::BranchIf(val, btrue, bfalse))
            }
            TokenKind::Hlt => {
                self.idx += 1;
                Ok(il::ILTerminator::Halt)
            }
            _ => Err(ParseError::UnexpectedToken(tok)),
        }
    }

    fn parse_basetype(&mut self, restore: usize) -> Result<il::ILType, ParseError> {
        let kw = exp_tok!(self, idstr)?;
        let typ = match kw.as_ref() {
            "w" | "I32" => self.target.type_word(),
            "l" | "I64" => self.target.type_long(),
            "s" | "F32" => self.target.type_single(),
            "d" | "F64" => self.target.type_double(),
            _ => {
                self.idx = restore;
                return Err(ParseError::UnknownKeyWord(kw));
            }
        };
        let typ = self.tables.typs.get_or_intern(typ);
        Ok(typ)
    }

    fn parse_abitype(&mut self, restore: usize) -> Result<il::ILType, ParseError> {
        if let Ok(kw) = exp_tok!(self, idstr) {
            let typ = match kw.as_ref() {
                "w" | "I32" => self.target.type_word(),
                "l" | "I64" => self.target.type_long(),
                "s" | "F32" => self.target.type_single(),
                "d" | "F64" => self.target.type_double(),

                "h" | "sh" | "uh" | "I16" => self.target.type_halfword(),
                "b" | "sb" | "ub" | "I8" => self.target.type_byte(),
                _ => {
                    self.idx = restore;
                    return Err(ParseError::UnknownKeyWord(kw));
                }
            };
            let typ = self.tables.typs.get_or_intern(typ);
            Ok(typ)
        } else {
            let typname = self.parse_itype()?;
            let typ = self
                .tables
                .typs
                .get_or_intern(il::ILTypeData::Aggregate(typname));

            Ok(typ)
        }
    }

    fn parse_instr(&mut self) -> Result<il::ILInstruction, ParseError> {
        macro_rules! parse_instr_operands {
            (bin $p:path) => {{
                let a = self.parse_operand()?;
                exp_tok!(self, ',')?;
                let b = self.parse_operand()?;
                let val = il::ILInstruction::Binop($p, a, b);
                return Ok(val);
            }};
            (un $p:expr) => {{
                let a = self.parse_operand()?;
                let val = il::ILInstruction::Unop($p, a);
                return Ok(val);
            }};
            (cmp $t:expr, $p:path) => {{
                let a = self.parse_operand()?;
                exp_tok!(self, ',')?;
                let b = self.parse_operand()?;
                let val = il::ILInstruction::Cmp($p, $t, a, b);
                return Ok(val);
            }};
            (load $t:expr) => {{
                let a = self.parse_operand()?;
                let val = il::ILInstruction::Load($t, a);
                return Ok(val);
            }};
        }
        macro_rules! parse_typed_operator {
            (cmp $self:expr, $instr:expr, $kind:literal base, $kindpat:path) => {
                match $instr.as_ref() {
                    concat!($kind, "w") => {
                        parse_instr_operands!(cmp $self.tables.get_type_I32(), $kindpat)
                    },
                    concat!($kind, "l") => {
                        parse_instr_operands!(cmp $self.tables.get_type_I64(), $kindpat)
                   },
                    concat!($kind, "s") => {
                        parse_instr_operands!(cmp $self.tables.get_type_F32(), $kindpat)
                    },
                    concat!($kind, "d") => {
                        parse_instr_operands!(cmp $self.tables.get_type_F64(), $kindpat)
                    },
                    _ => {}
                }

            };
            (cmp $self:expr, $instr:expr, $kind:literal f, $kindpat:path) => {
               match $instr.as_ref() {
                    concat!($kind, "s") => {
                        parse_instr_operands!(cmp $self.tables.get_type_F32(), $kindpat)
                    },
                    concat!($kind, "d") => {
                        parse_instr_operands!(cmp $self.tables.get_type_F64(), $kindpat)
                    },
                    _ => {}
                }

            };
        }

        tok_seq!(self; rst self.idx;TokenKind::Ident(op));
        let instr = crate::intern::unintern_boxstr(op).unwrap();
        match instr.as_ref() {
            "copy" => {
                let op = self.parse_operand()?;
                return Ok(il::ILInstruction::Copy(op));
            }
            "add" => {
                parse_instr_operands!(bin BinOp::Add )
            }
            "sub" => {
                parse_instr_operands!(bin BinOp::Sub )
            }
            "mul" => {
                parse_instr_operands!(bin BinOp::Mul )
            }
            "rem" => {
                parse_instr_operands!(bin BinOp::Rem )
            }
            "div" => {
                parse_instr_operands!(bin BinOp::Div )
            }
            "and" => {
                parse_instr_operands!(bin BinOp::And )
            }
            "neg" => {
                parse_instr_operands!(un UnOp::Neg )
            }
            "or" => {
                parse_instr_operands!(bin BinOp::Or )
            }
            "sar" => {
                parse_instr_operands!(bin BinOp::Sar )
            }
            "shl" => {
                parse_instr_operands!(bin BinOp::Shl )
            }
            "shr" => {
                parse_instr_operands!(bin BinOp::Shr )
            }
            "udiv" => {
                parse_instr_operands!(bin BinOp::Udiv )
            }
            "urem" => {
                parse_instr_operands!(bin BinOp::Urem )
            }
            "xor" => {
                parse_instr_operands!(bin BinOp::Xor )
            }
            "loadb" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_I8())),
            "loadsb" => parse_instr_operands!(load ILABITyp::signed(self.tables.get_type_I8())),
            "loadub" => parse_instr_operands!(load ILABITyp::unsigned(self.tables.get_type_I8())),

            "loadh" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_I16())),
            "loadsh" => parse_instr_operands!(load ILABITyp::signed(self.tables.get_type_I16())),
            "loaduh" => parse_instr_operands!(load ILABITyp::unsigned(self.tables.get_type_I16())),

            "loadw" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_I32())),
            "loadsw" => parse_instr_operands!(load ILABITyp::signed(self.tables.get_type_I32())),
            "loaduw" => parse_instr_operands!(load ILABITyp::unsigned(self.tables.get_type_I32())),

            "loadl" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_I64())),
            "loads" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_F32())),
            "loadd" => parse_instr_operands!(load ILABITyp::nonsigned(self.tables.get_type_F64())),
            "alloc4" => {
                parse_instr_operands!(un UnOp::Alloc4 )
            }
            "alloc8" => {
                parse_instr_operands!(un UnOp::Alloc8 )
            }
            "alloc16" => {
                parse_instr_operands!(un UnOp::Alloc16 )
            }
            "extsw" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::signed(self.tables.get_type_I32())))
            }
            "extuw" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::unsigned(self.tables.get_type_I32())))
            }
            "extsh" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::signed(self.tables.get_type_I16())))
            }
            "extuh" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::unsigned(self.tables.get_type_I16())))
            }
            "extsb" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::signed(self.tables.get_type_I8())))
            }
            "extub" => {
                parse_instr_operands!(un UnOp::Ext(ILABITyp::unsigned(self.tables.get_type_I8())))
            }
            "exts" => {
                parse_instr_operands!(un UnOp::ExtS)
            }
            "tuncd" => {
                parse_instr_operands!(un UnOp::TruncD)
            }
            _ => {}
        };
        match instr.as_ref() {
            "csltw" => {
                trace!("CLSTW PARSED");
                parse_instr_operands!(cmp self.tables.get_type_I32(), CmpOp::Slt)
            }
            "csltl" => {
                parse_instr_operands!(cmp self.tables.get_type_I64(), CmpOp::Slt)
            }
            "cslts" => {
                parse_instr_operands!(cmp self.tables.get_type_F32(), CmpOp::Slt)
            }
            "csltd" => {
                parse_instr_operands!(cmp self.tables.get_type_F64(), CmpOp::Slt)
            }
            _ => {
                trace!("not a slt-kind cmp");
            }
        };
        parse_typed_operator!(cmp self, instr, "cult" base, CmpOp::Ult);
        parse_typed_operator!(cmp self, instr, "csle" base, CmpOp::Slt);
        parse_typed_operator!(cmp self, instr, "cule" base, CmpOp::Ule);

        parse_typed_operator!(cmp self, instr, "ceq" base, CmpOp::Eq);
        parse_typed_operator!(cmp self, instr, "cne" base, CmpOp::Ne);

        parse_typed_operator!(cmp self, instr, "cgt" f, CmpOp::Gt);
        parse_typed_operator!(cmp self, instr, "cge" f, CmpOp::Ge);
        parse_typed_operator!(cmp self, instr, "clt" f, CmpOp::Lt);
        parse_typed_operator!(cmp self, instr, "cle" f, CmpOp::Le);

        parse_typed_operator!(cmp self, instr, "co" f, CmpOp::O);
        parse_typed_operator!(cmp self, instr, "cuo" f, CmpOp::Uo);
        Err(ParseError::UnknownKeyWord(instr))
    }

    fn parse_datadef_items(&mut self) -> Result<(ILDataLayout, Vec<ILDataItem>), ParseError> {
        let restore = self.idx;
        let mut fields = Vec::new();
        let mut items = Vec::new();
        loop {
            let typ = match self.parse_data_item_type_spec(restore) {
                Ok(t) => t,
                Err(_) if let "z" = exp_tok!(self, idstr)?.as_ref() => {
                    tok_seq!(self;rst restore; TokenKind::LitInt(count));
                    let typ_zero = self.tables.get_type_Zero();
                    fields.push(typ_zero);
                    items.push(ILDataItem::Zeroed(count as usize));
                    if exp_tok!(self, ',').is_ok() {
                        continue;
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            };
            while let Ok(item) = self.parse_datadef_item() {
                fields.push(typ);
                items.push(item);
            }
            if exp_tok!(self, ',').is_err() {
                break;
            }
        }
        let layout = ILDataLayout {
            fields,
            layout_kind: ILDataLayoutKind::Packed,
        };

        Ok((layout, items))
    }

    fn parse_data_item_type_spec(&mut self, restore: usize) -> Result<ILType, ParseError> {
        let kw = exp_tok!(self, idstr)?;
        let typ = match kw.as_ref() {
            "w" => self.target.type_word(),
            "l" => self.target.type_long(),
            "s" | "F32" => self.target.type_single(),
            "d" | "F64" => self.target.type_double(),

            "b" => self.target.type_byte(),
            "h" => self.target.type_halfword(),

            "I8" => il::ILTypeData::I8,
            "I16" => il::ILTypeData::I16,
            "I32" => il::ILTypeData::I32,
            "I64" => il::ILTypeData::I64,
            _ => {
                self.idx = restore;
                return Err(ParseError::UnknownKeyWord(kw));
            }
        };
        let typ = self.tables.typs.get_or_intern(typ);
        Ok(typ)
    }

    fn parse_datadef_item(&mut self) -> Result<ILDataItem, ParseError> {
        if let Ok(glob) = self.parse_iglob() {
            Ok(ILDataItem::Global(glob))
        } else if let Ok(i) = exp_tok!(self, int) {
            Ok(ILDataItem::ConstInt(i))
        } else if let Ok(i) = exp_tok!(self, str) {
            Ok(ILDataItem::ConstString(i))
        } else {
            Err(UnexpectedToken(self.peek_res()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn test_advance_while() {
        let s = "bobcat  poop";
        let mut p = Tokenize::new(s);

        let t = p.next_token().unwrap();
        assert_matches!(t.kind, TokenKind::Ident(_));

        let t = p.next_token().unwrap();
        assert_matches!(t.kind, TokenKind::Ws);

        let t = p.next_token().unwrap();
        assert_matches!(t.kind, TokenKind::Ident(_));
    }

    #[test]
    fn test_parse_ident_kinds() {
        let s = "@start
         @b1
             jnz 0, @b2, @b3";
        let _p = Parser::new(s, CompilationTarget::host());
        crate::intern::unintern_boxstr(StringID(4));
        crate::intern::unintern_boxstr(StringID(5));
        crate::intern::unintern_boxstr(StringID(6));
    }
}
