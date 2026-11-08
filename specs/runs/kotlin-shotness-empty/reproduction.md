# Kotlin Shotness reproduction

Repository: https://github.com/spring-petclinic/spring-petclinic-kotlin

HEAD: `817dba3b02261e4291f138d5d8bb8c85c7d49758`.
The repository was cloned through libgit2 with its full commit history.
A libgit2 revision walk confirms 844 commits reachable from HEAD, so the
requested 200-commit bound selects a partial history window.
The private `top-my-credits` repository from the screenshot is not available.

Exact flags used before and after the source fix:

```sh
codefang run -a history/shotness --limit 200 --format text /tmp/codefang-kotlin-repro/petclinic-history
```

Before: 0 nodes, 0 changes, 0 couplings.
After: 14 nodes, 21 changes, 29 couplings.

`public-before.txt` and `public-after.txt` retain the text reports.
`regression-before.txt` retains the failing real-binary regression on a
hermetic Kotlin Git fixture. The earlier static-complexity verification did
not exercise Shotness's declaration-name query.

`nested-companion-before.txt` retains the failing naming-edge regression:
the outer implicit companion must keep its `Companion` name when a nested
class declares a named companion.
