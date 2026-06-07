//! Whisper source code formatter.
//! Pretty-prints Whisper AST back to source text with consistent
//! whitespace, indentation, and alignment.

use whisper_parser::ast::{AstNode, Operator};

/// Format a parsed AST back into source text.
pub fn format_ast(nodes: &[AstNode]) -> String {
    let mut out = String::new();
    let mut first = true;

    for node in nodes {
        if !first {
            out.push('\n');
        }
        first = false;
        format_node(node, &mut out);
    }
    out
}

fn format_node(node: &AstNode, out: &mut String) {
    match node {
        AstNode::Literal(val) => {
            out.push_str(&format!("{val}"));
        }
        AstNode::WordRef(name) => {
            out.push_str(name);
        }
        AstNode::Op(op) => {
            out.push_str(&fmt_op(*op));
        }
        AstNode::Quote(body) => {
            out.push_str("{ ");
            for (i, n) in body.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                format_node(n, out);
            }
            out.push_str(" }");
        }
        AstNode::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                format_node(item, out);
            }
            out.push(']');
        }
        AstNode::Def { name, body } => {
            out.push_str(&format!(": {name} "));
            out.push_str("{ ");
            for n in body {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} ;");
        }
        AstNode::Import(path) => {
            out.push_str(&format!("import \"{path}\""));
        }
        AstNode::Export(name) => {
            out.push_str(&format!("export {name}"));
        }
        AstNode::Cond {
            then_branch,
            else_branch,
        } => {
            out.push_str("??");
            for n in then_branch {
                out.push(' ');
                format_node(n, out);
            }
            if let Some(eb) = else_branch {
                out.push_str(" |");
                for n in eb {
                    out.push(' ');
                    format_node(n, out);
                }
            }
            out.push(']');
        }
        AstNode::CondArrow { then_branch } => {
            out.push_str("{ ");
            for n in then_branch {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} ?->");
        }
        AstNode::Loop { body, condition } => {
            out.push_str("{ ");
            for n in body {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} { ");
            for n in condition {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} #");
        }
        AstNode::Times { body } => {
            out.push_str("{ ");
            for n in body {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} @times");
        }
        AstNode::ConfidenceLabel { body, confidence } => {
            out.push_str("{ ");
            for n in body {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str(&format!("}} :{}", confidence.0));
        }
        AstNode::ProbChoice { alt1, alt2 } => {
            out.push_str("{ ");
            for n in alt1 {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} { ");
            for n in alt2 {
                format_node(n, out);
                out.push(' ');
            }
            out.push_str("} ?|");
        }
    }
}

fn fmt_op(op: Operator) -> String {
    match op {
        Operator::Dup => "_".into(),
        Operator::Swap => "`".into(),
        Operator::Drop => "drop".into(),
        Operator::Rot => "@".into(),
        Operator::Pick(n) => format!("${n}"),
        Operator::Add => "+".into(),
        Operator::Sub => "-".into(),
        Operator::Mul => "*".into(),
        Operator::Div => "/".into(),
        Operator::Mod => "mod".into(),
        Operator::Eq => "=".into(),
        Operator::Lt => "<".into(),
        Operator::Gt => ">".into(),
        Operator::Neq => "!=".into(),
        Operator::Le => "<=".into(),
        Operator::Ge => ">=".into(),
        Operator::And => "&".into(),
        Operator::Or => "|".into(),
        Operator::Not => "!".into(),
        Operator::Nth => "@nth".into(),
        Operator::Append => "append".into(),
        Operator::Len => "len".into(),
        Operator::Map => "@map".into(),
        Operator::Each => "@each".into(),
        Operator::Fold => "@fold".into(),
        Operator::AtTimes => "@times".into(),
        Operator::CondQ => "??".into(),
        Operator::CondArrow => "?->".into(),
        Operator::Hash => "#".into(),
        Operator::CapCall(n) => format!("@{n}"),
        Operator::CapExec => "!".into(),
        Operator::ConfLabel(c) => format!(":{c}"),
        Operator::ProbChoice => "?|".into(),
        Operator::OutputTop => ".".into(),
        Operator::OutputAll => "..".into(),
        Operator::ReadInput => ",".into(),
        Operator::StrLen => "strlen".into(),
        Operator::StrCat => "strcat".into(),
        Operator::StrSlice => "strslice".into(),
        Operator::StrEq => "streq".into(),
        Operator::StrLt => "strlt".into(),
        Operator::StrFind => "strfind".into(),
        Operator::StrReplace => "strreplace".into(),
        Operator::StrToI64 => "strtoi64".into(),
        Operator::I64ToStr => "i64tostr".into(),
        Operator::StrNth => "strnth".into(),
        Operator::StrChars => "strchars".into(),
        Operator::CharsStr => "charsstr".into(),
        Operator::StrIter => "striter".into(),
        Operator::ListFind => "listfind".into(),
        Operator::StrJoin => "strjoin".into(),
        Operator::BytesNew => "bytes-new".into(),
        Operator::BytesPush => "bytes-push".into(),
        Operator::BytesLen => "bytes-len".into(),
        Operator::BytesWriteFile => "bytes-write".into(),
        Operator::Try => "try".into(),
        Operator::I64ToF64 => "i64tof64".into(),
        Operator::F64ToI64 => "f64toi64".into(),
        Operator::FSqrt => "fsqrt".into(),
        Operator::FSin => "fsin".into(),
        Operator::FCos => "fcos".into(),
        Operator::FTan => "ftan".into(),
        Operator::JsonParse => "json-parse".into(),
        Operator::JsonStringify => "json-stringify".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisper_parser::Parser;

    #[test]
    fn test_fmt_simple() {
        let src = "3 4 + .";
        let ast = Parser::parse_source(src).unwrap();
        let formatted = format_ast(&ast);
        assert_eq!(formatted, "3\n4\n+\n.");
    }

    #[test]
    fn test_fmt_def() {
        let src = ": sq { _ * } ;";
        let ast = Parser::parse_source(src).unwrap();
        let formatted = format_ast(&ast);
        assert!(formatted.contains("sq"), "got: {formatted}");
    }

    #[test]
    fn test_fmt_empty() {
        let formatted = format_ast(&[]);
        assert!(formatted.is_empty());
    }
}
