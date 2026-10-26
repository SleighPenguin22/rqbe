use crate::il::{self, ILModuleContext, ILTemp, ILType, StringID};
use crate::target::CompilationTarget;
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
    Attribute(StringID),
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

    fn parse_attribute(&mut self) -> Option<Token> {
        let loc = self.loc;
        let s = self.advance_while_char(|c| c != ']').ok()?.to_string();
        let s = self.strings.get_or_intern(s);
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
    (',') => {
        TokenKind::Assign
    };
    (phi) => {
        TokenKind::Phi
    };
    (call) => {
        TokenKind::Call
    };
}

struct ParserInternTables {}

impl Parser {
    pub fn new(source: &str, target: CompilationTarget) -> Self {
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
            target,
        }
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
    fn parse_itemp(&mut self) -> Option<StringID> {
        exp_tok!(self, '%')?;
        self.parse_tok_ident()
    }
    fn parse_glob(&mut self) -> Option<StringID> {
        exp_tok!(self, '$')?;
        self.parse_tok_ident()
    }
    fn parse_ilabel(&mut self) -> Option<StringID> {
        exp_tok!(self, '@')?;
        self.parse_tok_ident()
    }
    fn parse_itype(&mut self) -> Option<StringID> {
        exp_tok!(self, ':')?;
        self.parse_tok_ident()
    }

    fn parse_block(&mut self) -> Option<il::ILBlockData> {
        let label = self.parse_ilabel()?;
        let mut phis = Vec::new();
        let mut items = Vec::new();
        loop {
            let checkpoint = self.idx;
            if let Some(assignee) = self.parse_itemp() {
                exp_tok!(self, =)?;
                let typ = self.parse_basetype()?;
                if let Some(phi) = self.parse_phi() {
                    let temp = il::ILTemp::NonSSA(assignee);
                    phis.push((temp, typ, phi))
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
            if let Some(label) = self.parse_ilabel() {
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
        if let Some(t) = self.parse_itemp() {
            Some(il::ILValue::Temp(ILTemp::NonSSA(t)))
        } else if let Some(i) = exp_tok!(self, int) {
            Some(il::ILValue::ConstInt(i))
        } else {
            todo!()
        }
    }
    /// Parse either an assignment, a call or a store
    fn parse_block_item(&mut self) -> Option<il::ILBlockItem> {
        let start = self.idx;
        if let Some((temp, typ, address)) = self.parse_block_item_store() {
            return Some(il::ILBlockItem::Store(typ, temp, address));
        } else {
            self.idx = start;
        };
        if let Some(call) = self.parse_block_item_call() {
            return Some(il::ILBlockItem::Call(call));
        } else {
            self.idx = start;
        }
        if let Some((temp, typ, instr)) = self.parse_block_item_assign() {
            return Some(il::ILBlockItem::AssignInstr(temp, typ, instr));
        } else {
            self.idx = start;
            return None;
        }
    }

    fn parse_kw(&mut self) -> Option<StringID> {
        if let TokenKind::Ident(kw) = self.peek()?.kind {
            self.idx += 1;
            Some(kw)
        } else {
            return None;
        }
    }

    fn parse_block_item_call(&mut self) -> Option<il::ILCall> {
        let assigns_to = if let Some(temp) = self.parse_itemp() {
            let temp = ILTemp::NonSSA(temp);
            exp_tok!(self, =)?;
            let typ = self.parse_abitype()?;
            Some((temp, typ))
        } else {
            None
        };
        exp_tok!(self, call)?;
        let func = self.parse_kw()?;
        exp_tok!(self, '(')?;
        let mut args = Vec::new();
        loop {
            if let Some(typ) = self.parse_abitype() {
                let arg = self.parse_val()?;
                args.push((typ, arg));
            } else {
                break;
            }

            // if exp_tok!(self, ',').is_none() {
            //     break;
            // }
        }
        exp_tok!(self, ')')?;
        Some(il::ILCall {
            assignsee: assigns_to,
            func,
            args,
        })
    }
    fn parse_block_item_assign(&self) -> Option<(il::ILTemp, il::ILType, il::ILValInstr)> {
        todo!()
    }
    fn parse_block_item_store(&mut self) -> Option<(il::ILValue, il::ILType, il::ILValue)> {
        let kw = self.parse_kw()?;
        let kw = self.tables.strings.get_by_id(kw)?;
        let typ = match kw.as_str() {
            "storew" => self.tables.typs.get_or_intern(self.target.type_word()),
            "storeh" => self.tables.typs.get_or_intern(self.target.type_halfword()),
            "storel" => self.tables.typs.get_or_intern(self.target.type_long()),
            "stores" => self.tables.typs.get_or_intern(self.target.type_single()),
            "stored" => self.tables.typs.get_or_intern(self.target.type_double()),
            "storeb" => self.tables.typs.get_or_intern(self.target.type_byte()),
            _ => {
                return None;
            }
        };
        let val = self.parse_val()?;
        exp_tok!(self, ',')?;
        let addr = self.parse_val()?;
        Some((val, typ, addr))
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
                let label = self.parse_ilabel()?;
                Some(il::ILTerminator::Jmp(label))
            }
            TokenKind::Jnz => {
                self.idx += 1;
                let val = self.parse_val()?;
                exp_tok!(self, ',')?;
                let btrue = self.parse_ilabel()?;
                exp_tok!(self, ',')?;
                let bfalse = self.parse_ilabel()?;
                Some(il::ILTerminator::BranchIf(val, btrue, bfalse))
            }
            TokenKind::Hlt => {
                self.idx += 1;
                Some(il::ILTerminator::Halt)
            }
            _ => None,
        }
    }

    fn parse_basetype(&mut self) -> Option<il::ILType> {
        let kw = self.parse_kw()?;
        let kw = self.tables.strings.get_by_id(kw)?;
        let typ = match kw.as_str() {
            "w" => self.target.type_word(),
            "l" => self.target.type_long(),
            "s" => self.target.type_single(),
            "d" => self.target.type_double(),
            _ => {
                return None;
            }
        };
        let typ = self.tables.typs.get_or_intern(typ);
        Some(typ)
    }
    fn parse_exttype(&mut self) -> Option<il::ILType> {
        let kw = self.parse_kw()?;
        let kw = self.tables.strings.get_by_id(kw)?;
        let typ = match kw.as_str() {
            "w" => self.target.type_word(),
            "h" => self.target.type_halfword(),
            "l" => self.target.type_long(),
            "s" => self.target.type_single(),
            "d" => self.target.type_double(),
            "b" => self.target.type_byte(),
            _ => {
                return None;
            }
        };
        let typ = self.tables.typs.get_or_intern(typ);
        Some(typ)
    }

    fn parse_abitype(&mut self) -> Option<il::ILType> {
        if let Some(kw) = self.parse_kw() {
            let kw = self.tables.strings.get_by_id(kw)?;
            let typ = match kw.as_str() {
                "w" => self.target.type_word(),
                "h" => self.target.type_halfword(),
                "l" => self.target.type_long(),
                "s" => self.target.type_single(),
                "d" => self.target.type_double(),
                "b" => self.target.type_byte(),
                "sb" => self.target.type_byte(),
                "ub" => self.target.type_byte(),
                "uh" => self.target.type_halfword(),
                "sh" => self.target.type_halfword(),
                _ => {
                    return None;
                }
            };
            let typ = self.tables.typs.get_or_intern(typ);
            Some(typ)
        } else {
            let typname = self.parse_itype()?;
            let typ = self
                .tables
                .typs
                .get_or_intern(il::ILTypeData::Aggregate(typname));

            Some(typ)
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
        let s = ":poop %cat $shimmy @dog";
        let mut p = Parser::new(s, CompilationTarget::host());
        let t = p.parse_itemp();
        assert_eq!(t, None);

        let t = p.parse_itype().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "poop");

        let t = p.parse_itype();
        assert_eq!(t, None);

        let t = p.parse_itemp().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "cat");

        let t = p.parse_itemp();
        assert_eq!(t, None);

        let t = p.parse_glob().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "shimmy");

        let t = p.parse_glob();
        assert_eq!(t, None);

        let t = p.parse_ilabel().unwrap();
        assert_eq!(p.tables.strings.get_by_id(t).unwrap(), "dog");

        let t = p.parse_ilabel();
        assert_eq!(t, None);
    }
}
