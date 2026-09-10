//! Type inference engine for Whisper.
//! Uses Union-Find for type variable unification and constraint solving.

use crate::types::Type;
use std::collections::HashMap;

/// The type inference engine.
///
/// Uses Union-Find unification to resolve type variables across a program.
/// Maintains a type environment mapping word names to their stack effects.
pub struct TypeInferer {
    /// Mapping from type variable ID to its unified type.
    unification: HashMap<u64, Type>,
    /// Counter for generating fresh type variables.
    next_var: u64,
    /// Stack effect type environment (word → (inputs, outputs)).
    type_env: HashMap<String, (Vec<Type>, Vec<Type>)>,
}

impl TypeInferer {
    pub fn new() -> Self {
        TypeInferer {
            unification: HashMap::new(),
            next_var: 0,
            type_env: HashMap::new(),
        }
    }

    /// Generate a fresh type variable.
    pub fn fresh_var(&mut self) -> Type {
        let id = self.next_var;
        self.next_var += 1;
        Type::TypeVar(id)
    }

    /// Reset the inference state for a new round of inference.
    pub fn reset(&mut self) {
        self.unification.clear();
        self.next_var = 0;
    }

    /// Register a word's type signature in the environment.
    pub fn register_word(&mut self, name: &str, inputs: Vec<Type>, outputs: Vec<Type>) {
        self.type_env.insert(name.to_string(), (inputs, outputs));
    }

    /// Look up a word's type signature.
    pub fn lookup_word(&self, name: &str) -> Option<&(Vec<Type>, Vec<Type>)> {
        self.type_env.get(name)
    }

    /// Unify two types. Returns Ok if they can be made equal, Err otherwise.
    pub fn unify(&mut self, a: &Type, b: &Type) -> Result<(), String> {
        let a = self.find(a.clone());
        let b = self.find(b.clone());

        if a == b {
            return Ok(());
        }

        match (&a, &b) {
            // Type variable with anything (with occurs-check: binding a
            // variable to a type containing itself would make find() and
            // resolve() recurse forever — reject instead).
            (Type::TypeVar(id), other) | (other, Type::TypeVar(id)) => {
                if self.occurs(*id, other) {
                    return Err(format!(
                        "Infinite type: t{id} occurs in {} (recursive word signature?)",
                        other.name()
                    ));
                }
                self.unification.insert(*id, other.clone());
                Ok(())
            }
            // Signal(T) unifies with T
            (Type::Signal(inner), other) | (other, Type::Signal(inner)) => self.unify(inner, other),
            // N-1 数值协变：Bool <: I64 <: F64 两两可合，对齐运行时
            // pop_i64/pop_f64/binary_num_op 的协变（静态检查以消灭假
            // 阳性为先；反向泄漏如 f64 进 i64 位由运行时动态检查兜底）。
            (Type::Bool, Type::I64) | (Type::I64, Type::Bool)
            | (Type::Bool, Type::F64) | (Type::F64, Type::Bool)
            | (Type::I64, Type::F64) | (Type::F64, Type::I64) => Ok(()),
            // List covariance
            (Type::List(a_inner), Type::List(b_inner)) => self.unify(a_inner, b_inner),
            // Union types
            (Type::Union(a1, a2), other) | (other, Type::Union(a1, a2)) => {
                // other must be compatible with at least one branch
                self.unify(a1, other).or_else(|_| self.unify(a2, other))
            }
            // Ref types — unify input/output vectors
            (Type::Ref(a_in, a_out), Type::Ref(b_in, b_out)) => {
                if a_in.len() != b_in.len() || a_out.len() != b_out.len() {
                    return Err(format!("Cannot unify ref types: {a} != {b}"));
                }
                for (ai, bi) in a_in.iter().zip(b_in.iter()) {
                    self.unify(ai, bi)?;
                }
                for (ao, bo) in a_out.iter().zip(b_out.iter()) {
                    self.unify(ao, bo)?;
                }
                Ok(())
            }
            // Incompatible
            _ => Err(format!(
                "Type mismatch: cannot unify {} and {}",
                a.name(),
                b.name()
            )),
        }
    }

    /// Find the canonical representation of a type (Union-Find).
    /// Guards against binding cycles (defensive; occurs-check should
    /// prevent them from forming in the first place).
    pub fn find(&self, ty: Type) -> Type {
        let mut current = ty;
        let mut hops = 0usize;
        loop {
            match &current {
                Type::TypeVar(id) => match self.unification.get(id) {
                    Some(resolved) => {
                        hops += 1;
                        if hops > 10_000 {
                            return current; // cycle guard — give up resolving
                        }
                        current = resolved.clone();
                    }
                    None => return current,
                },
                _ => return current,
            }
        }
    }

    /// Resolve all type variables in a type to their unified concrete types.
    pub fn resolve(&self, ty: &Type) -> Type {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &Type, depth: usize) -> Type {
        if depth > 100 {
            return Type::TypeVar(u64::MAX); // pathological nesting — bail out
        }
        let resolved = self.find(ty.clone());
        match &resolved {
            Type::List(inner) => Type::List(Box::new(self.resolve_depth(inner, depth + 1))),
            Type::Ref(inputs, outputs) => Type::Ref(
                inputs
                    .iter()
                    .map(|t| self.resolve_depth(t, depth + 1))
                    .collect(),
                outputs
                    .iter()
                    .map(|t| self.resolve_depth(t, depth + 1))
                    .collect(),
            ),
            Type::Signal(inner) => Type::Signal(Box::new(self.resolve_depth(inner, depth + 1))),
            Type::Union(a, b) => Type::Union(
                Box::new(self.resolve_depth(a, depth + 1)),
                Box::new(self.resolve_depth(b, depth + 1)),
            ),
            other => other.clone(),
        }
    }
}

impl Default for TypeInferer {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeInferer {
    /// Does type variable `id` occur inside `ty`? Follows variable
    /// bindings so mutually-recursive variables are detected too.
    fn occurs(&self, id: u64, ty: &Type) -> bool {
        match ty {
            Type::TypeVar(other) => {
                if *other == id {
                    return true;
                }
                match self.unification.get(other) {
                    Some(resolved) => self.occurs(id, resolved),
                    None => false,
                }
            }
            Type::List(inner) | Type::Signal(inner) => self.occurs(id, inner),
            Type::Ref(inputs, outputs) => {
                inputs.iter().any(|t| self.occurs(id, t))
                    || outputs.iter().any(|t| self.occurs(id, t))
            }
            Type::Union(a, b) => self.occurs(id, a) || self.occurs(id, b),
            _ => false,
        }
    }
}
