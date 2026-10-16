//! libgit2's memory limits are process-global state, so their contract is
//! tested in this file's own process.

use std::os::raw::c_int;

/// The mapped-pack limit the test registers: 64 MiB.
const MAPPED_PACK: usize = 64 * 1024 * 1024;
/// The object-cache limit the test registers: 96 MiB.
const OBJECT_CACHE: usize = 96 * 1024 * 1024;

/// libgit2's current `(mapped-pack limit, object-cache limit)`.
fn read_limits() -> (usize, usize) {
    // SAFETY: this test is the process's only thread using libgit2; both
    // options only read globals into the out-parameters.
    unsafe {
        let mapped = git2::opts::get_mwindow_mapped_limit().expect("read the mapped limit");
        let (mut used, mut allowed): (isize, isize) = (0, 0);
        let rc = libgit2_sys::git_libgit2_opts(
            libgit2_sys::GIT_OPT_GET_CACHED_MEMORY as c_int,
            &mut used as *mut isize,
            &mut allowed as *mut isize,
        );
        assert_eq!(rc, 0, "read the object-cache limit");
        (
            mapped,
            usize::try_from(allowed).expect("non-negative limit"),
        )
    }
}

/// Limits registered before the process opens its first repository take
/// effect at that open; limits registered afterwards are refused and change
/// nothing (libgit2 options must not change under live repositories).
#[test]
fn the_native_limits_apply_at_the_first_open_and_only_then() {
    let dir = std::env::temp_dir().join(format!("cf-gitlib-native-limits-{}", std::process::id()));
    drop(git2::Repository::init(&dir).expect("init a repository"));
    let limits = cf_gitlib::NativeLimits {
        mapped_pack: MAPPED_PACK,
        object_cache: OBJECT_CACHE,
    };

    assert!(
        cf_gitlib::limit_native_memory(limits),
        "registered before the first open"
    );
    let repo = cf_gitlib::Repository::open(dir.to_str().expect("utf-8 path")).expect("open");
    assert_eq!(read_limits(), (MAPPED_PACK, OBJECT_CACHE));

    let halved = cf_gitlib::NativeLimits {
        mapped_pack: MAPPED_PACK / 2,
        object_cache: OBJECT_CACHE / 2,
    };
    assert!(
        !cf_gitlib::limit_native_memory(halved),
        "limits after the first open are refused"
    );
    assert_eq!(read_limits(), (MAPPED_PACK, OBJECT_CACHE));

    drop(repo);
    std::fs::remove_dir_all(&dir).expect("remove the repository");
}
