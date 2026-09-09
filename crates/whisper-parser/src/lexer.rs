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
                } else if self.peek() == Some('<') {
                    self.advance();
                    TokenKind::Shl
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Ge
                } else if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Shr
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
                // 限定量词法（J-4）：点前后均为词字符 → 归并进词
                // （str.find）。到达此分支说明 '.' 是 token 起点——
                // 前词已在 read_word 内自洽处理点号，这里只可能是
                // 输出(.) / OutputAll(..) / 限定词首字符非法情形。
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
        Token::new(
            kind,
            Span::new(self.pos, self.pos, self.line, self.column),
            lexeme,
        )
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
            // '$' is Pick only at token start; mid-word it is part of the
            // name (e.g. `is-$n?` in whisperc).
            // '>' and '<' are comparison ops only at token start;
            // mid-word they are part of the name (e.g. i64>f64).
            if ch.is_alphanumeric()
                || ch == '_'
                || ch == '-'
                || ch == '/'
                || ch == '?'
                || ch == '$'
                || ch == '>'
                || ch == '<'
                || ch == '!'
            {
                name.push(ch);
                self.advance();
            } else if ch == '.'
                && matches!(
                    self.source.get(self.pos + 1),
                    Some(c)
                        if c.is_alphanumeric()
                            || *c == '_'
                            || *c == '-'
                            || *c == '?'
                )
                && !name.is_empty()
            {
                // 限定量（J-4）：mod.word —— 点在词中间、后随词字符
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
            "prepend" => TokenKind::Prepend,
            "len" => TokenKind::Len,
            "drop" => TokenKind::Drop,
            "dup" => TokenKind::Dup,
            "swap" => TokenKind::Swap,
            "rot" => TokenKind::Rot,
            "over" => TokenKind::Pick(1),
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
            "strnth" => TokenKind::StrNth,
            "strchars" => TokenKind::StrChars,
            "charsstr" => TokenKind::CharsStr,
            "striter" => TokenKind::StrIter,
            "listfind" => TokenKind::ListFind,
            "strjoin" => TokenKind::StrJoin,
            "strsplit" => TokenKind::StrSplit,
            "ctos" => TokenKind::Ctos,
            "chr" => TokenKind::Ctos,
            "listslice" => TokenKind::ListSlice,
            "list-slice" => TokenKind::ListSlice,
            "args" => TokenKind::Args,
            "shl" => TokenKind::Shl,
            "shr" => TokenKind::Shr,
            "listen" => TokenKind::Listen,
            "accept" => TokenKind::Accept,
            "recv" => TokenKind::Recv,
            "send" => TokenKind::Send,
            "close" => TokenKind::CloseSock,
            "read-file" => TokenKind::ReadFile,
            "write-file" => TokenKind::WriteFile,
            "getenv" => TokenKind::EnvGet,
            "op-effect" => TokenKind::OpEffect,
            "size-of" => TokenKind::QuoteSize,
            "word-size" => TokenKind::WordSize,
            "seed!" => TokenKind::SetSeed,
            "rand" => TokenKind::Rand,
            "conf-of" => TokenKind::ConfOf,
            "unwrap" => TokenKind::UnwrapSig,
            "kv-set" => TokenKind::KvSet,
            "kv-get" => TokenKind::KvGet,
            "kv-del" => TokenKind::KvDel,
            "kv-keys" => TokenKind::KvKeys,
            "serve-concurrent" => TokenKind::ServeConcurrent,
            "read-line" => TokenKind::ReadLine,
            "str-upper" => TokenKind::StrUpper,
            "str-lower" => TokenKind::StrLower,
            "str-strip" => TokenKind::StrStrip,
            "fmt" => TokenKind::Fmt,
            "listdir" => TokenKind::ListDir,
            "mkdir" => TokenKind::Mkdir,
            "file-exists" => TokenKind::FileExists,
            "strftime" => TokenKind::StrfTime,
            "leave" => TokenKind::Leave,
            "strmatch" => TokenKind::StrMatch,
            "strfind-re" => TokenKind::StrFindRe,
            "pack" => TokenKind::Pack,
            "unpack" => TokenKind::Unpack,
            "range" => TokenKind::Range,
            "ffi-open" => TokenKind::FfiOpen,
            "ffi-sym" => TokenKind::FfiSym,
            "ffi-call" => TokenKind::FfiCall,
            "buf-alloc" => TokenKind::BufAlloc,
            "buf-free" => TokenKind::BufFree,
            "buf-peek" => TokenKind::BufPeek,
            "buf-poke" => TokenKind::BufPoke,
            "buf->str" => TokenKind::BufToStr,
            "split-cond" => TokenKind::SplitCond,
            "list?" => TokenKind::ListCheck,
            "i64?" => TokenKind::I64Check,
            "str?" => TokenKind::StrCheck,
            "bool?" => TokenKind::BoolCheck,
            "return" => TokenKind::Return,
            "map-new" => TokenKind::MapNew,
            "map-get" => TokenKind::MapGet,
            "map-set" => TokenKind::MapSet,
            "map-del" => TokenKind::MapDel,
            "map-keys" => TokenKind::MapKeys,
            "map-has" => TokenKind::MapHas,
            "bytes-new" => TokenKind::BytesNew,
            "bytes-push" => TokenKind::BytesPush,
            "bytes-len" => TokenKind::BytesLen,
            "bytes-write" => TokenKind::BytesWriteFile,
            "try" => TokenKind::Try,
            "i64tof64" => TokenKind::I64ToF64,
            "f64toi64" => TokenKind::F64ToI64,
            "fsqrt" => TokenKind::FSqrt,
            "fsin" => TokenKind::FSin,
            "fcos" => TokenKind::FCos,
            "ftan" => TokenKind::FTan,
            "json-parse" => TokenKind::JsonParse,
            "json-stringify" => TokenKind::JsonStringify,
            "now" => TokenKind::Now,
            "sleep" => TokenKind::Sleep,
            "random" => TokenKind::Random,
            "randint" => TokenKind::RandInt,
            "floor" => TokenKind::Floor,
            "ceil" => TokenKind::Ceil,
            "round" => TokenKind::Round,
            "log" => TokenKind::Log,
            "exp" => TokenKind::Exp,
            "atan2" => TokenKind::Atan2,
            "bitand" => TokenKind::BitAnd,
            "bitor" => TokenKind::BitOr,
            "bitxor" => TokenKind::BitXor,
            "bitnot" => TokenKind::BitNot,
            "sort" => TokenKind::Sort,
            "rev" => TokenKind::Rev,
            "dedup" => TokenKind::Dedup,
            "take" => TokenKind::Take,
            "drop-n" => TokenKind::DropN,
            _ => TokenKind::Word(name),
        }
    }

    fn read_at_word(&mut self) -> TokenKind {
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            let num = self.read_digits();
            return TokenKind::CapCall(num.parse().unwrap_or(0));
        }
        let name = self.read_identifier();
        if name.is_empty() {
            return TokenKind::Rot;
        }
        match name.as_str() {
            "nth" => TokenKind::AtNth,
            "map" => TokenKind::AtMap,
            "each" => TokenKind::AtEach,
            "fold" => TokenKind::AtFold,
            "filter" => TokenKind::AtFilter,
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

#[cfg(test)]
mod qualified_word_tests {
    use super::*;

    fn kinds(src: &str) -> Vec<String> {
        let mut lx = Lexer::new(src);
        let ts = lx.tokenize();
        let mut out: Vec<String> = Vec::new();
        for t in &ts {
            out.push(format!("{:?}", t.kind));
        }
        out
    }

    #[test]
    fn qualified_word_single_token() {
        assert_eq!(kinds("mmod.mw"), vec!["Word(\"mmod.mw\")".to_string(), "Eof".to_string()]);
    }

    #[test]
    fn standalone_dot_still_output() {
        assert_eq!(kinds("a ."), vec!["Word(\"a\")".to_string(), "Dot".to_string(), "Eof".to_string()]);
    }

    #[test]
    fn float_literal_untouched() {
        assert_eq!(
            kinds("3.14"),
            vec!["Float(3.14)".to_string(), "Eof".to_string()]
        );
    }

    #[test]
    fn dotdot_untouched() {
        assert_eq!(
            kinds(".."),
            vec!["DotDot".to_string(), "Eof".to_string()]
        );
    }

    #[test]
    fn trailing_dot_not_word() {
        assert_eq!(
            kinds("a."),
            vec!["Word(\"a\")".to_string(), "Dot".to_string(), "Eof".to_string()]
        );
    }
}
