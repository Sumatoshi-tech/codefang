//! Dispatch-strategy alias.

use crate::context::Ctx;

/// Sends a request to a worker pool.
///
/// The worker channel is captured in the closure, keeping the dispatch
/// strategy decoupled from request semantics. The `Box<dyn Fn ...>` form lets
/// callers store heterogeneous dispatch closures behind a single type.
pub type DispatchFunc<Req> =
    Box<dyn Fn(&Ctx, Req) -> Result<(), Box<dyn std::error::Error + Send + Sync>> + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    #[test]
    fn dispatch_func_invokes_closure() {
        let seen = Arc::new(AtomicI32::new(0));
        let seen_clone = seen.clone();
        let dispatch: DispatchFunc<i32> = Box::new(move |_ctx, req| {
            seen_clone.store(req, Ordering::SeqCst);
            Ok(())
        });

        let ctx = Ctx::background();
        let res = dispatch(&ctx, 42);
        assert!(res.is_ok());
        assert_eq!(seen.load(Ordering::SeqCst), 42);
    }

    #[test]
    fn dispatch_func_propagates_error() {
        let dispatch: DispatchFunc<i32> = Box::new(|_ctx, _req| Err("boom".into()));
        let ctx = Ctx::background();
        let err = dispatch(&ctx, 1).unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }
}
