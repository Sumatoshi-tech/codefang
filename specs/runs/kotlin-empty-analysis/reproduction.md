# Kotlin reproduction evidence

The public Spring Petclinic Kotlin archive was downloaded from its `main`
branch. The corresponding repository commit is
`817dba3b02261e4291f138d5d8bb8c85c7d49758`.
The archive's PAX `comment` also confirms that commit.
Archive SHA-256:
`4a7aceec9ff3de3d13eb52ae86eb1f2956b4fc998c74fb265998c7f7c17cfe38`.

Command used before and after replacing the installed binary:

```sh
codefang run -a static/complexity --format json --per-file /tmp/codefang-kotlin-repro/spring-petclinic-kotlin-main
```

The JSON evidence files retain the metrics and analyzed-file count from the
full reports. Before: zero functions. After: 96 functions, cyclomatic total
120, maximum 5, across 73 analyzed files.

The original executables are preserved for rollback under
`/tmp/codefang-kotlin-repro/previous-bin/`. Raw reports and build/check logs
are under `/tmp/codefang-kotlin-repro/`.

The Kotlin companion-object parser probe before replacement failed with the
message retained in `parser-before.stderr`.
