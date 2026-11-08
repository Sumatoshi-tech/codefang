# Shotness language audit evidence

The initial real-CLI matrix had 26 passing and 14 failing language paths.
`matrix-before.txt` retains those failures. Further compiled-registry inspection
added LaTeX missing/wrapped names; `latex-name-before.txt` and
`name-precedence-before.txt` retain its name-extraction failures.
`root-capture-before.txt` records a matcher returning `111` instead of `probe`.

The final matrix has 64 tests covering representative declarations in all 42
filename-selected function-declaring mappings, with CSS reference-only controls,
related declaration forms and a static-complexity cross-check. The fixtures have
no parser errors. A registry-derived gate prevents silently omitting another
selectable function-declaring mapping.

Run the versioned regression matrix:

```sh
cargo test -p codefang --test shotness_languages_e2e
cargo test -p cf-uast-mappings --test shotness_fixtures
```

Installed-binary verification also used one real repository containing all 63
fixture files, with introduction and body-edit commits:

```sh
codefang run -a history/shotness --limit 200 --format text /tmp/cf-gitlib-test-1117334-1794154026237746179-0
codefang run -a history/shotness --limit 200 --format json /tmp/cf-gitlib-test-1117334-1794154026237746179-0
```

Every reported name and touch count was compared with the versioned fixtures.
The old-style LaTeX definition has a later-line body edit outside the grammar's
header node and retains one introduction touch. The other named entities have
two touches. See `installed-polyglot-summary.json` and `installed-polyglot.txt`.
The previous binary reported 37 entities; the corrected binary reports 71.

Registry routing and grammar limits are documented in
[the coverage guide](../../../docs/shotness-languages.md).
