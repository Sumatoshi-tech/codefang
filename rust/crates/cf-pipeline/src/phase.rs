//! `Phase` / `run_phases` chain-of-responsibility (`phase.go`).

use crate::context::Ctx;

/// A single processing stage that transforms state `S`.
///
/// Mirrors Go's `Phase[S]` interface.
pub trait Phase<S> {
    /// The error type produced by a failed phase.
    type Error;

    /// Runs the phase, consuming and returning the (possibly transformed)
    /// state.
    fn run(&self, ctx: &Ctx, s: S) -> Result<S, Self::Error>;
}

/// Adapts a plain closure to the [`Phase`] trait, mirroring Go's
/// `PhaseFunc[S]`.
pub struct PhaseFunc<F>(pub F);

impl<F> PhaseFunc<F> {
    /// Wraps `f` as a [`Phase`].
    pub fn new(f: F) -> Self {
        PhaseFunc(f)
    }
}

impl<S, E, F> Phase<S> for PhaseFunc<F>
where
    F: Fn(&Ctx, S) -> Result<S, E>,
{
    type Error = E;

    fn run(&self, ctx: &Ctx, s: S) -> Result<S, Self::Error> {
        (self.0)(ctx, s)
    }
}

/// Executes phases sequentially, threading state through each one.
///
/// Returns immediately on the first error, preserving the partial state (the
/// state value as returned by the failing phase, matching Go's `return s,
/// err`). Returns the input state unchanged when no phases are provided.
///
/// All phases share a single error type `E` so they can be passed as a uniform
/// slice of trait objects, mirroring Go's variadic `phases ...Phase[S]`.
pub fn run_phases<S, E>(ctx: &Ctx, mut s: S, phases: &[&dyn Phase<S, Error = E>]) -> Result<S, E> {
    for p in phases {
        s = p.run(ctx, s)?;
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_func_runs() {
        let p = PhaseFunc::new(|_ctx: &Ctx, s: i32| Ok::<i32, ()>(s + 1));
        let ctx = Ctx::background();
        assert_eq!(p.run(&ctx, 1), Ok(2));
    }

    #[test]
    fn run_phases_threads_state() {
        let add = PhaseFunc::new(|_ctx: &Ctx, s: i32| Ok::<i32, ()>(s + 10));
        let mul = PhaseFunc::new(|_ctx: &Ctx, s: i32| Ok::<i32, ()>(s * 2));
        let ctx = Ctx::background();
        let phases: [&dyn Phase<i32, Error = ()>; 2] = [&add, &mul];
        // (1 + 10) * 2 = 22
        assert_eq!(run_phases(&ctx, 1, &phases), Ok(22));
    }

    #[test]
    fn run_phases_no_phases_returns_input() {
        let ctx = Ctx::background();
        let phases: [&dyn Phase<i32, Error = ()>; 0] = [];
        assert_eq!(run_phases(&ctx, 99, &phases), Ok(99));
    }

    #[test]
    fn run_phases_stops_on_first_error_with_partial_state() {
        let ok = PhaseFunc::new(|_ctx: &Ctx, s: i32| Ok::<i32, &str>(s + 1));
        // This phase fails but reports the partial state it would have produced.
        let boom = PhaseFunc::new(|_ctx: &Ctx, _s: i32| Err::<i32, &str>("boom"));
        let never = PhaseFunc::new(|_ctx: &Ctx, _s: i32| {
            panic!("must not run after error");
        });
        let ctx = Ctx::background();
        let phases: [&dyn Phase<i32, Error = &str>; 3] = [&ok, &boom, &never];
        assert_eq!(run_phases(&ctx, 0, &phases), Err("boom"));
    }
}
