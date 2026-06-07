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
            // Type variable with anything
            (Type::TypeVar(id), other) | (other, Type::TypeVar(id)) => {
                self.unification.insert(*id, other.clone());
                Ok(())
            }
            // Signal(T) unifies with T
            (Type::Signal(inner), other) | (other, Type::Signal(inner)) => self.unify(inner, other),
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
    pub fn find(&self, ty: Type) -> Type {
        match &ty {
            Type::TypeVar(id) => {
                if let Some(resolved) = self.unification.get(id) {
                    self.find(resolved.clone())
                } else {
                    ty
                }
            }
            _ => ty,
        }
    }

    /// Resolve all type variables in a type to their unified concrete types.
    pub fn resolve(&self, ty: &Type) -> Type {
        let resolved = self.find(ty.clone());
        match &resolved {
            Type::List(inner) => Type::List(Box::new(self.resolve(inner))),
            Type::Ref(inputs, outputs) => Type::Ref(
                inputs.iter().map(|t| self.resolve(t)).collect(),
                outputs.iter().map(|t| self.resolve(t)).collect(),
            ),
            Type::Signal(inner) => Type::Signal(Box::new(self.resolve(inner))),
            Type::Union(a, b) => Type::Union(Box::new(self.resolve(a)), Box::new(self.resolve(b))),
            other => other.clone(),
        }
    }
}

impl Default for TypeInferer {
    fn default() -> Self {
        Self::new()
    }
}
