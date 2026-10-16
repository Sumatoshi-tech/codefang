public class Foo {
    // cyclomatic 11, cognitive 10
    public int decisions(int a, int b, int[] xs) {
        int r = 0;
        if (a > 0 && b > 0) { r++; } else if (a < 0 || b < 0) { r--; } else { r = 2; }
        for (int x : xs) { r += x; }
        while (r > 100) { r--; }
        int t = a > 0 ? 1 : 2;
        try { r = r / a; } catch (ArithmeticException e) { r = 0; }
        switch (r) { case 1: r = 10; break; case 2: r = 20; break; default: r = 0; }
        return r + t;
    }

    // cyclomatic 4, cognitive 6, nesting 3
    public int nested(boolean a, boolean b, boolean c) {
        if (a) {
            if (b) {
                if (c) {
                    return 1;
                }
            }
        }
        return 0;
    }
}
