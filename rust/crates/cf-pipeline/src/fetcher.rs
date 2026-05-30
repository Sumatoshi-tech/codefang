//! The `Fetcher` cache-decorator base interface (`fetcher.go`).

use crate::context::Ctx;

/// Retrieves a response for a given request.
///
/// Serves as the base interface for the cache decorator pattern: wrap a
/// [`Fetcher`] with "check cache → fetch misses → update cache" logic. Mirrors
/// Go's `Fetcher[Req, Resp]` interface.
pub trait Fetcher<Req, Resp> {
    /// The error type produced by a failed fetch.
    type Error;

    /// Fetches the response for `req`.
    fn fetch(&self, ctx: &Ctx, req: Req) -> Result<Resp, Self::Error>;
}

/// Adapts a plain closure to the [`Fetcher`] trait, mirroring Go's
/// `FetcherFunc[Req, Resp]`.
pub struct FetcherFunc<F>(pub F);

impl<F> FetcherFunc<F> {
    /// Wraps `f` as a [`Fetcher`].
    pub fn new(f: F) -> Self {
        FetcherFunc(f)
    }
}

impl<Req, Resp, E, F> Fetcher<Req, Resp> for FetcherFunc<F>
where
    F: Fn(&Ctx, Req) -> Result<Resp, E>,
{
    type Error = E;

    fn fetch(&self, ctx: &Ctx, req: Req) -> Result<Resp, Self::Error> {
        (self.0)(ctx, req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetcher_func_calls_underlying() {
        let f: FetcherFunc<_> = FetcherFunc::new(|_ctx: &Ctx, req: i32| Ok::<i32, ()>(req * 2));
        let ctx = Ctx::background();
        assert_eq!(f.fetch(&ctx, 21), Ok(42));
    }

    #[test]
    fn fetcher_func_propagates_error() {
        let f = FetcherFunc::new(|_ctx: &Ctx, _req: i32| Err::<i32, &str>("miss"));
        let ctx = Ctx::background();
        assert_eq!(f.fetch(&ctx, 1), Err("miss"));
    }

    // A trivial cache-decorator demonstrating the intended composition.
    struct CacheDecorator<I> {
        inner: I,
        cache: std::cell::RefCell<Option<i32>>,
    }

    impl<I> Fetcher<i32, i32> for CacheDecorator<I>
    where
        I: Fetcher<i32, i32, Error = ()>,
    {
        type Error = ();

        fn fetch(&self, ctx: &Ctx, req: i32) -> Result<i32, ()> {
            if let Some(v) = *self.cache.borrow() {
                return Ok(v);
            }
            let v = self.inner.fetch(ctx, req)?;
            *self.cache.borrow_mut() = Some(v);
            Ok(v)
        }
    }

    #[test]
    fn decorator_pattern_composes() {
        let calls = std::cell::Cell::new(0);
        let inner = FetcherFunc::new(|_ctx: &Ctx, req: i32| {
            calls.set(calls.get() + 1);
            Ok::<i32, ()>(req)
        });
        let decorator = CacheDecorator {
            inner,
            cache: std::cell::RefCell::new(None),
        };
        let ctx = Ctx::background();
        assert_eq!(decorator.fetch(&ctx, 7), Ok(7));
        assert_eq!(decorator.fetch(&ctx, 7), Ok(7));
        assert_eq!(calls.get(), 1, "second fetch must hit cache");
    }
}
