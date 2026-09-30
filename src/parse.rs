use idset::internkey;

use crate::il::{self, ILModuleContext, ILPhiNode, ILTemp, StringID};

pub struct Tokenize<'s> {
    pub strings: idset::KeySet<StringID, String>,
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
    span: TokenSpan,
    pub kind: TokenKind,
}

impl Token {
    pub fn to_string(self, tokenizer: &Tokenize) -> String {
        let binding = self.kind.to_string();
        let kind = match self.kind {
            TokenKind::Ident(i) => tokenizer.strings.get_by_id(i).unwrap().as_str(),
            TokenKind::Comment(i) => tokenizer.strings.get_by_id(i).unwrap().as_str(),
            _ => binding.as_str(),
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

    Ident(StringID),

    CurlyOpen,
    CurlyClose,
    ParenOpen,
    ParenClose,
    Comment(StringID),
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
    line: usize,
    col: usize,
    len: usize,
}

impl TokenSpan {
    pub fn view(self, source: &str) -> Option<&str> {
        source.get(self.start..self.start + self.len)
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEOF,
    UnexpectedChar(char),
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
            if let Some('a'..'z' | 'A'..'Z' | '0'..'9' | '_' | '$' | '.') = self.peek() {
                self.advance()?;
            } else {
                break;
            }
        }
        let end = self.loc;
        Ok(self.span_between_states(start, end))
    }
}

impl<'s> Tokenize<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            loc: TokenizerLocation::start(),
            strings: idset::KeySet::new(),
        }
    }

    pub fn intern_str(&mut self, s: &str) -> StringID {
        match self.strings.get_id_of(s) {
            Some(id) => id,
            None => self.strings.get_or_intern(s.to_string()),
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
            ')' => emit_token!(self, locstart, TokenKind::ParenClose),
            c if c.is_whitespace() => {
                self.ws().ok()?;
                Some(Token {
                    span: locstart.to_span(self.loc),
                    kind: TokenKind::Ws,
                })
            }
            '1'..='9' => self.parse_literal_int(locstart),
            '#' => self.parse_line_comment(locstart),
            _ => self.next_multi_token(c, locstart),
        }
    }

    fn next_multi_token(&mut self, c: char, locstart: TokenizerLocation) -> Option<Token> {
        let c1 = self.peek()?;
        match (c, c1) {
            ('s', '_') => return self.parse_literal_float(),
            ('d', '_') => return self.parse_literal_double(),
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
        let ident = self.strings.get_or_intern(s);
        Some(Token {
            span: locstart.to_span(self.loc),
            kind: (TokenKind::Ident(ident)),
        })
    }

    fn parse_literal_int(&mut self, locstart: TokenizerLocation) -> Option<Token> {
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
        let id = self.strings.get_or_intern(s);
        Some(Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::Comment(id),
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
}

macro_rules! exp_tok {
    ($self:expr, int) => {
        if let TokenKind::LitInt(i) = $self.peek()?.kind {
            $self.idx += 1;
            Some(i)
        } else {
            None
        }
    };
    ($self:expr, $c:tt) => {
        if let tokenchar_kind!($c) = $self.peek()?.kind {
            $self.idx += 1;
            Some(())
        } else {
            None
        }
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
    (=) => {
        TokenKind::Assign
    };
    (phi) => {
        TokenKind::Phi
    };
    (litint $i:ident) => {
        TokenKind::LitInt($i)
    };
}

struct ParserInternTables {}

impl Parser {
    pub fn new(source: &str) -> Self {
        let mut t = Tokenize::new(source);
        let mut tokens = vec![];
        while let Some(tok) = t.next_token() {
            if let TokenKind::Ws = tok.kind {
                continue;
            } else {
                tokens.push(tok);
            }
        }
        let mut tables = ILModuleContext::with_capacity(32);
        tables.strings = t.strings;
        Self {
            tables,
            tokens: tokens,
            idx: 0,
        }
    }

    fn cur(&self) -> Token {
        self.tokens[self.idx]
    }

    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.idx).copied()
    }

    fn parse_tok_ident(&mut self) -> Option<StringID> {
        if let TokenKind::Ident(id) = self.peek()?.kind {
            self.idx += 1;
            Some(id)
        } else {
            None
        }
    }
    fn parse_temp(&mut self) -> Option<StringID> {
        exp_tok!(self, '%')?;
        self.parse_tok_ident()
    }
    fn parse_glob(&mut self) -> Option<StringID> {
        exp_tok!(self, '$')?;
        self.parse_tok_ident()
    }
    fn parse_label(&mut self) -> Option<StringID> {
        exp_tok!(self, '@')?;
        self.parse_tok_ident()
    }
    fn parse_type(&mut self) -> Option<StringID> {
        exp_tok!(self, ':')?;
        self.parse_tok_ident()
    }

    fn parse_block(&mut self) -> Option<il::ILBlockData> {
        let label = self.parse_label()?;
        let mut phis = Vec::new();
        let mut items = Vec::new();
        loop {
            let checkpoint = self.idx;
            if let Some(assignee) = self.parse_temp() {
                exp_tok!(self, =)?;
                let typ = self.parse_basetype()?;
                if let Some(phi) = self.parse_phi() {
                    let temp = il::ILTemp::NonSSA(assignee);
                    phis.push((temp, phi))
                } else {
                    self.idx = checkpoint;
                    break;
                }
            }
        }
        while let Some(instr) = self.parse_block_item() {
            items.push(instr);
        }
        let terminator = self.parse_terminator()?;
        Some(il::ILBlockData {
            label,
            phis,
            items,
            terminator,
        })
    }

    fn parse_phi(&mut self) -> Option<il::ILPhiNode> {
        exp_tok!(self, phi)?;
        let mut incoming = Vec::new();
        loop {
            if let Some(label) = self.parse_label() {
                let val = self.parse_val()?;
                incoming.push((label, val));
            } else {
                break;
            }
        }
        let phi = il::ILPhiNodeData { incoming };
        Some(self.tables.phi_nodes.get_or_intern(phi))
    }

    fn parse_val(&mut self) -> Option<il::ILValue> {
        if let Some(t) = self.parse_temp() {
            Some(il::ILValue::Assignee(ILTemp::NonSSA(t)))
        } else if let Some(i) = exp_tok!(self, int) {
            Some(il::ILValue::ConstInt(i))
        } else {
            todo!()
        }
    }

    fn parse_block_item(&self) -> Option<il::ILBlockItem> {
        todo!()
    }

    fn parse_terminator(&mut self) -> Option<il::ILTerminator> {
        match self.peek()?.kind {
            TokenKind::Ret => {
                self.idx += 1;
                if let Some(val) = self.parse_val() {
                    Some(il::ILTerminator::ReturnVal(val))
                } else {
                    Some(il::ILTerminator::Return)
                }
            }
            TokenKind::Jmp => {
                self.idx += 1;
                let label = self.parse_label()?;
                Some(il::ILTerminator::Jmp(label))
            }
            TokenKind::Jnz => {
                self.idx += 1;
                let val = self.parse_val()?;
                exp_tok!(self, ',')?;
                let btrue = self.parse_label()?;
                exp_tok!(self, ',')?;
                let bfalse = self.parse_label()?;
                Some(il::ILTerminator::BranchIf(val, btrue, bfalse))
            }
            TokenKind::Hlt => {
                self.idx += 1;
                Some(il::ILTerminator::Halt)
            }
            _ => None,
        }
    }

    fn parse_basetype(&self) -> Option<il::ILType> {
        todo!()
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
        let s = ":poop %cat $shimmy @dog";
        let mut p = Parser::new(s);
        let t = p.parse_temp();
        assert_eq!(t, None);

        let t = p.parse_type().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "poop");

        let t = p.parse_type();
        assert_eq!(t, None);

        let t = p.parse_temp().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "cat");

        let t = p.parse_temp();
        assert_eq!(t, None);

        let t = p.parse_glob().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "shimmy");

        let t = p.parse_glob();
        assert_eq!(t, None);

        let t = p.parse_label().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "dog");

        let t = p.parse_label();
        assert_eq!(t, None);
    }
}
