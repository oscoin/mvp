use std::{
    ffi,
    path::{self, PathBuf},
};

pub use librad::meta::project::Project;
use librad::{
    git::{include, local::url::LocalUrl, types::remote::Remote},
    peer::PeerId,
};
use radicle_surf::vcs::git::git2;

/// When checking out a working copy, we can run into several I/O failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Git error when checking out the project.
    #[error(transparent)]
    Git(#[from] git2::Error),

    /// An error occured building include files.
    #[error(transparent)]
    Include(#[from] include::Error),
}

/// The data necessary for checking out a project.
pub struct Checkout<P, ST>
where
    P: AsRef<path::Path>,
{
    /// The project.
    project: Project<ST>,
    /// The path on the filesystem where we're going to checkout to.
    path: P,
    /// Absolute path of the include file that will be set in the working copy config.
    include_path: PathBuf,
}

impl<P, ST> Checkout<P, ST>
where
    P: AsRef<path::Path>,
    ST: Clone,
{
    /// Create a new `Checkout` with the mock `Credential::Password` helper.
    pub fn new(project: Project<ST>, path: P, include_path: PathBuf) -> Self {
        Self {
            project,
            path,
            include_path,
        }
    }

    /// Checkout a working copy of a [`Project`].
    ///
    /// # Errors
    ///
    ///   * The checkout process failed.
    pub fn run(self, peer_id: PeerId) -> Result<PathBuf, Error> {
        // Check if the path provided ends in the 'directory_name' provided. If not we create the
        // full path to that name.
        let path = &self.path.as_ref();
        let project_path = if let Some(destination) = path.components().next_back() {
            let destination: &ffi::OsStr = destination.as_ref();
            let project_name = self.project.name().to_string();
            let name: &ffi::OsStr = project_name.as_ref();
            if destination == name {
                path.to_path_buf()
            } else {
                path.join(name)
            }
        } else {
            path.join(&self.project.name().to_string())
        };

        let mut builder = git2::build::RepoBuilder::new();
        builder.branch(self.project.default_branch());
        builder.remote_create(|repo, _, url| {
            let remote = Remote::rad_remote(url, None).create(repo)?;
            Ok(remote)
        });
        let repo = git2::build::RepoBuilder::clone(
            &mut builder,
            &LocalUrl::from_urn(self.project.urn(), peer_id).to_string(),
            &project_path,
        )?;

        super::set_rad_upstream(&repo, self.project.default_branch())?;

        include::set_include_path(&repo, self.include_path)?;

        Ok(project_path)
    }
}
