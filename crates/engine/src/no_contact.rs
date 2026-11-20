//! The contact adapter of a session that never had one installed.
//!
//! It prepares nothing, so every query falls back to the world box the document has always measured
//! (ADR 0179). The shell replaces it once with the real adapter, and a test that wants the box reading
//! simply leaves it in place.
use std::sync::Arc;

use geom::{Contact, Pose, Shape, Shared};

/// The contact adapter of a session that never had one installed.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoContact;

impl Contact for NoContact {
    fn handle(&self) -> Arc<dyn Contact> {
        Arc::new(NoContact)
    }

    fn prepare(&self, _shape: &Shape, _pose: Pose) -> Option<Shared> {
        None
    }
}
