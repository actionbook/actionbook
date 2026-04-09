fn main() {
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let build_version = if hash.is_empty() {
        pkg_version
    } else {
        format!("{pkg_version}-{hash}")
    };

    println!("cargo:rustc-env=BUILD_VERSION={build_version}");

    // Track git state so BUILD_VERSION updates on new commits.
    //
    // In a worktree, `git rev-parse --git-dir` returns something like
    // `.git/worktrees/<name>`. The HEAD file there is a ref pointer
    // (e.g. `ref: refs/heads/branch-name`) and doesn't change on new
    // commits. We must ALSO watch the packed-refs and the actual ref file
    // so that cargo reruns build.rs when the branch tip moves.
    let git_dir = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    let git_common_dir = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
        .trim()
        .to_string();

    if !git_dir.is_empty() {
        // HEAD itself (detects branch switches)
        println!("cargo:rerun-if-changed={git_dir}/HEAD");

        // Read HEAD to find the ref it points to, then watch that ref file.
        // This ensures build.rs reruns when a new commit is added to the branch.
        if let Ok(head_content) = std::fs::read_to_string(format!("{git_dir}/HEAD")) {
            let head_content = head_content.trim();
            if let Some(ref_path) = head_content.strip_prefix("ref: ") {
                // Watch the loose ref in the common git dir (handles worktrees)
                let common = if git_common_dir.is_empty() {
                    &git_dir
                } else {
                    &git_common_dir
                };
                println!("cargo:rerun-if-changed={common}/{ref_path}");
                // Also watch packed-refs since the ref might be packed
                println!("cargo:rerun-if-changed={common}/packed-refs");
            }
        }
    }
}
