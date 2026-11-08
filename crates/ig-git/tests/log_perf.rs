//! IdeaGit performance smoke test: a synthetic repository of ~50,000 commits with feature
//! branches, merges and tags (generated with `git fast-import`), loaded into the log.
//!
//! Ignored by default. Run in release mode to see the timings:
//! `cargo test --release -p ig-git --test log_perf -- --ignored --nocapture`

mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::log_helpers::{commit_at, provider};
use common::{SCRUBBED_ENV, git};
use ig_log::data::graph_data::build_data;
use ig_log::data::refresher::compact_commit;
use ig_log::data::{
    InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider, VcsLogRefresher, VcsLogStorage,
    vcs_root,
};
use ig_log::{Options, PermanentGraph, VisibleGraph};

const TOTAL_COMMITS: usize = 50_000;

struct Stream {
    s: String,
    mark: usize,
    time: i64,
}

impl Stream {
    fn commit(&mut self, r: &str, from: Option<usize>, merge: Option<usize>, msg: &str) -> usize {
        self.mark += 1;
        self.time += 1;
        let (mark, time) = (self.mark, self.time);
        self.s.push_str(&format!(
            "commit {r}\nmark :{mark}\ncommitter Perf <perf@example.com> {time} +0000\ndata {}\n{msg}\n",
            msg.len()
        ));
        if let Some(from) = from {
            self.s.push_str(&format!("from :{from}\n"));
        }
        if let Some(merge) = merge {
            self.s.push_str(&format!("merge :{merge}\n"));
        }
        self.s.push_str(&format!(
            "M 644 inline f{}\ndata 2\n{}\n\n",
            mark % 97,
            mark % 10
        ));
        mark
    }
}

/// Writes a fast-import stream: a main line where every 40th commit forks a feature branch of
/// 3..20 commits; two of three features are merged back, the others stay as branches. A tag every
/// 1000 main commits. Returns the stream, the number of commits and of branch refs.
fn fast_import_stream() -> (String, usize, usize) {
    let mut st = Stream {
        s: String::with_capacity(TOTAL_COMMITS * 120),
        mark: 0,
        time: 1_500_000_000,
    };
    let mut main_head: Option<usize> = None;
    let mut branches = 0;
    let mut main_count = 0usize;
    while st.mark < TOTAL_COMMITS {
        main_count += 1;
        main_head = Some(st.commit(
            "refs/heads/master",
            main_head,
            None,
            &format!("main {main_count}"),
        ));
        if main_count % 40 == 0 {
            let feature = main_count / 40;
            let name = format!("refs/heads/feature/{feature}");
            let len = 3 + feature % 18;
            let mut head = main_head;
            for k in 0..len {
                head = Some(st.commit(&name, head, None, &format!("feature {feature} commit {k}")));
            }
            if feature % 3 != 0 {
                main_head = Some(st.commit(
                    "refs/heads/master",
                    main_head,
                    head,
                    &format!("merge feature {feature}"),
                ));
                st.s.push_str(&format!(
                    "reset {name}\nfrom 0000000000000000000000000000000000000000\n\n"
                ));
            } else {
                branches += 1;
            }
        }
        if main_count % 1000 == 0 {
            st.s.push_str(&format!(
                "reset refs/tags/v{}\nfrom :{}\n\n",
                main_count / 1000,
                main_head.unwrap()
            ));
        }
    }
    (st.s, st.mark, branches + 1)
}

fn generate_repository(dir: &Path) -> (usize, usize) {
    git(dir, &["init", "-q", "-b", "master"]);
    let (stream, commits, branches) = fast_import_stream();
    let mut cmd = Command::new("git");
    cmd.args(["fast-import", "--quiet"])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped());
    for v in SCRUBBED_ENV {
        cmd.env_remove(v);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    git(dir, &["reset", "-q", "--hard", "master"]);
    (commits, branches)
}

fn ms(d: Duration) -> String {
    format!("{:>8.1} ms", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "performance smoke test; run with --release -- --ignored --nocapture"]
fn load_50k_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("big");
    std::fs::create_dir_all(&dir).unwrap();
    let start = Instant::now();
    let (commits, branches) = generate_repository(&dir);
    eprintln!(
        "generated {commits} commits, {branches} branches with fast-import in {}",
        ms(start.elapsed())
    );
    let root = vcs_root(&dir);
    let provider = Arc::new(provider());

    // git alone, same arguments as read_all_hashes, output discarded (warms the caches too)
    let start = Instant::now();
    let status = Command::new("git")
        .args(["log", "HEAD", "--branches", "--remotes", "--tags", "--date-order", "--decorate=full"])
        .arg("--pretty=format:%x01%x01%H%x02%x02%P%x02%x02%ct%x02%x02%an%x02%x02%ae%x02%x02%d%x03%x03")
        .current_dir(&dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let t_git = start.elapsed();

    // step by step
    let start = Instant::now();
    let mut timed = Vec::with_capacity(commits);
    let data = provider
        .read_all_hashes(&root, &mut |c| timed.push(c))
        .unwrap();
    let t_read = start.elapsed();
    assert_eq!(timed.len(), commits);

    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let start = Instant::now();
    let graph_commits: Vec<_> = timed
        .iter()
        .map(|c| compact_commit(storage.as_ref(), c, &root))
        .collect();
    let refs = RootRefsModel::create(data.refs.iter().cloned(), storage.as_ref());
    let t_compact = start.elapsed();

    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
    let start = Instant::now();
    let pack = build_data(
        &graph_commits,
        BTreeMap::from([(root.clone(), Arc::new(refs))]),
        &providers,
        &storage,
        true,
    );
    let t_build = start.elapsed();

    let graph = pack.permanent_graph().unwrap();
    let start = Instant::now();
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let t_visible = start.elapsed();
    assert_eq!(visible.visible_commit_count() as usize, commits);

    let start = Instant::now();
    let mut elements = 0;
    for row in 0..1000 {
        elements += visible.get_row_info(row).print_elements().len();
    }
    let t_rows = start.elapsed();
    assert!(elements > 1000);

    let start = Instant::now();
    let width = visible.recommended_width();
    let t_width = start.elapsed();

    // end to end, as the UI does it
    let storage = Arc::new(InMemoryStorage::new());
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers.clone(), 1000);
    let start = Instant::now();
    let full = refresher.initialize();
    let t_initialize = start.elapsed();
    assert_eq!(full.commits_count(), commits);

    commit_at(&dir, "new.txt", 2_000_000_000);
    let start = Instant::now();
    let refreshed = refresher.refresh(std::slice::from_ref(&root));
    let t_refresh = start.elapsed();
    assert_eq!(refreshed.commits_count(), commits + 1);
    assert!(refreshed.is_full() && refreshed.load_error().is_none());

    eprintln!("git log alone (output discarded)      {}", ms(t_git));
    eprintln!("read_all_hashes (git log + parse)     {}", ms(t_read));
    eprintln!("intern ids + refs model               {}", ms(t_compact));
    eprintln!("build data pack (permanent graph)     {}", ms(t_build));
    eprintln!("create visible graph                  {}", ms(t_visible));
    eprintln!("print elements of 1000 rows           {}", ms(t_rows));
    eprintln!(
        "recommended width (width {width:>3})        {}",
        ms(t_width)
    );
    eprintln!("refresher.initialize (block + full)   {}", ms(t_initialize));
    eprintln!("refresher.refresh (1 new commit)      {}", ms(t_refresh));
}
