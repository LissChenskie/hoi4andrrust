//! Deterministic country AI. No graphics, input or wall-clock access.
//!
//! Layering: `observation` reads the simulation, planners decide, and every
//! change goes through `commands`, which calls the same validated game API a
//! human uses and records the outcome. Planners never write simulation fields.
//! All state lives in [`AiState`], which is part of the save game.
pub mod commands;
pub mod observation;
pub mod research;
pub mod scheduler;
pub mod state;

pub use state::AiState;
#[cfg(test)]
mod tests;
