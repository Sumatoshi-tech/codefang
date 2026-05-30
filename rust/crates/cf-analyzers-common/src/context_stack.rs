//! Generic LIFO context stack for UAST traversal (`context_stack.go`).
//!
//! Replaces the repeated push/pop/peek pattern in visitor implementations with
//! a single small generic stack.

/// A generic LIFO stack for tracking nested analysis contexts.
///
/// Mirrors `common.ContextStack[T]`.
#[derive(Debug, Clone, Default)]
pub struct ContextStack<T> {
    items: Vec<T>,
}

impl<T> ContextStack<T> {
    /// Creates a new empty stack. Mirrors `common.NewContextStack`.
    #[must_use]
    pub fn new() -> Self {
        ContextStack { items: Vec::new() }
    }

    /// Pushes an element onto the top of the stack.
    pub fn push(&mut self, ctx: T) {
        self.items.push(ctx);
    }

    /// Removes and returns the top element, or `None` if the stack is empty.
    ///
    /// The `(value, ok)` Go signature maps onto `Option<T>`.
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    /// Returns a reference to the top element without removing it, or `None` if
    /// the stack is empty. Mirrors `common.ContextStack.Current`.
    #[must_use]
    pub fn current(&self) -> Option<&T> {
        self.items.last()
    }

    /// Returns the number of elements on the stack.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` when the stack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop() {
        let mut s: ContextStack<String> = ContextStack::new();
        assert_eq!(s.depth(), 0);

        s.push("first".to_string());
        s.push("second".to_string());
        assert_eq!(s.depth(), 2);

        assert_eq!(s.pop().as_deref(), Some("second"));
        assert_eq!(s.current().map(String::as_str), Some("first"));
    }

    #[test]
    fn empty_pop() {
        let mut s: ContextStack<i32> = ContextStack::new();
        assert_eq!(s.pop(), None);
    }

    #[test]
    fn empty_current() {
        let s: ContextStack<i32> = ContextStack::new();
        assert_eq!(s.current(), None);
    }

    #[test]
    fn struct_elements() {
        #[derive(Debug, PartialEq)]
        struct Ctx {
            name: &'static str,
            depth: i32,
        }
        let mut s: ContextStack<Ctx> = ContextStack::new();
        s.push(Ctx { name: "a", depth: 1 });
        s.push(Ctx { name: "b", depth: 2 });

        assert_eq!(s.current().map(|c| c.name), Some("b"));
        let _ = s.pop();
        assert_eq!(s.current().map(|c| c.name), Some("a"));
    }
}
