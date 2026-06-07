//! Lexer for Whisper source code (.ws format).
//! Tokenizes source text into a stream of tokens.

use crate::token::{Span, Token, TokenKind};

/// Error type for lexer errors.
#[derive(Debug, Clone, PartialEq)]
pub struct LexerError {
    pub message: String,
    pub span: Span,
}

/// The lexer converts Whisper source text into a token stream.
pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire source into a vector of tokens.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        if self.pos >= self.source.len() {
            return self.make_token(TokenKind::Eof, String::new());
        }

        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.column;
        let ch = self.current();

        let kind = match ch {
            // Delimiters
            '[' => {
                self.advance();
                TokenKind::LBracket
            }
            ']' => {
                self.advance();
                TokenKind::RBracket
            }
            '{' => {
                self.advance();
                TokenKind::LBrace
            }
            '}' => {
                self.advance();
                TokenKind::RBrace
            }

            // String literals
            '"' => self.read_string(),

            // Numbers
            '0'..='9' => self.read_number(),

            // Single-character operators and symbols
            '-' => self.read_number(),
            '_' => {
                self.advance();
                TokenKind::Dup
            }
            '`' => {
                self.advance();
                TokenKind::Swap
            }
            '%' => {
                self.advance();
                // Context-dependent: could be Drop or Mod
                // The parser disambiguates based on context
                TokenKind::Percent
            }
            '+' => {
                self.advance();
                TokenKind::Plus
            }
            '*' => {
                self.advance();
                TokenKind::Star
            }
            '/' => {
                self.advance();
                TokenKind::Slash
            }
            '=' => {
                self.advance();
                TokenKind::Eq
            }
            '<' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Le
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            '&' => {
                self.advance();
                TokenKind::And
            }
            '|' => {
                self.advance();
                TokenKind::Or
            }
            '!' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Neq
                } else {
                    TokenKind::Not
                }
            }
            ':' => {
                self.advance();
                // Could be Colon (word definition) or ConfLabel
                // Check for number after :
                if self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                    let conf = self.read_confidence_suffix();
                    TokenKind::ConfLabel(conf)
                } else {
                    TokenKind::Colon
                }
            }
            ';' => {
                self.advance();
                TokenKind::Semicolon
            }
            '#' => {
                self.advance();
                if self.peek() == Some('t') {
                    self.advance();
                    TokenKind::BoolTrue
                } else if self.peek() == Some('f') {
                    self.advance();
                    TokenKind::BoolFalse
                } else {
                    TokenKind::Hash
                }
            }
            '?' => {
                self.advance();
                if self.peek() == Some('?') {
                    self.advance();
                    TokenKind::CondQ
                } else if self.peek() == Some('|') {
                    self.advance();
                    TokenKind::ProbChoice
                } else if self.peek() == Some('-') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        TokenKind::CondArrow
                    } else {
                        TokenKind::Error("Expected ?> or ?->".into())
                    }
                } else {
                    TokenKind::Error("Unknown ? sequence".into())
                }
            }
            '@' => {
                self.advance();
                self.read_at_word()
            }
            '.' => {
                self.advance();
                if self.peek() == Some('.') {
                    self.advance();
                    TokenKind::DotDot
                } else {
                    TokenKind::Dot
                }
            }
            ',' => {
                self.advance();
                TokenKind::Comma
            }
            '$' => {
                self.advance();
                let n = self.read_digits();
                TokenKind::Pick(n.parse().unwrap_or(0))
            }

            // Word references (identifiers)
            c if c.is_alphabetic() => self.read_word(),

            _ => {
                self.advance();
                TokenKind::Error(format!("Unexpected character: '{ch}'"))
            }
        };

        let lexeme: String = self.source[start_pos..self.pos].iter().collect();
        let span = Span::new(start_pos, self.pos, start_line, start_col);
        Token::new(kind, span, lexeme)
    }

    // === Helper methods ===

    fn current(&self) -> char {
        self.source[self.pos]
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn advance(&mut self) {
        if self.pos < self.source.len() {
            if self.source[self.pos] == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.pos += 1;
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.source.len() {
            match self.source[self.pos] {
                ' ' | '\t' | '\r' | '\n' => self.advance(),
                '/' if self.peek_at(1) == Some('/') => {
                    // Line comment: // ...
                    while self.pos < self.source.len() && self.source[self.pos] != '\n' {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn make_token(&self, kind: TokenKind, lexeme: String) -> Token {
        Token::new(kind, Span::new(self.pos, self.pos, self.line, self.column), lexeme)
    }

    fn read_string(&mut self) -> TokenKind {
        self.advance(); // skip opening "
        let mut s = String::new();
        while self.pos < self.source.len() {
            let ch = self.current();
            if ch == '"' {
                self.advance();
                return TokenKind::String(s);
            }
            if ch == '\\' {
                self.advance();
                if self.pos < self.source.len() {
                    let escaped = self.current();
                    match escaped {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        c => {
                            s.push('\\');
                            s.push(c);
                        }
                    }
                    self.advance();
                }
            } else {
                s.push(ch);
                self.advance();
            }
        }
        TokenKind::Error("Unterminated string literal".into())
    }

    fn read_number(&mut self) -> TokenKind {
        let is_neg = self.current() == '-';
        if is_neg {
            self.advance();
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return TokenKind::Minus;
            }
        }
        let mut num_str = String::new();
        if is_neg {
            num_str.push('-');
        }
        let mut is_float = false;
        while self.pos < self.source.len() {
            let ch = self.current();
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else if ch == '.' {
                is_float = true;
                num_str.push('.');
                self.advance();
            } else {
                break;
            }
        }
        if is_float {
            TokenKind::Float(num_str.parse().unwrap_or(0.0))
        } else {
            TokenKind::Integer(num_str.parse().unwrap_or(0))
        }
    }

    fn read_digits(&mut self) -> String {
        let mut s = String::new();
        while self.pos < self.source.len() && self.current().is_ascii_digit() {
            s.push(self.current());
            self.advance();
        }
        s
    }

    fn read_confidence_suffix(&mut self) -> f64 {
        let num = self.read_digits();
        if self.peek() == Some('.') {
            self.advance();
            let frac = self.read_digits();
            format!("{num}.{frac}").parse().unwrap_or(1.0)
        } else {
            num.parse().unwrap_or(1.0)
        }
    }

    fn read_word(&mut self) -> TokenKind {
        let mut name = String::new();
        while self.pos < self.source.len() {
            let ch = self.current();
            if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/' {
                name.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        match name.as_str() {
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "append" => TokenKind::Append,
            "len" => TokenKind::Len,
            "drop" => TokenKind::Drop,
            "mod" => TokenKind::Mod,
            "strlen" => TokenKind::StrLen,
            "strcat" => TokenKind::StrCat,
            "strslice" => TokenKind::StrSlice,
            "streq" => TokenKind::StrEq,
            "strlt" => TokenKind::StrLt,
            "strfind" => TokenKind::StrFind,
            "strreplace" => TokenKind::StrReplace,
            "strtoi64" => TokenKind::StrToI64,
            "i64tostr" => TokenKind::I64ToStr,
            "i64tof64" => TokenKind::I64ToF64,
            "f64toi64" => TokenKind::F64ToI64,
            "fsqrt" => TokenKind::FSqrt,
            "fsin" => TokenKind::FSin,
            "fcos" => TokenKind::FCos,
            "ftan" => TokenKind::FTan,
            "json-parse" => TokenKind::JsonParse,
            "json-stringify" => TokenKind::JsonStringify,
            _ => TokenKind::Word(name),
        }
    }

    fn read_at_word(&mut self) -> TokenKind {
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            let num = self.read_digits();
            return TokenKind::CapCall(num.parse().unwrap_or(0));
        }
        let name = self.read_identifier();
        match name.as_str() {
            "nth" => TokenKind::AtNth,
            "map" => TokenKind::AtMap,
            "each" => TokenKind::AtEach,
            "fold" => TokenKind::AtFold,
            "times" => TokenKind::AtTimes,
            _ => TokenKind::Word(format!("@{name}")),
        }
    }

    fn read_identifier(&mut self) -> String {
        let mut name = String::new();
        while self.pos < self.source.len() {
            let ch = self.current();
            if ch.is_alphanumeric() || ch == '_' {
                name.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        name
    }
}
