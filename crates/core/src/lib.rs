//! The deterministic core: world, camera, protocol tags, and the render and
//! tick path that turns them into frames.
//!
//! Nothing here depends on the research surface. That direction is enforced by
//! the crate boundary rather than by convention, which is the reason the split
//! exists: the module DAG already said `bitblock-research` is downstream, and
//! now the compiler knows it too.

pub mod camera;
pub mod chem;
pub mod display;
pub mod frame;
pub mod frame_v2;
pub mod history;
pub mod interaction;
pub mod program;
pub mod protocol;
pub mod render;
pub mod sidebar;
pub mod tick;
#[cfg(windows)]
pub mod windows_present;
pub mod world;
