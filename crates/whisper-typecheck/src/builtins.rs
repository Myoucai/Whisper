/// Type signatures for built-in operators.
///
/// Each entry maps an operator name to its stack effect signature:
/// (inputs, outputs) where each is a list of Type.
/// Type::TypeVar(0) denotes a polymorphic type variable.

use crate::types::Type;

/// Get the stack effect signature for a builtin operator.
pub fn get_builtin_signature(name: &str) -> Option<(Vec<Type>, Vec<Type>)> {
    let i64 = Type::I64;
    let _f64 = Type::F64;
    let bool = Type::Bool;
    let _str = Type::Str;
    let tv = || Type::TypeVar(0); // generic type variable
    let tv2 = || Type::TypeVar(1); // second generic type variable

    match name {
        // Stack operations
        "dup" => Some((vec![tv()], vec![tv(), tv()])),
        "swap" => Some((vec![tv(), tv2()], vec![tv2(), tv()])),
        "drop" => Some((vec![tv()], vec![])),
        "rot" => Some((vec![tv(), tv2(), Type::TypeVar(2)], vec![tv2(), Type::TypeVar(2), tv()])),

        // Arithmetic — both i64 and f64 versions
        "+" => Some((vec![i64.clone(), i64.clone()], vec![i64.clone()])),
        "-" => Some((vec![i64.clone(), i64.clone()], vec![i64.clone()])),
        "*" => Some((vec![i64.clone(), i64.clone()], vec![i64.clone()])),
        "/" => Some((vec![i64.clone(), i64.clone()], vec![i64.clone()])),

        // Comparison
        "=" => Some((vec![tv(), tv2()], vec![bool.clone()])),
        "<" => Some((vec![i64.clone(), i64.clone()], vec![bool.clone()])),
        ">" => Some((vec![i64.clone(), i64.clone()], vec![bool.clone()])),

        // Logic
        "&" => Some((vec![bool.clone(), bool.clone()], vec![bool.clone()])),
        "|" => Some((vec![bool.clone(), bool.clone()], vec![bool.clone()])),
        "!" => Some((vec![bool.clone()], vec![bool.clone()])),

        // List operations
        "@nth" => Some((
            vec![Type::List(Box::new(tv())), i64.clone()],
            vec![tv()],
        )),
        "append" => Some((
            vec![Type::List(Box::new(tv())), tv()],
            vec![Type::List(Box::new(tv()))],
        )),
        "len" => Some((vec![Type::List(Box::new(tv()))], vec![i64.clone()])),

        _ => None,
    }
}
