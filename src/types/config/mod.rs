mod blueprint;
mod ignorerules;
pub use blueprint::{Blueprints, PkgRequest};
pub use ignorerules::IgnoreRules;

use anyhow::{bail, Result};
use clap::Parser;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub arch: String,
    pub purge_on_remove: bool,
    pub repo: HashMap<String, RepoConfig>,
}

impl Config {
    pub fn check_sanity(&self) -> Result<()> {
        lazy_static! {
            static ref KEY_FILENAME: Regex = Regex::new("^[a-zA-Z0-9.]+$").unwrap();
        }

        for (name, repo) in &self.repo {
            for key_filename in &repo.keys {
                if !KEY_FILENAME.is_match(key_filename) {
                    bail!(
                        "Invalid character in public key name {} for repo {}",
                        name,
                        key_filename
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RepoConfig {
    pub url: String,
    pub distribution: String,
    pub components: Vec<String>,
    pub keys: Vec<String>,
}

#[derive(Parser)]
#[clap(about, version, author)]
pub struct Opts {
    #[clap(long, default_value = "/", help = "Root directory for operation")]
    pub root: PathBuf,
    #[clap(
        long,
        default_value = "etc/omakase/",
        help = "Position of the config folder"
    )]
    pub config_root: PathBuf,
    #[clap(short, long, help = "Print additional debug information")]
    pub verbose: bool,
    #[clap(long, help = "Unpack but not configure desired packages")]
    pub unpack_only: bool,
    #[clap(subcommand)]
    pub subcmd: SubCmd,
}

#[derive(Parser)]
pub enum SubCmd {
    #[clap(about = "Install new packages")]
    Install(InstallPkg),
    #[clap(about = "Remove packages")]
    Remove(RemovePkg),
    #[clap(about = "Refresh local package databases")]
    Refresh,
    #[clap(about = "Install and upgrade all packages according to Blueprint")]
    Execute,
    #[clap(about = "Alias to Execute")]
    Upgrade,
    #[clap(about = "Search packages from package database")]
    Search(SearchPkg),
    #[clap(about = "Search what packages provide a certain file")]
    Provide(ProvideFile),
    #[clap(about = "Delete local database and package cache")]
    Clean(CleanConfig),
}

#[derive(Parser)]
pub struct InstallPkg {
    pub names: Vec<String>,
    #[clap(long, help = "Don't install recommended packages")]
    pub no_recommends: bool,
}

#[derive(Parser)]
pub struct RemovePkg {
    pub names: Vec<String>,
    #[clap(long, help = "Also remove recommended packages")]
    pub remove_recommends: bool,
}

#[derive(Parser)]
pub struct SearchPkg {
    pub keyword: String,
}

#[derive(Parser)]
pub struct ProvideFile {
    pub file: String,
}

#[derive(Parser)]
pub struct CleanConfig {
    #[clap(short, long, help = "Remove both package cache and local database")]
    pub all: bool,
}
