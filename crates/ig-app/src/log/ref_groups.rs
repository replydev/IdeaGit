//! Groups the references of a commit for its Log row label.
//!
//! Port of `GitRefManager.groupForTable` (`plugins/git4idea/backend/src/log/GitRefManager.kt`),
//! `SimpleRefGroup.buildGroups` / `getColors` and `DetachedHeadRefGroup`
//! (`platform/vcs-log/impl/src/com/intellij/vcs/log/impl/`).
//!
//! Deviations: tracked groups (`origin & main`) are found by name only (upstream's
//! `createTrackedGroup` fallback: a remote branch named `<remote>/<local>`), because branch tracking
//! info isn't read yet; ref type colors are [`RefColor`] keys the UI maps to theme colors
//! (`VersionControl.GitLog.*IconColor`).

use ig_git::log::ref_manager::{HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use ig_log::data::{VcsRef, VcsRefType};

/// `GitRefManager.REMOTE_TABLE_SEPARATOR`.
const REMOTE_TABLE_SEPARATOR: &str = " & ";

/// A ref type's label color (upstream `VcsRefType.getBackgroundColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefColor {
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
    Other,
}

impl RefColor {
    pub fn of(ref_type: VcsRefType) -> RefColor {
        match ref_type {
            t if t == HEAD => RefColor::Head,
            t if t == LOCAL_BRANCH => RefColor::LocalBranch,
            t if t == REMOTE_BRANCH => RefColor::RemoteBranch,
            t if t == TAG => RefColor::Tag,
            _ => RefColor::Other,
        }
    }
}

/// A label group (`RefGroup`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefGroup {
    pub name: String,
    pub refs: Vec<VcsRef>,
    /// `DetachedHeadRefGroup`: a warning label instead of ref colors.
    pub detached_head: bool,
}

impl RefGroup {
    fn simple(name: impl Into<String>, refs: Vec<VcsRef>) -> RefGroup {
        RefGroup {
            name: name.into(),
            refs,
            detached_head: false,
        }
    }

    /// `SimpleRefGroup.getColors`: one color per ref type in order of appearance, twice when the
    /// type has several refs. Detached-head groups have none.
    pub fn colors(&self) -> Vec<RefColor> {
        if self.detached_head {
            return Vec::new();
        }
        let mut types: Vec<(VcsRefType, usize)> = Vec::new();
        for r in &self.refs {
            match types.iter_mut().find(|(t, _)| *t == r.ref_type()) {
                Some((_, n)) => *n += 1,
                None => types.push((r.ref_type(), 1)),
            }
        }
        types
            .into_iter()
            .flat_map(|(t, n)| std::iter::repeat_n(RefColor::of(t), if n > 1 { 2 } else { 1 }))
            .collect()
    }
}

/// The repository state grouping needs.
#[derive(Clone, Debug, Default)]
pub struct RepoRefsContext<'a> {
    /// `None` when HEAD is detached (or there are no commits).
    pub current_branch: Option<&'a str>,
    pub remotes: &'a [String],
}

/// `groupForTable(references, compact, showTagNames)`. `sorted` must already be in label order
/// (`VcsLogRefManager::sort_labels`); `detached_head_name` is `VcsLogBundle`'s
/// `vcs.log.references.detached.head`.
pub fn group_for_table(
    sorted: &[VcsRef],
    repo: &RepoRefsContext<'_>,
    compact: bool,
    show_tag_names: bool,
    detached_head_name: &str,
) -> Vec<RefGroup> {
    if sorted.is_empty() {
        return Vec::new();
    }
    // `ContainerUtil.groupBy(sortedReferences) { it.type }`: types in order of appearance
    let mut grouped: Vec<(VcsRefType, Vec<VcsRef>)> = Vec::new();
    for r in sorted {
        match grouped.iter_mut().find(|(t, _)| *t == r.ref_type()) {
            Some((_, refs)) => refs.push(r.clone()),
            None => grouped.push((r.ref_type(), vec![r.clone()])),
        }
    }
    let take = |grouped: &mut Vec<(VcsRefType, Vec<VcsRef>)>, t: VcsRefType| -> Vec<VcsRef> {
        match grouped.iter().position(|(gt, _)| *gt == t) {
            Some(i) => grouped.remove(i).1,
            None => Vec::new(),
        }
    };
    let head_refs = take(&mut grouped, HEAD);

    // tracked groups (name-based `createTrackedGroup`)
    let mut groups: Vec<RefGroup> = Vec::new();
    let locals: Vec<VcsRef> = grouped
        .iter()
        .find(|(t, _)| *t == LOCAL_BRANCH)
        .map(|(_, r)| r.clone())
        .unwrap_or_default();
    let mut tracked = Vec::new();
    for local in &locals {
        let candidate = repo.remotes.iter().find_map(|remote| {
            let name = format!("{remote}/{}", local.name());
            grouped
                .iter()
                .find(|(t, _)| *t == REMOTE_BRANCH)
                .and_then(|(_, rs)| rs.iter().find(|r| r.name() == name))
                .map(|r| (remote.clone(), r.clone()))
        });
        if let Some((remote, remote_ref)) = candidate {
            tracked.push(RefGroup::simple(
                format!("{remote}{REMOTE_TABLE_SEPARATOR}{}", local.name()),
                vec![local.clone(), remote_ref],
            ));
        }
    }
    for group in &tracked {
        for (_, refs) in grouped.iter_mut() {
            refs.retain(|r| !group.refs.contains(r));
        }
    }
    grouped.retain(|(_, refs)| !refs.is_empty());

    // current branch first
    let current = repo.current_branch.and_then(|branch| {
        let (_, locals) = grouped.iter_mut().find(|(t, _)| *t == LOCAL_BRANCH)?;
        let i = locals.iter().position(|r| r.name() == branch)?;
        Some(RefGroup::simple(branch, vec![locals.remove(i)]))
    });
    grouped.retain(|(_, refs)| !refs.is_empty());
    groups.extend(current);
    groups.extend(tracked);

    let mut result = build_groups(groups, grouped, compact, show_tag_names);
    if head_refs.is_empty() {
        return result;
    }
    if repo.current_branch.is_none() {
        result.insert(
            0,
            RefGroup {
                name: detached_head_name.to_string(),
                refs: head_refs,
                detached_head: true,
            },
        );
    } else if let Some(first) = result.first_mut() {
        first.refs.splice(0..0, head_refs);
    } else {
        result.push(RefGroup::simple("", head_refs));
    }
    result
}

/// `SimpleRefGroup.buildGroups`.
fn build_groups(
    ref_groups: Vec<RefGroup>,
    refs: Vec<(VcsRefType, Vec<VcsRef>)>,
    compact: bool,
    show_tag_names: bool,
) -> Vec<RefGroup> {
    if refs.is_empty() {
        return ref_groups;
    }
    if compact {
        let first_ref = &refs[0].1[0];
        if ref_groups.is_empty() {
            let name = if first_ref.ref_type().is_branch() || show_tag_names {
                first_ref.name().to_string()
            } else {
                String::new()
            };
            return vec![RefGroup::simple(
                name,
                refs.into_iter().flat_map(|(_, r)| r).collect(),
            )];
        }
        let name = ref_groups[0].name.clone();
        let all = ref_groups
            .into_iter()
            .flat_map(|g| g.refs)
            .chain(refs.into_iter().flat_map(|(_, r)| r))
            .collect();
        return vec![RefGroup::simple(name, all)];
    }
    let mut result = ref_groups;
    for (ref_type, refs_of_type) in refs {
        if ref_type.is_branch() {
            result.extend(refs_of_type.into_iter().map(|r| {
                let name = r.name().to_string();
                RefGroup::simple(name, vec![r])
            }));
        } else {
            let name = if show_tag_names {
                refs_of_type[0].name().to_string()
            } else {
                String::new()
            };
            result.push(RefGroup::simple(name, refs_of_type));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use ig_log::data::{Hash, vcs_root};

    use super::*;

    fn r(name: &str, t: VcsRefType) -> VcsRef {
        VcsRef::new(Hash::build("abcd"), name, t, vcs_root("/r"))
    }

    fn names(groups: &[RefGroup]) -> Vec<(&str, usize)> {
        groups
            .iter()
            .map(|g| (g.name.as_str(), g.refs.len()))
            .collect()
    }

    #[test]
    fn non_compact_groups_current_tracked_then_each_branch_and_tags_together() {
        let refs = [
            r("HEAD", HEAD),
            r("main", LOCAL_BRANCH),
            r("feature", LOCAL_BRANCH),
            r("origin/feature", REMOTE_BRANCH),
            r("origin/other", REMOTE_BRANCH),
            r("v1", TAG),
            r("v2", TAG),
        ];
        let remotes = ["origin".to_string()];
        let repo = RepoRefsContext {
            current_branch: Some("main"),
            remotes: &remotes,
        };
        let groups = group_for_table(&refs, &repo, false, false, "Detached HEAD");
        assert_eq!(
            names(&groups),
            [
                ("main", 2),
                ("origin & feature", 2),
                ("origin/other", 1),
                ("", 2)
            ]
        );
        // HEAD joined the first group
        assert_eq!(groups[0].colors(), [RefColor::Head, RefColor::LocalBranch]);
        assert_eq!(groups[3].colors(), [RefColor::Tag, RefColor::Tag]);
    }

    #[test]
    fn compact_view_makes_one_group_named_after_the_first() {
        let refs = [
            r("main", LOCAL_BRANCH),
            r("origin/main", REMOTE_BRANCH),
            r("v1", TAG),
        ];
        let remotes = ["origin".to_string()];
        let repo = RepoRefsContext {
            current_branch: Some("dev"),
            remotes: &remotes,
        };
        let groups = group_for_table(&refs, &repo, true, false, "Detached HEAD");
        assert_eq!(names(&groups), [("origin & main", 3)]);
        assert_eq!(
            groups[0].colors(),
            [RefColor::LocalBranch, RefColor::RemoteBranch, RefColor::Tag]
        );
        // only tags: no name unless tag names are shown
        let tags = [r("v1", TAG), r("v2", TAG)];
        assert_eq!(
            names(&group_for_table(&tags, &repo, true, false, "")),
            [("", 2)]
        );
        assert_eq!(
            names(&group_for_table(&tags, &repo, true, true, "")),
            [("v1", 2)]
        );
    }

    #[test]
    fn detached_head_gets_a_warning_group_first() {
        let refs = [r("HEAD", HEAD), r("v1", TAG)];
        let repo = RepoRefsContext {
            current_branch: None,
            remotes: &[],
        };
        let groups = group_for_table(&refs, &repo, true, false, "Detached HEAD");
        assert_eq!(names(&groups), [("Detached HEAD", 1), ("", 1)]);
        assert!(groups[0].detached_head && groups[0].colors().is_empty());
    }
}
