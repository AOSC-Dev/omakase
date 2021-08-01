mod action;
mod checksum;
mod version;

pub use action::PkgActions;
pub use checksum::Checksum;
pub use version::{PkgVersion, VersionRequirement};

use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct PkgRequirement {
    pub with_recommends: Option<bool>,
    pub version: Option<VersionRequirement>,
}

#[derive(Clone, Debug)]
pub struct PkgMeta {
    pub name: String,
    pub version: PkgVersion,
    pub depends: Vec<(String, VersionRequirement)>,
    pub breaks: Vec<(String, VersionRequirement)>,
    pub conflicts: Vec<(String, VersionRequirement)>,
    pub install_size: usize,
    pub url: String,
    // u64 because reqwest's content length is u64
    pub size: u64,
    pub checksum: Checksum,
}