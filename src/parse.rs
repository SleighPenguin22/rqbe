#![allow(unused)]
use idset::internkey;

use crate::il::builder::ILModuleBuilder;

internkey!(StrID);
pub struct Tokenize<'s> {
    pub strings: idset::KeySet<StrID, String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    LitFloat(u32),
    LitDouble(u64),

    Ident(StrID),

    CurlyOpen,
    CurlyClose,
    ParenOpen,
    ParenClose,
}

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

fn valid_ident_str(s: &str) -> bool {
    s.starts_with(char::is_alphabetic) && s.is_ascii()
}

macro_rules! token {
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
    pub fn ident_str(&mut self) -> Result<Option<&str>, ParseError> {
        let start = self.loc;
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if !c.is_alphabetic() {
            return Ok(None);
        }
        self.advance_one()?;
        self.advance_while_char(|c| match c {
            'a'..'z' | 'A'..'Z' | '0'..'9' | '_' | '-' | '.' => true,
            _ => false,
        })?;
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

    pub fn intern_str(&mut self, s: &str) -> StrID {
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
    fn peek_one(&self) -> Option<char> {
        self.source.chars().nth(self.idx())
    }
    fn span_between_states(&self, s1: TokenizerLocation, s2: TokenizerLocation) -> Option<&str> {
        span_between(self.source, s1, s2)
    }
    fn span_between(&self, s: TokenizerLocation) -> Option<&str> {
        span_between(self.source, self.loc, s)
    }

    fn advance_one(&mut self) -> Result<(), ParseError> {
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if self.idx() < self.src_len() {
            self.loc.idx += 1;
            if c == '\n' {
                self.loc.line += 1;
                self.loc.col = 1;
            }
        }
        Ok(())
    }
    fn advance_n(&mut self, n: usize) -> Result<(), ParseError> {
        for _ in 0..n {
            self.advance_one()?;
        }
        Ok(())
    }
    fn advance_while_char<F>(&mut self, f: F) -> Result<&'s str, ParseError>
    where
        F: Fn(char) -> bool,
    {
        let mut start = self.idx();
        loop {
            let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
            if !f(c) {
                break;
            }
            self.advance_one()?;
        }
        Ok(&self.source[start..self.idx()])
    }

    fn advance_if<F>(&mut self, f: F) -> Result<Option<char>, ParseError>
    where
        F: Fn(char) -> bool,
    {
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if f(c) {
            self.advance_one()?;
            Ok(Some(c))
        } else {
            Ok(None)
        }
    }
    fn advance_if_none_or<F>(&mut self, f: F) -> Result<Option<char>, ParseError>
    where
        F: Fn(char) -> bool,
    {
        let c = self.peek_one();

        match c {
            Some(c) => {
                if f(c) {
                    self.advance_one().unwrap();
                    Ok(Some(c))
                } else {
                    Err(ParseError::UnexpectedChar(c))
                }
            }
            None => Ok(None),
        }
    }
    fn advance_exact(&mut self, ch: char) -> Result<char, ParseError> {
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if c == ch {
            self.advance_one()?;
            Ok(c)
        } else {
            Err(ParseError::UnexpectedChar(c))
        }
    }

    fn ws(&mut self) -> Result<(), ParseError> {
        self.advance_while_char(char::is_whitespace).map(|_| ())
    }

    fn advance_while_charset<'set>(&mut self, charset: &'set str) -> Result<&'s str, ParseError> {
        self.advance_while_char(|c| charset.contains(c))
    }

    pub fn next_token(&mut self) -> Option<Token> {
        let locstart = self.loc;
        let c = self.peek_one()?;
        self.advance_one().ok()?;
        match c {
            '%' => token!(self, locstart, TokenKind::Percent),
            '$' => token!(self, locstart, TokenKind::Dollar),
            ':' => token!(self, locstart, TokenKind::Colon),
            '@' => token!(self, locstart, TokenKind::At),
            '=' => token!(self, locstart, TokenKind::Assign),
            ',' => token!(self, locstart, TokenKind::Comma),
            '{' => token!(self, locstart, TokenKind::CurlyOpen),
            '}' => token!(self, locstart, TokenKind::CurlyClose),
            '(' => token!(self, locstart, TokenKind::ParenOpen),
            ')' => token!(self, locstart, TokenKind::ParenClose),
            c if c.is_whitespace() => {
                self.ws().ok()?;
                Some(Token {
                    span: locstart.to_span(self.loc),
                    kind: TokenKind::Ws,
                })
            }
            c if c.is_digit(10) && c != '0' => self.parse_literal_int(locstart),
            _ => self.next_multi_token(c, locstart),
        }
    }

    fn next_multi_token(&mut self, c: char, locstart: TokenizerLocation) -> Option<Token> {
        let c1 = self.peek_one()?;
        self.advance_one().ok()?;
        match (c, c1) {
            ('s', '_') => return self.parse_literal_float(),
            ('d', '_') => return self.parse_literal_double(),
            _ => {}
        };
        self.advance_until_ws().ok()?;
        let s = self.span_between(locstart)?;
        match s {
            "function" => return token!(self, locstart, TokenKind::Function),
            "type" => return token!(self, locstart, TokenKind::Type),
            "data" => return token!(self, locstart, TokenKind::Data),
            "phi" => return token!(self, locstart, TokenKind::Phi),
            "call" => return token!(self, locstart, TokenKind::Call),
            "..." => return token!(self, locstart, TokenKind::DotDotDot),
            "export" => return token!(self, locstart, TokenKind::Export),
            "thread" => return token!(self, locstart, TokenKind::Thread),
            "ret" => return token!(self, locstart, TokenKind::Ret),
            "jmp" => return token!(self, locstart, TokenKind::Jmp),
            "jnz" => return token!(self, locstart, TokenKind::Jnz),
            "hlt" => return token!(self, locstart, TokenKind::Hlt),
            "env" => return token!(self, locstart, TokenKind::Env),
            _ => {}
        }
        let s = s.to_string();
        let ident = self.strings.get_or_intern(s);
        Some(Token {
            span: locstart.to_span(self.loc),
            kind: (TokenKind::Ident(ident)),
        })
    }

    fn advance_until_ws(&mut self) -> Result<(), ParseError> {
        let mut start = self.idx();
        self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        loop {
            let c = self.peek_one();
            if c.is_none() {
                return Ok(());
            } else if c.is_some_and(|c| !char::is_whitespace(c)) {
                self.advance_one()?;
                continue;
            } else {
                break;
            }
        }
        Ok(())
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
        let s = self.advance_while_char(|c| !c.is_whitespace()).ok()?;
        s.parse::<f32>().ok().map(|f| Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::LitFloat(f.to_bits()),
        })
    }
    fn parse_literal_double(&mut self) -> Option<Token> {
        let locstart = self.loc;
        let s = self.advance_while_char(|c| !c.is_whitespace()).ok()?;
        s.parse::<f64>().ok().map(|f| Token {
            span: locstart.to_span(self.loc),
            kind: TokenKind::LitDouble(f.to_bits()),
        })
    }
}

fn span_between(source: &str, s1: TokenizerLocation, s2: TokenizerLocation) -> Option<&str> {
    let (s1, s2) = s1.order(s2);
    source.get(s1.idx..s2.idx)
}

pub struct Parser<'s> {
    ctx: ILModuleBuilder,
    source: &'s str,
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
}
