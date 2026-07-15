//! Finance server boundary.
//!
//! The module exposes integer resource ids. All mutations are implemented by
//! the domain helpers in this directory so the Leptos forms and PAT router
//! share one validation and transaction path.

mod domain;

pub use domain::*;
