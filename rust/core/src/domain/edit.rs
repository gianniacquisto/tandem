//! One field's change in a multi-field operation: leave it alone, or set it
//! (`Edit.swift`).
//!
//! A plain `Option<T>` cannot carry both meanings: `None` would mean "clear this"
//! and "do not touch this" at once. `.set(None)` on an optional field clears it;
//! [`Edit::Unchanged`] is the absent instruction.

/// One field's change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edit<T> {
    /// The caller did not name this field.
    #[default]
    Unchanged,
    /// The caller named it, with this value.
    Set(T),
}

impl<T> Edit<T> {
    /// Whether the caller named this field at all.
    pub fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    /// The value to set, or `None` when the field is unchanged. For an optional
    /// field this is `Option<Option<T>>`: `Some(None)` is "clear it", `None` is
    /// "leave it".
    pub fn value(self) -> Option<T> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unchanged => None,
        }
    }
}
