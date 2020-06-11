use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use librad::surf;
use librad::surf::git::{git2, BranchName, Browser};

use crate::error;

use super::Peer;

/// Branch name representation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Branch(pub(super) String);

impl fmt::Display for Branch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Tag name representation.
///
/// We still need full tag support.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Tag(pub(super) String);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Representation of a person (e.g. committer, author, signer) from a repository. Usually
/// extracted from a signature.
pub struct Person {
    /// Name part of the commit signature.
    pub name: String,
    /// Email part of the commit signature.
    pub email: String,
    /// Reference (url/uri) to a persons avatar image.
    pub avatar: String,
}

/// Representation of a code commit.
pub struct Commit {
    /// Identifier of the commit in the form of a sha1 hash. Often referred to as oid or object
    /// id.
    pub sha1: git2::Oid,
    /// The author of the commit.
    pub author: Person,
    /// The summary of the commit message body.
    pub summary: String,
    /// The entire commit message body.
    pub message: String,
    /// The committer of the commit.
    pub committer: Person,
    /// The recorded time of the committer signature. This is a convenience alias until we
    /// expose the actual author and commiter signatures.
    pub committer_time: git2::Time,
}

impl Commit {
    /// Returns the commit description text. This is the text after the one-line summary.
    #[must_use]
    pub fn description(&self) -> &str {
        self.message
            .strip_prefix(&self.summary)
            .unwrap_or(&self.message)
            .trim()
    }
}

impl From<&surf::vcs::git::Commit> for Commit {
    fn from(commit: &surf::vcs::git::Commit) -> Self {
        let avatar = |input: &String| {
            let mut s = DefaultHasher::new();
            input.hash(&mut s);

            format!(
                "https://avatars.dicebear.com/v2/jdenticon/{}.svg",
                s.finish().to_string()
            )
        };

        Self {
            sha1: commit.id,
            author: Person {
                name: commit.author.name.clone(),
                email: commit.author.email.clone(),
                avatar: avatar(&commit.author.email),
            },
            summary: commit.summary.clone(),
            message: commit.message.clone(),
            committer: Person {
                name: commit.committer.name.clone(),
                email: commit.committer.email.clone(),
                avatar: avatar(&commit.committer.email),
            },
            committer_time: commit.author.time,
        }
    }
}

/// Git object types.
///
/// `shafiul.github.io/gitbook/1_the_git_object_model.html`
#[derive(Debug, Eq, Ord, PartialOrd, PartialEq)]
pub enum ObjectType {
    /// References a list of other trees and blobs.
    Tree,
    /// Used to store file data.
    Blob,
}

/// Set of extra information we carry for blob and tree objects returned from the API.
pub struct Info {
    /// Name part of an object.
    pub name: String,
    /// The type of the object.
    pub object_type: ObjectType,
    /// The last commmit that touched this object.
    pub last_commit: Option<Commit>,
}

/// File data abstraction.
pub struct Blob {
    /// Actual content of the file, if the content is ASCII.
    pub content: BlobContent,
    /// Extra info for the file.
    pub info: Info,
    /// Absolute path to the object from the root of the repo.
    pub path: String,
}

impl Blob {
    /// Indicates if the content of the [`Blob`] is binary.
    #[must_use]
    pub fn is_binary(&self) -> bool {
        self.content == BlobContent::Binary
    }
}

/// Variants of blob content.
#[derive(PartialEq)]
pub enum BlobContent {
    /// Content is ASCII and can be passed as a string.
    Ascii(String),
    /// Content is binary and needs special treatment.
    Binary,
}

/// Result of a directory listing, carries other trees and blobs.
pub struct Tree {
    /// Absolute path to the tree object from the repo root.
    pub path: String,
    /// Entries listed in that tree result.
    pub entries: Vec<TreeEntry>,
    /// Extra info for the tree object.
    pub info: Info,
}

/// Entry in a Tree result.
pub struct TreeEntry {
    /// Extra info for the entry.
    pub info: Info,
    /// Absolute path to the object from the root of the repo.
    pub path: String,
}

/// Returns the [`Blob`] for a file at `revision` under `path`.
///
/// # Errors
///
/// Will return [`error::Error`] if the project doesn't exist or a surf interaction fails.
pub fn blob(
    peer: &Peer,
    project_urn: &str,
    default_branch: String, // TODO(finto): This should be handled by the broweser surf#115
    revision: Option<String>,
    maybe_path: Option<String>,
) -> Result<Blob, error::Error> {
    let api = peer.api.lock().map_err(|_| error::Error::LibradLock)?;
    let repo = api.storage().open_repo(project_urn.parse()?)?;
    let browser = repo.browser(&revision.unwrap_or_else(|| default_branch))?;

    let root = browser.get_directory()?;
    let path = maybe_path.clone().unwrap_or_default();
    let p = surf::file_system::Path::from_str(&path)?;

    let file = root
        .find_file(p.clone())
        .ok_or_else(|| error::Error::PathNotFound(p.clone()))?;

    let mut commit_path = surf::file_system::Path::root();
    commit_path.append(p.clone());

    let last_commit = browser.last_commit(commit_path)?.map(|c| Commit::from(&c));
    let (_rest, last) = p.split_last();
    let content = match std::str::from_utf8(&file.contents) {
        Ok(content) => BlobContent::Ascii(content.to_string()),
        Err(_) => BlobContent::Binary,
    };

    Ok(Blob {
        content,
        info: Info {
            name: last.to_string(),
            object_type: ObjectType::Blob,
            last_commit,
        },
        path: maybe_path.unwrap_or(last.to_string()),
    })
}

/// Given a project id to a repo returns the list of branches.
///
/// # Errors
///
/// Will return [`error::Error`] if the project doesn't exist or the surf interaction fails.
pub fn branches<'repo>(browser: &Browser<'repo>) -> Result<Vec<Branch>, error::Error> {
    let mut branches = browser
        .list_branches(None)?
        .into_iter()
        .map(|b| Branch(b.name.name().to_string()))
        .collect::<Vec<Branch>>();

    branches.sort();

    Ok(branches)
}

/// Information about a locally checked out repository.
#[derive(Deserialize, Serialize)]
pub struct LocalState {
    /// List of branches.
    branches: Vec<Branch>,
    /// Indicator if the repository is associated to coco project.
    managed: bool,
}

/// Given a path to a repo returns the list of branches and if it is managed by coco.
///
/// # Errors
///
/// Will return [`error::Error`] if the repository doesn't exist.
pub fn local_state(repo_path: &str) -> Result<LocalState, error::Error> {
    let repo = surf::vcs::git::Repository::new(repo_path)?;
    let browser = Browser::new(&repo, "master")?;
    let mut branches = browser
        .list_branches(None)?
        .into_iter()
        .map(|b| Branch(b.name.name().to_string()))
        .collect::<Vec<Branch>>();

    branches.sort();

    let managed = {
        let repo = git2::Repository::open(repo_path)?;
        repo.remotes()?.into_iter().flatten().any(|r| r == "rad")
    };

    Ok(LocalState { branches, managed })
}

/// Retrieves the [`Commit`] for the given `sha1`.
///
/// # Errors
///
/// Will return [`error::Error`] if the project doesn't exist or the surf interaction fails.
pub fn commit<'repo>(browser: &mut Browser<'repo>, sha1: &str) -> Result<Commit, error::Error> {
    browser.commit(surf::vcs::git::Oid::from_str(sha1)?)?;

    let history = browser.get();
    let commit = history.first();

    Ok(Commit::from(commit))
}

/// Retrieves the [`Commit`] history for the given `branch`.
///
/// # Errors
///
/// Will return [`error::Error`] if the project doesn't exist or the surf interaction fails.
pub fn commits<'repo>(
    browser: &mut Browser<'repo>,
    branch: &str,
) -> Result<Vec<Commit>, error::Error> {
    browser.branch(BranchName::new(branch))?;

    let commits = browser.get().iter().map(Commit::from).collect();

    Ok(commits)
}

/// Retrieves the list of [`Tag`] for the given project `id`.
///
/// # Errors
///
/// Will return [`error::Error`] if the project doesn't exist or the surf interaction fails.
pub fn tags<'repo>(browser: &Browser<'repo>) -> Result<Vec<Tag>, error::Error> {
    let tag_names = browser.list_tags()?;
    let mut tags: Vec<Tag> = tag_names
        .into_iter()
        .map(|tag_name| Tag(tag_name.name().to_string()))
        .collect();

    tags.sort();

    Ok(tags)
}

/// Retrieve the [`Tree`] for the given `revision` and directory `prefix`.
///
/// # Errors
///
/// Will return [`error::Error`] if any of the surf interactions fail.
/// TODO(fintohaps): default branch fall back from Browser
pub fn tree<'repo>(
    browser: &mut Browser<'repo>,
    default_branch: &str, // TODO(finto): This should be handled by the broweser surf#115
    maybe_revision: Option<String>,
    maybe_prefix: Option<String>,
) -> Result<Tree, error::Error> {
    let revision = maybe_revision.unwrap_or_else(|| default_branch.to_string());
    let prefix = maybe_prefix.unwrap_or_default();

    browser.revspec(&revision)?;

    let path = if prefix == "/" || prefix == "" {
        surf::file_system::Path::root()
    } else {
        surf::file_system::Path::from_str(&prefix)?
    };

    let root_dir = browser.get_directory()?;
    let prefix_dir = if path.is_root() {
        root_dir
    } else {
        root_dir
            .find_directory(path.clone())
            .ok_or_else(|| error::Error::PathNotFound(path.clone()))?
    };
    let mut prefix_contents = prefix_dir.list_directory();
    prefix_contents.sort();

    let entries_results: Result<Vec<TreeEntry>, error::Error> = prefix_contents
        .iter()
        .map(|(label, system_type)| {
            let entry_path = if path.is_root() {
                surf::file_system::Path::new(label.clone())
            } else {
                let mut p = path.clone();
                p.push(label.clone());
                p
            };
            let mut commit_path = surf::file_system::Path::root();
            commit_path.append(entry_path.clone());

            let info = Info {
                name: label.to_string(),
                object_type: match system_type {
                    surf::file_system::SystemType::Directory => ObjectType::Tree,
                    surf::file_system::SystemType::File => ObjectType::Blob,
                },
                last_commit: None,
            };

            Ok(TreeEntry {
                info,
                path: entry_path.to_string(),
            })
        })
        .collect();

    let mut entries = entries_results?;

    // We want to ensure that in the response Tree entries come first. `Ord` being derived on
    // the enum ensures Variant declaration order.
    //
    // https://doc.rust-lang.org/std/cmp/trait.Ord.html#derivable
    entries.sort_by(|a, b| a.info.object_type.cmp(&b.info.object_type));

    let last_commit = if path.is_root() {
        Some(Commit::from(browser.get().first()))
    } else {
        None
    };
    let name = if path.is_root() {
        "".into()
    } else {
        let (_first, last) = path.split_last();
        last.to_string()
    };
    let info = Info {
        name,
        object_type: ObjectType::Tree,
        last_commit,
    };

    Ok(Tree {
        path: prefix,
        entries,
        info,
    })
}
