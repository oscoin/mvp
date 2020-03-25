//! Combine the domain `CoCo` and Registry domain specific understanding of a Project into a single
//! abstraction.

use librad::meta;
use librad::project;
use radicle_registry_client as registry;
use std::str::FromStr;

use crate::coco;
use crate::error;

/// Metadata key used to store an image url for a project.
const IMG_URL_LABEL: &str = "img_url";

/// Object the API returns for project metadata.
#[derive(serde_derive::Deserialize, serde_derive::Serialize)]
pub struct Metadata {
    /// Project name.
    pub name: String,
    /// High-level description of the project.
    pub description: String,
    /// Default branch for checkouts, often used as mainline as well.
    pub default_branch: String,
    /// Image url for the project.
    pub img_url: String,
}

impl From<meta::Project> for Metadata {
    fn from(project_meta: meta::Project) -> Self {
        let img_url = project_meta
            .rel
            .into_iter()
            .filter_map(|r| {
                if let meta::Relation::Url(label, url) = r {
                    Some((label, url))
                } else {
                    None
                }
            })
            .find_map(|(label, url)| {
                if *label == *IMG_URL_LABEL {
                    Some(url.to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "".to_string());

        Self {
            name: project_meta.name.unwrap_or_else(|| "name unknown".into()),
            description: project_meta.description.unwrap_or_else(|| "".into()),
            default_branch: project_meta.default_branch,
            img_url,
        }
    }
}

/// Radicle project for sharing and collaborating.
pub struct Project {
    /// Unique identifier of the project in the network.
    pub id: project::ProjectId,
    /// Attached metadata, mostly for human pleasure.
    pub metadata: Metadata,
    /// Informs if the project is present in the Registry and under what top-level entity it can be
    /// found.
    pub registration: Option<Registration>,
}

/// Variants for possible registration states of a project.
// TODO(xla): Remove once properly integrated.
#[allow(dead_code)]
pub enum Registration {
    /// Project is registered under an Org.
    Org(registry::OrgId),
    /// Project is registered under a User.
    User(registry::UserId),
}

/// Coarse statistics for the Project source code.
pub struct Stats {
    /// Amount of known branches.
    pub branches: u32,
    /// Number of commits on the default branch.
    pub commits: u32,
    /// Amount of unique commiters on the default branch.
    pub contributors: u32,
}

/// TODO(xla): Add documentation.
pub async fn get(paths: &librad::paths::Paths, id: &str) -> Result<Project, error::Error> {
    let meta = coco::get_project_meta(paths, id)?;

    Ok(Project {
        id: librad::project::ProjectId::from_str(id)?,
        metadata: meta.into(),
        registration: None,
    })
}
