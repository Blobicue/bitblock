//! Facade over the workspace, so `bitblock::protocol::ContentTag` keeps meaning
//! what it always did for the binary, the examples and the integration test.
//!
//! The split behind it is what the module DAG always said: `bitblock-core` is
//! upstream of `bitblock-research`, and now cargo knows, so editing a research
//! leaf no longer recompiles the world model.
pub use bitblock_core::*;

#[cfg(feature = "protocol-research")]
pub use bitblock_research::{
    asset_1, asset_d1, auth_commit_1, bounds_1, commit_r1, commit_state, consensus_gov_d1,
    coordinate_episode, crypto_1, crypto_d1, display_artifact, display_store, economic_state_1,
    emergence_r1, genesis_d1, governance_1, history_store, identity_1, identity_d1, journal,
    ladder_bridge, language_r1, local_bounds, octree_commit, optimization, output_color,
    output_topology, physical_sensor, presentation, presentation_output, presentation_output_v2,
    profile_bootstrap, profile_d1, protocol_id_1, replay_evidence, resource_1, rule_1, scale_assay,
    sequential, storage, threat_1, transition_spec, transition_spec_1, verkle_feasibility, vm_1,
    vm_d1,
};
#[cfg(all(windows, feature = "protocol-research"))]
pub use bitblock_research::{windows_timing, windows_topology};
