//! Testbed for timeline forking. Notation is the ground truth; puzzle states
//! are always derived from it. Dependencies point one way:
//! `puzzle` <- `notation` <- `doc` <- `session` <- `ui`.

pub mod doc;
pub mod notation;
pub mod puzzle;
pub mod session;
pub mod ui;
#[cfg(test)]
mod tests;
