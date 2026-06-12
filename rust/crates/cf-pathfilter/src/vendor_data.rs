//! Vendored vendor-path matcher patterns from `src-d/enry` v2.1.0.
//!
//! # Data parity (DESIGN §2.6)
//!
//! File-classification decisions change *which* bytes appear in machine reports
//! (pinned by `rust/tests/compat`), so the underlying data tables must match
//! the enry library **byte-for-byte**. We therefore vendor the **same**
//! regular-expression sources that `github.com/src-d/enry/v2@v2.1.0` compiles
//! in `data/vendor.go`, rather than swapping in a behaviourally different
//! detector.
//!
//! ## Relationship to `build.rs`
//!
//! The authoritative table at build time is the one `build.rs` extracts from the
//! local enry v2.1.0 `data/vendor.go` (`GENERATED_VENDOR_PATTERNS`). This in-tree
//! [`VENDOR_PATTERNS`] list is the offline fallback used when that source is not
//! present. It is a **verbatim transcription** of the same upstream file, in the
//! same order, so the two are identical — the `enry_vendor::tests::
//! vendor_patterns_match_enry_source` test asserts exactly that whenever the
//! generator ran. If you bump the pinned enry version, regenerate both: extract
//! each `substring.Regexp(`...`)` argument from that version's `data/vendor.go`
//! and replace this array (the backtick/raw literals are copied with no escape
//! translation).
//!
//! ## How enry matches
//!
//! enry's `data.VendorMatchers` is `substring.Or(substring.Regexp(p1), …)`; each
//! `substring.Regexp` performs an *unanchored* search, and `Or` is true if any
//! matches. enry's regexp engine is RE2-syntax, which the Rust [`regex`] crate
//! also implements, so identical source strings produce identical match
//! behaviour.
//!
//! [`regex`]: https://docs.rs/regex

/// Offline fallback table of vendor-path regex sources, byte-identical to enry
/// v2.1.0 `data/vendor.go` (`VendorMatchers`), in upstream order.
///
/// Each entry is fed to [`regex::Regex::new`] and matched with an unanchored
/// search, exactly mirroring enry's `substring.Regexp` + `regexp.MatchString`.
pub const VENDOR_PATTERNS: &[&str] = &[
    r"(^|/)cache/",
    r"^[Dd]ependencies/",
    r"(^|/)dist/",
    r"^deps/",
    r"(^|/)configure$",
    r"(^|/)config.guess$",
    r"(^|/)config.sub$",
    r"(^|/)aclocal.m4",
    r"(^|/)libtool.m4",
    r"(^|/)ltoptions.m4",
    r"(^|/)ltsugar.m4",
    r"(^|/)ltversion.m4",
    r"(^|/)lt~obsolete.m4",
    r"cpplint.py",
    r"node_modules/",
    r"bower_components/",
    r"^rebar$",
    r"erlang.mk",
    r"Godeps/_workspace/",
    r"(^|/)testdata/",
    r".indent.pro",
    r"(\.|-)min\.(js|css)$",
    r"([^\s]*)import\.(css|less|scss|styl)$",
    r"(^|/)bootstrap([^.]*)\.(js|css|less|scss|styl)$",
    r"(^|/)custom\.bootstrap([^\s]*)(js|css|less|scss|styl)$",
    r"(^|/)font-?awesome\.(css|less|scss|styl)$",
    r"(^|/)font-?awesome/.*\.(css|less|scss|styl)$",
    r"(^|/)foundation\.(css|less|scss|styl)$",
    r"(^|/)normalize\.(css|less|scss|styl)$",
    r"(^|/)skeleton\.(css|less|scss|styl)$",
    r"(^|/)[Bb]ourbon/.*\.(css|less|scss|styl)$",
    r"(^|/)animate\.(css|less|scss|styl)$",
    r"(^|/)materialize\.(css|less|scss|styl|js)$",
    r"(^|/)select2/.*\.(css|scss|js)$",
    r"(^|/)bulma\.(css|sass|scss)$",
    r"(3rd|[Tt]hird)[-_]?[Pp]arty/",
    r"vendors?/",
    r"extern(al)?/",
    r"(^|/)[Vv]+endor/",
    r"^debian/",
    r"run.n$",
    r"bootstrap-datepicker/",
    r"(^|/)jquery([^.]*)\.js$",
    r"(^|/)jquery\-\d\.\d+(\.\d+)?\.js$",
    r"(^|/)jquery\-ui(\-\d\.\d+(\.\d+)?)?(\.\w+)?\.(js|css)$",
    r"(^|/)jquery\.(ui|effects)\.([^.]*)\.(js|css)$",
    r"jquery.fn.gantt.js",
    r"jquery.fancybox.(js|css)",
    r"fuelux.js",
    r"(^|/)jquery\.fileupload(-\w+)?\.js$",
    r"jquery.dataTables.js",
    r"bootbox.js",
    r"pdf.worker.js",
    r"(^|/)slick\.\w+.js$",
    r"(^|/)Leaflet\.Coordinates-\d+\.\d+\.\d+\.src\.js$",
    r"leaflet.draw-src.js",
    r"leaflet.draw.css",
    r"Control.FullScreen.css",
    r"Control.FullScreen.js",
    r"leaflet.spin.js",
    r"wicket-leaflet.js",
    r".sublime-project",
    r".sublime-workspace",
    r".vscode",
    r"(^|/)prototype(.*)\.js$",
    r"(^|/)effects\.js$",
    r"(^|/)controls\.js$",
    r"(^|/)dragdrop\.js$",
    r"(.*?)\.d\.ts$",
    r"(^|/)mootools([^.]*)\d+\.\d+.\d+([^.]*)\.js$",
    r"(^|/)dojo\.js$",
    r"(^|/)MochiKit\.js$",
    r"(^|/)yahoo-([^.]*)\.js$",
    r"(^|/)yui([^.]*)\.js$",
    r"(^|/)ckeditor\.js$",
    r"(^|/)tiny_mce([^.]*)\.js$",
    r"(^|/)tiny_mce/(langs|plugins|themes|utils)",
    r"(^|/)ace-builds/",
    r"(^|/)fontello(.*?)\.css$",
    r"(^|/)MathJax/",
    r"(^|/)Chart\.js$",
    r"(^|/)[Cc]ode[Mm]irror/(\d+\.\d+/)?(lib|mode|theme|addon|keymap|demo)",
    r"(^|/)shBrush([^.]*)\.js$",
    r"(^|/)shCore\.js$",
    r"(^|/)shLegacy\.js$",
    r"(^|/)angular([^.]*)\.js$",
    r"(^|\/)d3(\.v\d+)?([^.]*)\.js$",
    r"(^|/)react(-[^.]*)?\.js$",
    r"(^|/)flow-typed/.*\.js$",
    r"(^|/)modernizr\-\d\.\d+(\.\d+)?\.js$",
    r"(^|/)modernizr\.custom\.\d+\.js$",
    r"(^|/)knockout-(\d+\.){3}(debug\.)?js$",
    r"(^|/)docs?/_?(build|themes?|templates?|static)/",
    r"(^|/)admin_media/",
    r"(^|/)env/",
    r"^fabfile\.py$",
    r"^waf$",
    r"^.osx$",
    r"\.xctemplate/",
    r"\.imageset/",
    r"(^|/)Carthage/",
    r"(^|/)Sparkle/",
    r"Crashlytics.framework/",
    r"Fabric.framework/",
    r"BuddyBuildSDK.framework/",
    r"Realm.framework",
    r"RealmSwift.framework",
    r"gitattributes$",
    r"gitignore$",
    r"gitmodules$",
    r"(^|/)gradlew$",
    r"(^|/)gradlew\.bat$",
    r"(^|/)gradle/wrapper/",
    r"(^|/)mvnw$",
    r"(^|/)mvnw\.cmd$",
    r"(^|/)\.mvn/wrapper/",
    r"-vsdoc\.js$",
    r"\.intellisense\.js$",
    r"(^|/)jquery([^.]*)\.validate(\.unobtrusive)?\.js$",
    r"(^|/)jquery([^.]*)\.unobtrusive\-ajax\.js$",
    r"(^|/)[Mm]icrosoft([Mm]vc)?([Aa]jax|[Vv]alidation)(\.debug)?\.js$",
    r"^[Pp]ackages\/.+\.\d+\/",
    r"(^|/)extjs/.*?\.js$",
    r"(^|/)extjs/.*?\.xml$",
    r"(^|/)extjs/.*?\.txt$",
    r"(^|/)extjs/.*?\.html$",
    r"(^|/)extjs/.*?\.properties$",
    r"(^|/)extjs/.sencha/",
    r"(^|/)extjs/docs/",
    r"(^|/)extjs/builds/",
    r"(^|/)extjs/cmd/",
    r"(^|/)extjs/examples/",
    r"(^|/)extjs/locale/",
    r"(^|/)extjs/packages/",
    r"(^|/)extjs/plugins/",
    r"(^|/)extjs/resources/",
    r"(^|/)extjs/src/",
    r"(^|/)extjs/welcome/",
    r"(^|/)html5shiv\.js$",
    r"^[Tt]ests?/fixtures/",
    r"^[Ss]pecs?/fixtures/",
    r"(^|/)cordova([^.]*)\.js$",
    r"(^|/)cordova\-\d\.\d(\.\d)?\.js$",
    r"foundation(\..*)?\.js$",
    r"^Vagrantfile$",
    r".[Dd][Ss]_[Ss]tore$",
    r"^vignettes/",
    r"^inst/extdata/",
    r"octicons.css",
    r"sprockets-octicons.scss",
    r"(^|/)activator$",
    r"(^|/)activator\.bat$",
    r"proguard.pro",
    r"proguard-rules.pro",
    r"^puphpet/",
    r"(^|/)\.google_apis/",
    r"^Jenkinsfile$",
];
