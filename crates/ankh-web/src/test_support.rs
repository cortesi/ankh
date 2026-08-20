//! In-process web harness helpers for Ankh router tests.

use axum::{Extension, Router};

use crate::AnkhWebState;

/// In-process Ankh public router harness.
#[derive(Clone)]
pub struct TestAppHarness {
    /// Mounted router.
    router: Router,
    /// Shared web state used by the router.
    state: AnkhWebState,
}

impl TestAppHarness {
    /// Build a harness around a caller-supplied state object.
    #[must_use]
    pub fn new(state: AnkhWebState) -> Self {
        let router = Router::new()
            .merge(crate::router())
            .merge(crate::admin_router())
            .layer(Extension(state.clone()));
        Self { router, state }
    }

    /// Return the mounted router.
    pub fn router(&self) -> Router {
        self.router.clone()
    }

    /// Return the shared web state.
    #[must_use]
    pub fn state(&self) -> &AnkhWebState {
        &self.state
    }
}
