//! Diagnostics that drive the real Space Scuffle client but ship outside it.
//!
//! # Responsibility
//! This crate owns the real-GPU capture harness, the transparent-phase audit, the native input
//! probe, the capture input fixtures, the frame-budget measurement, the showcase video and the
//! effect lab's contact sheets. Each one builds the production application through the
//! public `client` API and attaches itself through ordinary plugins, schedules and state.
//!
//! # Place in the workspace
//! The dependency runs one way: `client_diagnostics` depends on `client`. The client must never
//! depend on this crate, in any section of its manifest, so no diagnostic can reach production.
//!
//! # Invariants
//! - No module replaces the production renderer, scene, asset loader or simulation.
//! - Production code holds no audit hook. A capture changes the game only through the controls and
//!   schedule sets that `client` exposes, such as `BattleSet::ExternalControl`.

#![forbid(unsafe_code)]

pub mod effect_lab;
pub mod frame_budget;
pub mod native_input_probe;
pub mod palette_fixture;
pub mod runtime_capture;
pub mod shell_order_audit;
pub mod showcase;
