//! Type signatures for built-in operators.
//!
//! Each entry maps an operator name to its stack effect signature:
//! (inputs, outputs) where each is a list of Type.
//! Type::TypeVar(0) denotes a polymorphic type variable.

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
        "rot" => Some((
            vec![tv(), tv2(), Type::TypeVar(2)],
            vec![tv2(), Type::TypeVar(2), tv()],
        )),

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
        "@nth" => Some((vec![Type::List(Box::new(tv())), i64.clone()], vec![tv()])),
        "append" => Some((
            vec![Type::List(Box::new(tv())), tv()],
            vec![Type::List(Box::new(tv()))],
        )),
        "len" => Some((vec![Type::List(Box::new(tv()))], vec![i64.clone()])),

        // String operations
        "strlen" => Some((vec![Type::Str], vec![Type::I64])),
        "strcat" => Some((vec![Type::Str, Type::Str], vec![Type::Str])),
        "strslice" => Some((vec![Type::Str, Type::I64, Type::I64], vec![Type::Str])),
        "streq" => Some((vec![Type::Str, Type::Str], vec![Type::Bool])),
        "strlt" => Some((vec![Type::Str, Type::Str], vec![Type::Bool])),
        "strfind" => Some((vec![Type::Str, Type::Str], vec![Type::I64])),
        "strreplace" => Some((vec![Type::Str, Type::Str, Type::Str], vec![Type::Str])),
        "strtoi64" => Some((vec![Type::Str], vec![Type::I64])),
        "i64tostr" => Some((vec![Type::I64], vec![Type::Str])),
        "strnth" => Some((vec![Type::Str, Type::I64], vec![Type::I64])),
        "strchars" => Some((vec![Type::Str], vec![Type::List(Box::new(Type::I64))])),
        "charsstr" => Some((vec![Type::List(Box::new(Type::I64))], vec![Type::Str])),
        "striter" => Some((vec![Type::Str], vec![Type::I64, Type::Str])),
        "listfind" => Some((
            vec![
                Type::List(Box::new(Type::List(Box::new(Type::TypeVar(0))))),
                Type::TypeVar(0),
            ],
            vec![Type::Bool, Type::TypeVar(0)],
        )),
        "strjoin" => Some((vec![Type::List(Box::new(Type::Str))], vec![Type::Str])),
        "bytes-new" => Some((vec![], vec![Type::I64])),
        "bytes-push" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        "bytes-len" => Some((vec![Type::I64], vec![Type::I64])),
        "bytes-write" => Some((vec![Type::I64, Type::Str], vec![])),

        // Float operations
        "i64tof64" => Some((vec![Type::I64], vec![Type::F64])),
        "f64toi64" => Some((vec![Type::F64], vec![Type::I64])),
        "fsqrt" => Some((vec![Type::F64], vec![Type::F64])),
        "fsin" => Some((vec![Type::F64], vec![Type::F64])),
        "fcos" => Some((vec![Type::F64], vec![Type::F64])),
        "ftan" => Some((vec![Type::F64], vec![Type::F64])),

        // JSON
        "json-parse" => Some((vec![Type::Str], vec![Type::TypeVar(0)])),
        "json-stringify" => Some((vec![Type::TypeVar(0)], vec![Type::Str])),

        _ => None,
    }
}
