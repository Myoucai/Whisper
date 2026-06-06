// Whisper Type Checker - Static type inference and stack effect validation

pub mod builtins;
pub mod checker;
pub mod infer;
pub mod stack_effect;
pub mod types;

pub use checker::TypeChecker;
pub use infer::TypeInferer;
pub use types::Type;

/// Convert from the simplified SType to the full Type system.
pub fn stype_to_type(st: &checker::SType, inferer: &mut TypeInferer) -> Type {
    match st {
        checker::SType::Any => inferer.fresh_var(),
        checker::SType::Num => {
            // Num = i64 | f64, use a fresh variable that will unify
            inferer.fresh_var()
        }
        checker::SType::Int => Type::I64,
        checker::SType::Float => Type::F64,
        checker::SType::Bool => Type::Bool,
        checker::SType::Str => Type::Str,
        checker::SType::List => Type::List(Box::new(inferer.fresh_var())),
        checker::SType::Ref => {
            let t = inferer.fresh_var();
            Type::Ref(vec![t.clone()], vec![t])
        }
    }
}
