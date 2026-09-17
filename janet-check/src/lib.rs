//! Janet analysis and type inference.
//!
//! - [`syntax`]: tree-sitter parsing and position mapping.
//! - [`analysis`]: read-only queries (definitions, references, types, the core environment).
//! - [`janet`]: scripts run with the user's `janet`.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod analysis;
pub mod janet;
pub mod syntax;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
