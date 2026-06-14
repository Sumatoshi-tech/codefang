use cf_gitlib::changes::tree_diff;
use cf_gitlib::Repository;

fn main() {
    let repo = Repository::open("/home/dmitriy/sources/kubernetes").unwrap();
    let head = repo.head().unwrap();
    let commit = repo.lookup_commit(head).unwrap();
    println!("HEAD = {}", head);
    println!("num_parents = {}", commit.num_parents());
    let auth = commit.author();
    let comm = commit.committer();
    println!("author when secs={} off={}", auth.when.seconds(), auth.when.offset_minutes());
    println!("committer when secs={} off={}", comm.when.seconds(), comm.when.offset_minutes());

    let new_tree = commit.tree().unwrap();
    let parent = commit.parent(0).unwrap();
    let old_tree = parent.tree().unwrap();
    let changes = tree_diff(&repo, Some(&old_tree), Some(&new_tree)).unwrap();
    println!("changes = {}", changes.len());
    for c in &changes {
        let name = if c.to.name.is_empty() { &c.from.name } else { &c.to.name };
        println!("  {:?} {}", c.action, name);
    }
}
