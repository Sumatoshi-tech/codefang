public abstract class Probe {
    abstract int a(int x);
    interface I { int i(int x); }
    int outer(int a) {
        Runnable r = () -> { if (a > 0) { System.out.println(a); } };
        if (a > 0) {
            if (a > 1) { return 1; }
        } else {
            if (a < 0) { return -1; }
        }
        do { a--; } while (a > 0);
        outer: for (int i = 0; i < 3 && a > 0; i++) { if (i == 2) { break outer; } }
        if (a > 0 && a < 10 || a == 1) { return 1; }
        return 0;
    }
}
