#![allow(unused)]

use crate::il::builder::ILModuleBuilder;
pub struct Parser<'s> {
    ctx: ILModuleBuilder,
    source: &'s str,
    state: ParseState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ParseState {
    idx: usize,
    line: usize,
    col: usize,
}

impl PartialOrd for ParseState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.idx.cmp(&other.idx))
    }
}
impl Ord for ParseState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.idx.cmp(&other.idx)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEOF,
    UnexpectedChar(char),
}

impl ParseState {
    pub fn new(idx: usize, line: usize, col: usize) -> Self {
        Self { idx, line, col }
    }
    pub fn start() -> Self {
        Self {
            idx: 0,
            line: 1,
            col: 1,
        }
    }
}

fn span_states(s: &str, s1: ParseState, s2: ParseState) -> Option<&str> {
    let (min, max) = if s1 < s2 { (s1, s2) } else { (s2, s1) };
    let range = min.idx..max.idx;
    (max.idx < s.len()).then_some(&s[range])
}

fn valid_ident_str(s: &str) -> bool {
    s.starts_with(char::is_alphabetic) && s.is_ascii()
}

impl<'s> Parser<'s> {
    pub fn ident_str(&mut self) -> Result<Option<&str>, ParseError> {
        let start = self.state;
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if !c.is_alphabetic() {
            return Ok(None);
        }
        self.advance_one()?;
        self.advance_while_char(|c| match c {
            'a'..'z' | 'A'..'Z' | '0'..'9' | '_' | '-' | '.' => true,
            _ => false,
        })?;
        let end = self.state;
        Ok(self.span_between_states(start, end))
    }
}

impl<'s> Parser<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            ctx: ILModuleBuilder::start(),
            source,
            state: ParseState::start(),
        }
    }
    const fn idx(&self) -> usize {
        self.state.idx
    }
    const fn src_len(&self) -> usize {
        self.source.len()
    }
    fn peek_one(&self) -> Option<char> {
        self.source.chars().nth(self.idx())
    }
    fn span_between_states(&self, s1: ParseState, s2: ParseState) -> Option<&str> {
        span_states(self.source, s1, s2)
    }
    fn span_between(&self, s: ParseState) -> Option<&str> {
        span_states(self.source, self.state, s)
    }

    fn advance_one(&mut self) -> Result<(), ParseError> {
        let c = self.peek_one().ok_or(ParseError::UnexpectedEOF)?;
        if self.idx() < self.src_len() {
            self.state.idx += 1;
            if c == '\n' {
                self.state.line += 1;
                self.state.col = 1;
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
    fn advance_while_some_and<F>(&mut self, f: F) -> Result<&'s str, ParseError>
    where
        F: Fn(char) -> bool,
    {
        let mut start = self.idx();
        loop {
            if !self.peek_one().is_some_and(&f) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_advance_while() {
        let s = "bobcat";
        let mut p = Parser::new(s);
        p.advance_while_char(|c| c != 'c').unwrap();
        assert_eq!(p.peek_one(), Some('c'));
        assert_eq!(p.idx(), 3);
    }
}
