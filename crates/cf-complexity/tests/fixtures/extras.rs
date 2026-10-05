fn outer(a: Option<i32>, b: bool) -> i32 {
    fn inner(c: bool) -> i32 {
        if c { 1 } else { 0 }
    }
    let g = |x: bool| if x { 1 } else { 2 };
    if let Some(v) = a {
        if v > 1 { return inner(b); }
    } else {
        if b { return g(b); }
    }
    let mut n = 0;
    'outer: for i in 0..3 { for j in 0..3 { if i == j { continue 'outer; } } }
    match n { x if x > 1 => 1, 2 | 3 => 2, _ => 0 }
}

trait T { fn m(&self) -> i32; fn d(&self) -> i32 { 1 } }
