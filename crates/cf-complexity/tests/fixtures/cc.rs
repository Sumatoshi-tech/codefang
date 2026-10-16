// cyclomatic 9 (if,&&,else-if,||,for,while,2 arms), cognitive 8
fn rs_decisions(a: i32, b: i32, xs: &[i32]) -> i32 {
    let mut r = 0;
    if a > 0 && b > 0 { r += 1; } else if a < 0 || b < 0 { r -= 1; } else { r = 2; }
    for x in xs { r += x; }
    while r > 100 { r -= 1; }
    match r { 1 => r = 10, 2 => r = 20, _ => r = 0 }
    r
}

// cyclomatic 4, cognitive 6, nesting 3
fn rs_nested(a: bool, b: bool, c: bool) -> i32 {
    if a {
        if b {
            if c {
                return 1;
            }
        }
    }
    0
}

struct S;
impl S {
    // cyclomatic 2, cognitive 1
    fn method(&self, a: bool) -> i32 {
        if a { return 1; }
        0
    }
}
