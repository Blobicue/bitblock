//! Facade over the workspace, so `bitblock::protocol::ContentTag` keeps meaning
//! what it always did for the binary, the examples and the integration test.
//!
//! The split behind it is what the module DAG always said: `bitblock-core` is
//! upstream of `bitblock-research`, and now cargo knows, so editing a research
//! leaf no longer recompiles the world model.
pub use bitblock_core::*;

#[cfg(feature = "protocol-research")]
pub use bitblock_research::{
    coordinate_episode, display_artifact, display_store, history_store, journal, ladder_bridge,
    optimization, output_color, output_topology, physical_sensor, presentation,
    presentation_output, presentation_output_v2, replay_evidence, sequential, storage,
};
#[cfg(all(windows, feature = "protocol-research"))]
pub use bitblock_research::{windows_timing, windows_topology};
