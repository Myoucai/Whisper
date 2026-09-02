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
        "prepend" => Some((
            vec![tv(), Type::List(Box::new(tv()))],
            vec![Type::List(Box::new(tv()))],
        )),
        "len" => Some((vec![Type::List(Box::new(tv()))], vec![i64.clone()])),
        "@filter" => Some((
            vec![
                Type::List(Box::new(tv())),
                Type::Ref(vec![tv()], vec![Type::Bool]),
            ],
            vec![Type::List(Box::new(tv()))],
        )),
        "strsplit" => Some((
            vec![Type::Str, Type::Str],
            vec![Type::List(Box::new(Type::Str))],
        )),
        "ctos" => Some((vec![Type::I64], vec![Type::Str])),
        "listslice" => Some((
            vec![Type::List(Box::new(tv())), Type::I64, Type::I64],
            vec![Type::List(Box::new(tv()))],
        )),

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
        "try" => Some((
            vec![Type::Ref(vec![], vec![])],
            vec![Type::Bool, Type::TypeVar(0)],
        )),
        "list-slice" => Some((
            vec![Type::List(Box::new(tv())), Type::I64, Type::I64],
            vec![Type::List(Box::new(tv()))],
        )),
        "<<" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        ">>" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        "args" => Some((vec![], vec![Type::List(Box::new(Type::Str))])),

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

        // Time
        "now" => Some((vec![], vec![Type::I64])),
        "sleep" => Some((vec![Type::I64], vec![])),

        // Random
        "random" => Some((vec![], vec![Type::F64])),
        "randint" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),

        // Float math
        "floor" => Some((vec![Type::F64], vec![Type::F64])),
        "ceil" => Some((vec![Type::F64], vec![Type::F64])),
        "round" => Some((vec![Type::F64], vec![Type::F64])),
        "log" => Some((vec![Type::F64], vec![Type::F64])),
        "exp" => Some((vec![Type::F64], vec![Type::F64])),
        "atan2" => Some((vec![Type::F64, Type::F64], vec![Type::F64])),

        // Bitwise
        "bitand" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        "bitor" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        "bitxor" => Some((vec![Type::I64, Type::I64], vec![Type::I64])),
        "bitnot" => Some((vec![Type::I64], vec![Type::I64])),

        // Sort
        "sort" => Some((
            vec![Type::List(Box::new(tv()))],
            vec![Type::List(Box::new(tv()))],
        )),
        "rev" => Some((
            vec![Type::List(Box::new(tv()))],
            vec![Type::List(Box::new(tv()))],
        )),
        "dedup" => Some((
            vec![Type::List(Box::new(tv()))],
            vec![Type::List(Box::new(tv()))],
        )),
        "take" => Some((
            vec![Type::List(Box::new(tv())), Type::I64],
            vec![Type::List(Box::new(tv()))],
        )),
        "drop-n" => Some((
            vec![Type::List(Box::new(tv())), Type::I64],
            vec![Type::List(Box::new(tv()))],
        )),

        // Map operations
        "map-new" => Some((vec![], vec![Type::TypeVar(99)])),
        "map-get" => Some((vec![Type::TypeVar(99), Type::Str], vec![Type::TypeVar(0)])),
        "map-set" => Some((
            vec![Type::TypeVar(99), Type::Str, Type::TypeVar(0)],
            vec![Type::TypeVar(99)],
        )),
        "map-del" => Some((vec![Type::TypeVar(99), Type::Str], vec![Type::TypeVar(99)])),
        "map-keys" => Some((
            vec![Type::TypeVar(99)],
            vec![Type::List(Box::new(Type::Str))],
        )),
        "map-has" => Some((vec![Type::TypeVar(99), Type::Str], vec![Type::Bool])),

        // KV（词名形态——lexer 也直接给 KvSet 等 opcode，双保险）
        "kv-set" => Some((vec![Type::Str, Type::Str], vec![])),
        "op-effect" => Some((vec![Type::Str], vec![Type::I64, Type::I64])),
        "size-of" => Some((vec![Type::Ref(vec![], vec![])], vec![Type::I64])),
        "word-size" => Some((vec![Type::Str], vec![Type::I64])),
        "seed!" => Some((vec![Type::I64], vec![])),
        "rand" => Some((vec![], vec![Type::F64])),
        "conf-of" => Some((vec![Type::TypeVar(0)], vec![Type::TypeVar(0), Type::F64])),
        "unwrap" => Some((vec![Type::TypeVar(0)], vec![Type::TypeVar(0)])),
        "kv-get" => Some((vec![Type::Str], vec![Type::Str])),
        "kv-del" => Some((vec![Type::Str], vec![])),
        "kv-keys" => Some((vec![], vec![Type::List(Box::new(Type::Str))])),

        // 高阶族（@each/@map/@fold/@filter/@times：list/quote 操作数全部消费）
        "@each" => Some((
            vec![Type::List(Box::new(tv())), Type::Ref(vec![tv()], vec![])],
            vec![],
        )),
        "@map" => Some((
            vec![
                Type::List(Box::new(tv())),
                Type::Ref(vec![tv()], vec![tv2()]),
            ],
            vec![Type::List(Box::new(tv2()))],
        )),
        "@fold" => Some((
            vec![
                Type::List(Box::new(tv())),
                tv2(),
                Type::Ref(vec![tv2(), tv()], vec![tv2()]),
            ],
            vec![tv2()],
        )),
        "@filter" => Some((
            vec![
                Type::List(Box::new(tv())),
                Type::Ref(vec![tv()], vec![Type::Bool]),
            ],
            vec![Type::List(Box::new(tv()))],
        )),
        "@times" => Some((
            vec![Type::I64, Type::Ref(vec![], vec![])],
            vec![],
        )),

        // 文件/正则杂项
        "read-file" => Some((vec![Type::Str], vec![Type::Str])),
        "write-file" => Some((vec![Type::Str, Type::Str], vec![Type::I64])),
        "strfind-re" => Some((vec![Type::Str, Type::Str], vec![Type::I64])),
        "strmatch" => Some((vec![Type::Str, Type::Str], vec![Type::Bool])),
        "range" => Some((
            vec![Type::I64, Type::I64],
            vec![Type::List(Box::new(Type::I64))],
        )),
        "rot" => Some((
            vec![tv(), tv2(), Type::TypeVar(2)],
            vec![tv2(), Type::TypeVar(2), tv()],
        )),
        "i64?" => Some((vec![tv()], vec![Type::Bool])),
        "str?" => Some((vec![tv()], vec![Type::Bool])),
        "bool?" => Some((vec![tv()], vec![Type::Bool])),
        "list?" => Some((vec![tv()], vec![Type::Bool])),

        _ => None,
    }
}
