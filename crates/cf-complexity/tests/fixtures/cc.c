/* cyclomatic 10, cognitive 9 (+1 if goto counted) */
int c_decisions(int a, int b, int *xs, int n) {
    int r = 0;
    if (a > 0 && b > 0) { r++; } else if (a < 0 || b < 0) { r--; } else { r = 2; }
    for (int i = 0; i < n; i++) { r += xs[i]; }
    while (r > 100) { r--; }
    int t = a ? 1 : 2;
    switch (r) { case 1: r = 10; break; case 2: r = 20; break; default: r = 0; }
    goto done;
done:
    return r + t;
}

/* cyclomatic 4, cognitive 6, nesting 3 */
int c_nested(int a, int b, int c) {
    if (a) {
        if (b) {
            if (c) {
                return 1;
            }
        }
    }
    return 0;
}
