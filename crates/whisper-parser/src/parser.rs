//! Parser for Whisper source code.
//! Converts a token stream into an AST (Vec<AstNode>).
//!
//! Supports error recovery: when `parse_source_recovering` is used, the
//! parser collects all errors while producing a best-effort partial AST.

use crate::ast::{AstNode, Operator};
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};
use std::rc::Rc;
use whisper_core::value::Value;

/// Error type for parser errors.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub token: Token,
}

/// The Whisper parser.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Accumulated errors (for recovering mode).
    errors: Vec<ParseError>,
    /// Brace nesting depth tracker.  Used to auto-close at EOF.
    brace_depth: usize,
    bracket_depth: usize,
    /// Conditional nesting depth (??...|...] blocks).
    cond_depth: usize,
    /// Stack of bracket_depth values saved when entering each ?? block.
    cond_bracket_stack: Vec<usize>,
    /// Whether we're in recovering mode (collect errors, don't abort).
    recovering: bool,
}

/// Sync-point token kinds — safe places to restart parsing after an error.
fn is_sync_point(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Semicolon
            | TokenKind::RBrace
            | TokenKind::RBracket
            | TokenKind::Import
            | TokenKind::Export
            | TokenKind::Colon
            | TokenKind::Eof
    )
}

impl Parser {
    /// Create a parser from a token stream.
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            errors: Vec::new(),
            brace_depth: 0,
            bracket_depth: 0,
            cond_depth: 0,
            cond_bracket_stack: Vec::new(),
            recovering: false,
        }
    }

    /// Parse source text with error recovery.
    /// Always returns an AST (best-effort).  Errors are collected separately.
    pub fn parse_source_recovering(source: &str) -> (Vec<AstNode>, Vec<ParseError>) {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        parser.recovering = true;
        let nodes = parser.parse_inner();
        (nodes, parser.errors)
    }

    /// Strict parse — returns first error (backward-compatible signature).
    pub fn parse_source(source: &str) -> Result<Vec<AstNode>, ParseError> {
        let (ast, errors) = Self::parse_source_recovering(source);
        if let Some(err) = errors.into_iter().next() {
            // Return the first error; for multiple errors use parse_source_recovering
            Err(err)
        } else {
            Ok(ast)
        }
    }

    /// Parse the token stream into a sequence of AST nodes.
    /// In recovering mode, always succeeds (collects errors internally).
    /// In strict mode, returns first error.
    pub fn parse(&mut self) -> Result<Vec<AstNode>, ParseError> {
        let nodes = self.parse_inner();
        if !self.recovering {
            if let Some(err) = self.errors.pop() {
                return Err(err);
            }
        }
        Ok(nodes)
    }

    /// Core parsing loop — always produces nodes, stores errors internally.
    fn parse_inner(&mut self) -> Vec<AstNode> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            match self.parse_node() {
                Ok(Some(node)) => nodes.push(node),
                Ok(None) => {}
                Err(err) => {
                    self.errors.push(err);
                    // For a stack-based language, each token is independent.
                    // Skip the bad token and continue — don't synchronize
                    // past valid tokens.
                    self.advance();
                }
            }
        }
        // Auto-close any unclosed delimiters and report errors
        if self.brace_depth > 0 {
            self.errors.push(ParseError {
                message: format!(
                    "Unclosed '{{': {} brace(s) still open at end of file",
                    self.brace_depth
                ),
                token: self.synthetic_eof(),
            });
        }
        if self.bracket_depth > 0 {
            self.errors.push(ParseError {
                message: format!(
                    "Unclosed '[': {} bracket(s) still open at end of file",
                    self.bracket_depth
                ),
                token: self.synthetic_eof(),
            });
        }
        nodes = Self::fold_syntax_sugar(nodes);
        nodes
    }

    /// Post-processing pass: fold syntactic sugar patterns into proper AST nodes.
    ///
    /// Patterns:
    ///   [..., Quote(body), Quote(condition), Op(Hash)]  →  [..., Loop { body, condition }]
    ///   [..., Quote(then_body), Op(CondArrow)]          →  [..., CondArrow { then_branch }]
    fn fold_syntax_sugar(nodes: Vec<AstNode>) -> Vec<AstNode> {
        let mut result: Vec<AstNode> = Vec::with_capacity(nodes.len());
        let len = nodes.len();
        let mut i = 0;
        while i < len {
            // Pattern: Quote(body) + Quote(condition) + Op(Hash)  →  Loop
            if i + 2 < len {
                if let (AstNode::Quote(body), AstNode::Quote(condition), AstNode::Op(Operator::Hash)) =
                    (&nodes[i], &nodes[i + 1], &nodes[i + 2])
                {
                    result.push(AstNode::Loop {
                        body: body.clone(),
                        condition: condition.clone(),
                    });
                    i += 3;
                    continue;
                }
            }
            // Pattern: Quote(then_body) + Op(CondArrow)  →  CondArrow { then_branch }
            if i + 1 < len {
                if let (AstNode::Quote(then_body), AstNode::Op(Operator::CondArrow)) =
                    (&nodes[i], &nodes[i + 1])
                {
                    result.push(AstNode::CondArrow {
                        then_branch: then_body.clone(),
                    });
                    i += 2;
                    continue;
                }
            }
            result.push(nodes[i].clone());
            i += 1;
        }
        result
    }

    /// Skip tokens until a sync point is found.
    fn synchronize(&mut self) {
        while !self.is_at_end() {
            // Pop out of any open delimiters as we synchronize
            match &self.current().kind {
                TokenKind::RBrace if self.brace_depth > 0 => {
                    self.brace_depth -= 1;
                }
                TokenKind::RBracket if self.bracket_depth > 0 => {
                    self.bracket_depth -= 1;
                }
                _ => {}
            }
            if is_sync_point(&self.current().kind) {
                // Consume the sync token so we don't re-trigger on it
                self.advance();
                break;
            }
            self.advance();
        }
    }

    /// Parse a single AST node.
    fn parse_node(&mut self) -> Result<Option<AstNode>, ParseError> {
        let token = self.current().clone();

        match &token.kind {
            TokenKind::Eof => Ok(None),

            // Literals
            TokenKind::Integer(n) => {
                self.advance();
                Ok(Some(AstNode::Literal(Value::I64(*n))))
            }
            TokenKind::Float(n) => {
                self.advance();
                Ok(Some(AstNode::Literal(Value::F64(*n))))
            }
            TokenKind::String(s) => {
                let val = Value::Str(Rc::new(s.clone()));
                self.advance();
                Ok(Some(AstNode::Literal(val)))
            }
            TokenKind::BoolTrue => {
                self.advance();
                Ok(Some(AstNode::Literal(Value::Bool(true))))
            }
            TokenKind::BoolFalse => {
                self.advance();
                Ok(Some(AstNode::Literal(Value::Bool(false))))
            }

            // Lists: [ ... ]
            TokenKind::LBracket => {
                self.advance();
                self.bracket_depth += 1;
                let items = self.parse_until_recovering_inner(TokenKind::RBracket, true);
                if self.bracket_depth > 0 {
                    self.bracket_depth -= 1;
                }
                Ok(Some(AstNode::List(items)))
            }

            // Quotations: { ... }
            TokenKind::LBrace => {
                self.advance();
                self.brace_depth += 1;
                let body = self.parse_until_recovering(TokenKind::RBrace);
                if self.brace_depth > 0 {
                    self.brace_depth -= 1;
                }
                Ok(Some(AstNode::Quote(body)))
            }

            // Word definitions: : name { body } ;
            TokenKind::Colon => {
                self.advance();
                let name = match self.recoverable_expect_word() {
                    Ok(n) => n,
                    Err(e) => {
                        self.errors.push(e);
                        return Ok(None);
                    }
                };
                // Expect { body }
                if !matches!(self.current().kind, TokenKind::LBrace) {
                    self.errors.push(ParseError {
                        message: format!(
                            "Expected '{{' after word name '{name}', got {:?}",
                            self.current().kind
                        ),
                        token: self.current().clone(),
                    });
                    self.synchronize();
                    return Ok(None);
                }
                self.advance(); // consume {
                self.brace_depth += 1;
                let body = self.parse_until_recovering(TokenKind::RBrace);
                if self.brace_depth > 0 {
                    self.brace_depth -= 1;
                }
                if !matches!(self.current().kind, TokenKind::Semicolon) {
                    self.errors.push(ParseError {
                        message: format!("Expected ';' after word body for '{name}'"),
                        token: self.current().clone(),
                    });
                    // Don't consume — it might be a sync point
                } else {
                    self.advance(); // consume ;
                }
                Ok(Some(AstNode::Def { name, body }))
            }

            // Conditional: ??true-expr|false-expr]
            TokenKind::CondQ => {
                self.advance();
                self.cond_depth += 1;
                self.cond_bracket_stack.push(self.bracket_depth);
                self.bracket_depth = 0; // reset for inside ??
                let then_branch =
                    self.parse_until_any_recovering(&[TokenKind::Or, TokenKind::RBracket]);
                let else_branch = if matches!(self.current().kind, TokenKind::Or) {
                    self.advance();
                    Some(self.parse_until_recovering(TokenKind::RBracket))
                } else if matches!(self.current().kind, TokenKind::RBracket) {
                    self.advance();
                    None
                } else {
                    // Missing | or ] — recover
                    if !self.is_at_end() {
                        self.errors.push(ParseError {
                            message: format!(
                                "Expected '|' or ']' after conditional, got {:?}",
                                self.current().kind
                            ),
                            token: self.current().clone(),
                        });
                    }
                    None
                };
                if self.cond_depth > 0 {
                    self.cond_depth -= 1;
                }
                if let Some(saved) = self.cond_bracket_stack.pop() {
                    self.bracket_depth = saved;
                }
                Ok(Some(AstNode::Cond {
                    then_branch,
                    else_branch,
                }))
            }

            // Single-branch conditional: cond {then} ?->
            // Emit as Op(CondArrow) — fold_syntax_sugar captures the
            // preceding Quote(then_body) and replaces both with CondArrow node.
            TokenKind::CondArrow => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::CondArrow)))
            }

            // Loop: {body} {cond} #
            TokenKind::Hash => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Hash)))
            }

            // Stack operators
            TokenKind::Dup => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Dup)))
            }
            TokenKind::Swap => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Swap)))
            }
            TokenKind::Rot => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Rot)))
            }
            TokenKind::Pick(n) => {
                let p = *n;
                self.advance();
                Ok(Some(AstNode::Op(Operator::Pick(p))))
            }
            TokenKind::Drop => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Drop)))
            }
            TokenKind::Percent => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Mod)))
            }
            TokenKind::Mod => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Mod)))
            }

            // Arithmetic
            TokenKind::Plus => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Add)))
            }
            TokenKind::Minus => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Sub)))
            }
            TokenKind::Star => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Mul)))
            }
            TokenKind::Slash => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Div)))
            }

            // Comparison
            TokenKind::Eq => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Eq)))
            }
            TokenKind::Lt => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Lt)))
            }
            TokenKind::Gt => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Gt)))
            }
            TokenKind::Neq => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Neq)))
            }
            TokenKind::Le => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Le)))
            }
            TokenKind::Ge => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Ge)))
            }

            // Logic
            TokenKind::And => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::And)))
            }
            TokenKind::Or => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Or)))
            }
            TokenKind::Not => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Not)))
            }

            // List operations
            TokenKind::AtNth => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Nth)))
            }
            TokenKind::Append => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Append)))
            }
            TokenKind::Len => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Len)))
            }
            TokenKind::AtMap => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Map)))
            }
            TokenKind::AtEach => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Each)))
            }
            TokenKind::AtFold => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Fold)))
            }
            TokenKind::AtTimes => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::AtTimes)))
            }

            // String operations
            TokenKind::StrLen => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrLen)))
            }
            TokenKind::StrCat => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrCat)))
            }
            TokenKind::StrSlice => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrSlice)))
            }
            TokenKind::StrEq => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrEq)))
            }
            TokenKind::StrLt => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrLt)))
            }
            TokenKind::StrFind => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrFind)))
            }
            TokenKind::StrReplace => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrReplace)))
            }
            TokenKind::StrToI64 => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrToI64)))
            }
            TokenKind::I64ToStr => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::I64ToStr)))
            }
            TokenKind::StrNth => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrNth)))
            }
            TokenKind::StrChars => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrChars)))
            }
            TokenKind::CharsStr => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::CharsStr)))
            }
            TokenKind::StrIter => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrIter)))
            }
            TokenKind::ListFind => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::ListFind)))
            }
            TokenKind::StrJoin => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::StrJoin)))
            }
            TokenKind::BytesNew => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::BytesNew)))
            }
            TokenKind::BytesPush => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::BytesPush)))
            }
            TokenKind::BytesLen => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::BytesLen)))
            }
            TokenKind::BytesWriteFile => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::BytesWriteFile)))
            }
            TokenKind::Try => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::Try)))
            }

            // Float operations
            TokenKind::I64ToF64 => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::I64ToF64)))
            }
            TokenKind::F64ToI64 => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::F64ToI64)))
            }
            TokenKind::FSqrt => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::FSqrt)))
            }
            TokenKind::FSin => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::FSin)))
            }
            TokenKind::FCos => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::FCos)))
            }
            TokenKind::FTan => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::FTan)))
            }

            // JSON
            TokenKind::JsonParse => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::JsonParse)))
            }
            TokenKind::JsonStringify => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::JsonStringify)))
            }

            // Capability
            TokenKind::CapCall(n) => {
                let id = *n;
                self.advance();
                Ok(Some(AstNode::Op(Operator::CapCall(id))))
            }
            TokenKind::Bang => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::CapExec)))
            }

            // Confidence
            TokenKind::ConfLabel(conf) => {
                let c = *conf;
                self.advance();
                Ok(Some(AstNode::Op(Operator::ConfLabel(c))))
            }
            TokenKind::ProbChoice => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::ProbChoice)))
            }

            // IO
            TokenKind::Dot => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::OutputTop)))
            }
            TokenKind::DotDot => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::OutputAll)))
            }
            TokenKind::Comma => {
                self.advance();
                Ok(Some(AstNode::Op(Operator::ReadInput)))
            }

            // Import/Export
            TokenKind::Import => {
                self.advance();
                let path = match self.recoverable_expect_string() {
                    Ok(s) => s,
                    Err(e) => {
                        self.errors.push(e);
                        return Ok(None);
                    }
                };
                Ok(Some(AstNode::Import(path)))
            }
            TokenKind::Export => {
                self.advance();
                let name = match self.recoverable_expect_word() {
                    Ok(n) => n,
                    Err(e) => {
                        self.errors.push(e);
                        return Ok(None);
                    }
                };
                Ok(Some(AstNode::Export(name)))
            }

            // Word reference
            TokenKind::Word(name) => {
                let n = name.clone();
                self.advance();
                Ok(Some(AstNode::WordRef(n)))
            }

            TokenKind::Semicolon => {
                // Stray semicolon; skip (possible after error recovery)
                self.advance();
                Ok(None)
            }

            TokenKind::Error(msg) => Err(ParseError {
                message: msg.clone(),
                token: token.clone(),
            }),

            other => Err(ParseError {
                message: format!("Unexpected token: {other:?}"),
                token: token.clone(),
            }),
        }
    }

    // === Helper methods ===

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&EOF_TOKEN)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.current().kind, TokenKind::Eof)
    }

    /// Like expect_word but recovers: reports error and returns Err.
    fn recoverable_expect_word(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Word(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            }
            // Builtin operators that can also be used as definition/export names
            TokenKind::StrLen => {
                self.advance();
                Ok("strlen".into())
            }
            TokenKind::StrCat => {
                self.advance();
                Ok("strcat".into())
            }
            TokenKind::StrSlice => {
                self.advance();
                Ok("strslice".into())
            }
            TokenKind::StrEq => {
                self.advance();
                Ok("streq".into())
            }
            TokenKind::StrLt => {
                self.advance();
                Ok("strlt".into())
            }
            TokenKind::StrFind => {
                self.advance();
                Ok("strfind".into())
            }
            TokenKind::StrReplace => {
                self.advance();
                Ok("strreplace".into())
            }
            TokenKind::StrToI64 => {
                self.advance();
                Ok("strtoi64".into())
            }
            TokenKind::I64ToStr => {
                self.advance();
                Ok("i64tostr".into())
            }
            TokenKind::I64ToF64 => {
                self.advance();
                Ok("i64tof64".into())
            }
            TokenKind::F64ToI64 => {
                self.advance();
                Ok("f64toi64".into())
            }
            TokenKind::FSqrt => {
                self.advance();
                Ok("fsqrt".into())
            }
            TokenKind::FSin => {
                self.advance();
                Ok("fsin".into())
            }
            TokenKind::FCos => {
                self.advance();
                Ok("fcos".into())
            }
            TokenKind::FTan => {
                self.advance();
                Ok("ftan".into())
            }
            TokenKind::JsonParse => {
                self.advance();
                Ok("json-parse".into())
            }
            TokenKind::JsonStringify => {
                self.advance();
                Ok("json-stringify".into())
            }
            _ => Err(ParseError {
                message: format!("Expected word, got {:?}", token.kind),
                token,
            }),
        }
    }

    /// Like expect_string but recovers: reports error and returns Err.
    fn recoverable_expect_string(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            TokenKind::Word(w) => {
                let w = w.clone();
                self.advance();
                Ok(w)
            }
            _ => Err(ParseError {
                message: format!("Expected string, got {:?}", token.kind),
                token,
            }),
        }
    }

    /// Parse nodes until a matching end delimiter, with error recovery.
    /// On missing closer: auto-recover at EOF or sync point.
    fn parse_until_recovering(&mut self, end: TokenKind) -> Vec<AstNode> {
        self.parse_until_recovering_inner(end, false)
    }

    /// Parse nodes until end delimiter. `inside_list` = true means we're inside
    /// a `[...]` list and `]` should always close it.
    fn parse_until_recovering_inner(
        &mut self,
        end: TokenKind,
        inside_list: bool,
    ) -> Vec<AstNode> {
        let is_rbracket = std::mem::discriminant(&end)
            == std::mem::discriminant(&TokenKind::RBracket);
        // For RBracket: stop only when not inside a [..] list (bracket_depth == 0).
        // When bracket_depth > 0, ] belongs to the list, not to us.
        // Exception: if inside_list is true, always stop (we're the list parser).
        let can_stop = if is_rbracket && !inside_list {
            self.bracket_depth == 0
        } else {
            true
        };
        if can_stop
            && std::mem::discriminant(&self.current().kind) == std::mem::discriminant(&end)
        {
            self.advance();
            return Vec::new();
        }

        let start_pos = self.pos;
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            let should_stop = if is_rbracket && !inside_list {
                self.bracket_depth == 0
                    && std::mem::discriminant(&self.current().kind)
                        == std::mem::discriminant(&end)
            } else {
                std::mem::discriminant(&self.current().kind) == std::mem::discriminant(&end)
            };
            if should_stop {
                self.advance(); // consume end delimiter
                return Self::fold_syntax_sugar(nodes);
            }
            match self.parse_node() {
                Ok(Some(node)) => nodes.push(node),
                Ok(None) => {}
                Err(err) => {
                    self.errors.push(err);
                    // Don't synchronize inside delimited blocks —
                    // synchronize would consume the closing delimiter
                    self.advance();
                }
            }
        }
        // EOF reached without closing delimiter — already reported by parse_inner
        if self.pos > start_pos {
            self.errors.push(ParseError {
                message: format!("Reached end of file while looking for {end:?}"),
                token: self.synthetic_eof(),
            });
        }
        // Consume the closing ] that we stopped at (parse_until stops before it)
        if is_rbracket && !inside_list
            && matches!(self.current().kind, TokenKind::RBracket)
        {
            self.advance();
        }
        Self::fold_syntax_sugar(nodes)
    }

    /// Parse nodes until any end token, with error recovery.
    fn parse_until_any_recovering(&mut self, ends: &[TokenKind]) -> Vec<AstNode> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            let current_disc = std::mem::discriminant(&self.current().kind);
            let should_stop = ends.iter().any(|e| {
                let e_disc = std::mem::discriminant(e);
                // Don't stop at RBracket if we're inside a [..] list (bracket_depth > 0).
                // The ] belongs to the list, not to the ?? block.
                if e_disc == std::mem::discriminant(&TokenKind::RBracket)
                    && self.bracket_depth > 0
                {
                    false
                } else {
                    e_disc == current_disc
                }
            });
            if should_stop {
                return Self::fold_syntax_sugar(nodes);
            }
            match self.parse_node() {
                Ok(Some(node)) => nodes.push(node),
                Ok(None) => {}
                Err(err) => {
                    self.errors.push(err);
                    self.advance();
                }
            }
        }
        Self::fold_syntax_sugar(nodes)
    }

    fn synthetic_eof(&self) -> Token {
        let last = self.tokens.last().map(|t| t.span.end).unwrap_or(0);
        Token::new(
            TokenKind::Eof,
            crate::token::Span::new(last, last, 0, 0),
            String::new(),
        )
    }
}

/// Sentinel token for EOF position.
static EOF_TOKEN: std::sync::LazyLock<Token> = std::sync::LazyLock::new(|| {
    Token::new(
        TokenKind::Eof,
        crate::token::Span::new(0, 0, 0, 0),
        String::new(),
    )
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_unexpected_char() {
        let (ast, errors) = Parser::parse_source_recovering("3 ^ 4 +");
        // Should still parse the valid parts
        assert!(!errors.is_empty(), "should report error for '^'");
        assert!(!ast.is_empty(), "should produce partial AST");
    }

    #[test]
    fn test_recovery_unclosed_brace() {
        let (ast, errors) = Parser::parse_source_recovering("{ 3 4 +");
        assert!(!errors.is_empty(), "should report unclosed brace");
        assert!(!ast.is_empty(), "should produce partial AST");
    }

    #[test]
    fn test_recovery_unclosed_bracket() {
        let (ast, errors) = Parser::parse_source_recovering("[1 2 3");
        assert!(!errors.is_empty(), "should report unclosed bracket");
        assert!(!ast.is_empty(), "should produce partial AST");
    }

    #[test]
    fn test_recovery_stray_brace() {
        let (ast, errors) = Parser::parse_source_recovering("3 4 + } 5 6 *");
        assert!(!errors.is_empty(), "should report unexpected '}}'");
        // Should still have nodes from before AND after the error
        assert!(ast.len() >= 2, "should parse nodes before and after error");
    }

    #[test]
    fn test_recovery_unterminated_string() {
        let (_ast, errors) = Parser::parse_source_recovering("\"hello 3 4 +");
        assert!(!errors.is_empty(), "should report unterminated string");
    }

    #[test]
    fn test_recovery_valid_after_error() {
        let source = "3 ^ 4 +";
        let (ast, errors) = Parser::parse_source_recovering(source);
        assert!(!errors.is_empty());
        // The "4 +" should have been parsed
        assert!(ast.len() >= 2, "got {ast:?}");
    }

    #[test]
    fn test_recovery_strict_mode_still_works() {
        let result = Parser::parse_source("3 ^ 4 +");
        assert!(result.is_err());
    }

    #[test]
    fn test_recovery_multiple_errors() {
        let (ast, errors) = Parser::parse_source_recovering("^ 1 2 + ^ 3 4 *");
        assert_eq!(errors.len(), 2, "should collect two ^ errors");
        assert!(ast.len() >= 2, "should have nodes from valid parts");
    }

    #[test]
    fn test_recovery_no_errors_valid_input() {
        let (ast, errors) = Parser::parse_source_recovering("3 4 + 5 *");
        assert!(errors.is_empty(), "should have no errors");
        assert_eq!(ast.len(), 5); // 3, 4, +, 5, *
    }

    // ── Syntax sugar folding tests ──────────────────────────────────────

    #[test]
    fn test_fold_loop_syntax() {
        // { 1 + } { _ 10 < } #  →  Loop { body: [1 +], condition: [_ 10 <] }
        let (ast, errors) = Parser::parse_source_recovering("{ 1 + } { _ 10 < } #");
        assert!(errors.is_empty(), "should have no errors, got {errors:?}");
        assert_eq!(ast.len(), 1, "should fold to single Loop node, got {ast:?}");
        match &ast[0] {
            AstNode::Loop { body, condition } => {
                assert_eq!(body.len(), 2); // 1, +
                assert_eq!(condition.len(), 3); // _, 10, <
            }
            other => panic!("expected Loop node, got {other:?}"),
        }
    }

    #[test]
    fn test_fold_loop_mixed_with_other_nodes() {
        // 5 { _ * } { _ 1 > } # .  →  5, Loop { body, condition }, .
        let (ast, errors) =
            Parser::parse_source_recovering("5 { _ * } { _ 1 > } # .");
        assert!(errors.is_empty(), "should have no errors");
        assert_eq!(ast.len(), 3, "5 + Loop + . = 3 nodes, got {ast:?}");
        assert!(matches!(&ast[0], AstNode::Literal(_))); // 5
        assert!(matches!(&ast[1], AstNode::Loop { .. })); // Loop
        assert!(matches!(&ast[2], AstNode::Op(Operator::OutputTop))); // .
    }

    #[test]
    fn test_fold_cond_arrow_syntax() {
        // { 100 } ?->  →  CondArrow { then_branch: [100] }
        let (ast, errors) = Parser::parse_source_recovering("{ 100 } ?->");
        assert!(errors.is_empty(), "should have no errors, got {errors:?}");
        assert_eq!(ast.len(), 1, "should fold to single CondArrow node");
        match &ast[0] {
            AstNode::CondArrow { then_branch } => {
                assert_eq!(then_branch.len(), 1); // 100
            }
            other => panic!("expected CondArrow node, got {other:?}"),
        }
    }

    #[test]
    fn test_loop_folding_preserves_nested_quotes() {
        // Nested: { { 1 } { 2 } # }  →  Quote(Loop)
        // The outer {} should remain a Quote; the inner ones should fold to Loop
        let (ast, errors) =
            Parser::parse_source_recovering("{ { 1 } { 2 } # }");
        assert!(errors.is_empty(), "should have no errors");
        assert_eq!(ast.len(), 1, "should be single Quote node, got {ast:?}");
        match &ast[0] {
            AstNode::Quote(nodes) => {
                assert_eq!(nodes.len(), 1, "inner should be 1 Loop node");
                assert!(matches!(&nodes[0], AstNode::Loop { .. }));
            }
            other => panic!("expected Quote, got {other:?}"),
        }
    }
}
