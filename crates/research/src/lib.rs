//! Opt-in research carriers: typed storage, journals, presentation and display
//! artifacts, sensor observations, replay evidence, and the optimisation gate.
//!
//! None of this is in a default build, and none of it is proof of monitor
//! light, scanout, ownership, or consensus. See docs/PROTOCOL.md.
//!
//! The core modules are re-exported so that `crate::protocol::ContentTag` keeps
//! resolving inside this crate exactly as it did when everything was one
//! module tree. That is deliberate: the split is meant to give the compiler the
//! dependency graph, not to rewrite 84 import paths and risk a transcription
//! error in a byte-exact codec.
#[cfg(windows)]
pub use bitblock_core::windows_present;
pub use bitblock_core::{
    camera, display, frame, frame_v2, history, interaction, program, protocol, render, sidebar,
    tick, world,
};

pub mod commit_r1;
pub mod coordinate_episode;
pub mod display_artifact;
pub mod display_store;
pub mod history_store;
pub mod journal;
pub mod ladder_bridge;
pub mod local_bounds;
pub mod octree_commit;
pub mod optimization;
pub mod output_color;
pub mod output_topology;
pub mod physical_sensor;
pub mod presentation;
pub mod presentation_output;
pub mod presentation_output_v2;
pub mod replay_evidence;
pub mod scale_assay;
pub mod sequential;
pub mod storage;
#[cfg(windows)]
pub mod windows_timing;
#[cfg(windows)]
pub mod windows_topology;
