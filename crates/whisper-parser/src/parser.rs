//! Parser for Whisper source code.
//! Converts a token stream into an AST (Vec<AstNode>).

use crate::ast::{AstNode, Operator};
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};
use whisper_core::value::Value;
use std::rc::Rc;

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
}

impl Parser {
    /// Create a parser from a token stream.
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    /// Parse source text directly into an AST.
    pub fn parse_source(source: &str) -> Result<Vec<AstNode>, ParseError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    /// Parse the token stream into a sequence of AST nodes.
    pub fn parse(&mut self) -> Result<Vec<AstNode>, ParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            if let Some(node) = self.parse_node()? {
                nodes.push(node);
            }
        }
        Ok(nodes)
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
                self.advance(); // consume [
                let items = self.parse_until(TokenKind::RBracket)?;
                Ok(Some(AstNode::List(items)))
            }

            // Quotations: { ... }
            TokenKind::LBrace => {
                self.advance(); // consume {
                let body = self.parse_until(TokenKind::RBrace)?;
                Ok(Some(AstNode::Quote(body)))
            }

            // Word definitions: : name { body } ;
            TokenKind::Colon => {
                self.advance(); // skip :
                let name = self.expect_word()?;
                // Expect { body }
                self.expect(&TokenKind::LBrace)?;
                let body = self.parse_until(TokenKind::RBrace)?;
                self.expect(&TokenKind::Semicolon)?;
                Ok(Some(AstNode::Def { name, body }))
            }

            // Conditional: ??true-expr|false-expr]
            TokenKind::CondQ => {
                self.advance(); // skip ??
                let then_branch = self.parse_until_any(&[TokenKind::Or, TokenKind::RBracket])?;
                let else_branch = if matches!(self.current().kind, TokenKind::Or) {
                    self.advance(); // skip |
                    Some(self.parse_until(TokenKind::RBracket)?)
                } else if matches!(self.current().kind, TokenKind::RBracket) {
                    self.advance(); // skip ]
                    None
                } else {
                    None
                };
                Ok(Some(AstNode::Cond {
                    then_branch,
                    else_branch,
                }))
            }

            // Single-branch conditional: {then} ?->
            TokenKind::CondArrow => {
                self.advance();
                // The then-block should already be on the stack as a quote
                // We just mark it; actual logic is in codegen
                Ok(Some(AstNode::CondArrow {
                    then_branch: Vec::new(),
                }))
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
                // @ alone could be rot
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
                let path = self.expect_string()?;
                Ok(Some(AstNode::Import(path)))
            }
            TokenKind::Export => {
                self.advance();
                let name = self.expect_word()?;
                Ok(Some(AstNode::Export(name)))
            }

            // Word reference
            TokenKind::Word(name) => {
                let n = name.clone();
                self.advance();
                Ok(Some(AstNode::WordRef(n)))
            }

            TokenKind::Semicolon => {
                // Stray semicolon; skip
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
        self.tokens
            .get(self.pos)
            .unwrap_or(&EOF_TOKEN)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
            || matches!(self.current().kind, TokenKind::Eof)
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<(), ParseError> {
        let token = self.current().clone();
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(expected) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected {expected:?}, got {:?}", token.kind),
                token,
            })
        }
    }

    fn expect_word(&mut self) -> Result<String, ParseError> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Word(name) => {
                let n = name.clone();
                self.advance();
                Ok(n)
            }
            _ => Err(ParseError {
                message: format!("Expected word, got {:?}", token.kind),
                token,
            }),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
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

    /// Parse nodes until a matching end token is found.
    fn parse_until(&mut self, end: TokenKind) -> Result<Vec<AstNode>, ParseError> {
        // Skip the opening delimiter if we're currently on it
        if std::mem::discriminant(&self.current().kind) == std::mem::discriminant(&end) {
            self.advance();
            return Ok(Vec::new());
        }

        let mut nodes = Vec::new();
        while !self.is_at_end() {
            if std::mem::discriminant(&self.current().kind) == std::mem::discriminant(&end) {
                self.advance(); // skip end delimiter
                break;
            }
            if let Some(node) = self.parse_node()? {
                nodes.push(node);
            }
        }
        Ok(nodes)
    }

    /// Parse nodes until any of the given end tokens is found.
    fn parse_until_any(
        &mut self,
        ends: &[TokenKind],
    ) -> Result<Vec<AstNode>, ParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            let current_disc = std::mem::discriminant(&self.current().kind);
            if ends
                .iter()
                .any(|e| std::mem::discriminant(e) == current_disc)
            {
                break;
            }
            if let Some(node) = self.parse_node()? {
                nodes.push(node);
            }
        }
        Ok(nodes)
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
