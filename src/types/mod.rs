mod actions;
mod checksum;
pub mod config;
mod version;

pub use actions::{PkgActionModifier, PkgActions, PkgInstallAction};
pub use checksum::{Checksum, ChecksumValidator};
pub use version::{parse_version, parse_version_requirement, PkgVersion, VersionRequirement};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Deserialize, Default)]
pub struct PkgRequirement {
    pub with_recommends: Option<bool>,
    pub version: Option<VersionRequirement>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PkgMeta {
    pub name: String,
    pub section: String,
    pub description: String,
    pub version: PkgVersion,
    pub depends: Vec<(String, VersionRequirement)>,
    pub breaks: Vec<(String, VersionRequirement)>,
    pub conflicts: Vec<(String, VersionRequirement)>,
    pub recommends: Option<Vec<(String, VersionRequirement)>>,
    pub suggests: Option<Vec<(String, VersionRequirement)>>,
    pub provides: Option<Vec<(String, VersionRequirement)>>,
    pub replaces: Option<Vec<(String, VersionRequirement)>>,
    pub install_size: u64,

    pub essential: bool,
    pub source: PkgSource,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PkgSource {
    // Http((url, size, checksum))
    Http((String, u64, Checksum)),
    // Local(path)
    Local(PathBuf),
}
