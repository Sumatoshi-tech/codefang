//! Empirical probe: classify files like the devs/anomaly language plumbing
//! (fast extension path, then the enry cascade). One `path -> lang` line each.
fn main() {
    for f in std::env::args().skip(1) {
        let data = std::fs::read(&f).unwrap_or_default();
        let lang = cf_langpath::language_by_path_with_content(&f, &data).unwrap_or_default();
        println!("{f} -> {lang}");
    }
}
