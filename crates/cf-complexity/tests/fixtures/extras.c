int proto(int a);

int outer(int a, int b) {
    if (a) {
        if (a > 1) { return 1; }
    } else {
        if (a < 0) { return -1; }
    }
    if (a) if (b) return 2;
    if (a) b++; else if (b) a++; else a--;
    do { a--; } while (a > 0);
    for (;;) { if (a++ > 10) break; }
    return (a || b) && !a;
}
