/// Utilities to deal with deb package db
use crate::{
    pool::PkgPool,
    types::{Checksum, PkgMeta, PkgSource, PkgVersion},
    utils::debcontrol::parse_pkg_list,
    warn,
};
use anyhow::{bail, format_err, Result};
use debcontrol::{BufParse, Streaming};
use rayon::prelude::*;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::path::Path;

const INTERESTED_FIELDS: &[&str] = &[
    "Package",
    "Filename",
    "Section",
    "Version",
    "Depends",
    "Breaks",
    "Conflicts",
    "Installed-Size",
    "Size",
    "SHA256",
    "SHA512",
    "Recommends",
    "Suggests",
    "Description",
];

#[inline]
pub fn import(db: &Path, pool: &mut dyn PkgPool, baseurl: &str) -> Result<()> {
    let f = File::open(db)?;
    let mut buf_parse = BufParse::new(f, 16384);
    let mut pkgs = Vec::new();

    while let Some(result) = buf_parse.try_next().unwrap() {
        match result {
            Streaming::Item(paragraph) => {
                let mut fields = HashMap::new();
                for field in paragraph.fields {
                    if INTERESTED_FIELDS.contains(&field.name) {
                        fields.insert(field.name.to_string(), field.value);
                    }
                }
                pkgs.push(fields);
            }
            Streaming::Incomplete => buf_parse.buffer().unwrap(),
        }
    }

    // Parse fields in parallel
    let pkgmetas: Vec<PkgMeta> = pkgs
        .into_par_iter()
        .filter_map(|fields| match fields_to_packagemeta(fields, baseurl) {
            Ok(res) => Some(res),
            Err(e) => {
                warn!("Invalid entry in package database: {}", e);
                None
            }
        })
        .collect();
    // Import results into pool
    for pkgmeta in pkgmetas {
        pool.add(pkgmeta);
    }

    Ok(())
}

#[inline]
fn fields_to_packagemeta(mut f: HashMap<String, String>, baseurl: &str) -> Result<PkgMeta> {
    // Get name first, for error reporting
    let name = f
        .remove("Package")
        .ok_or_else(|| format_err!("Package doesn't have name"))?;
    // Generate real url
    let mut path = baseurl.to_string();
    path.push('/');
    path.push_str(
        f.get("Filename")
            .ok_or_else(|| format_err!("Package {} doesn't have field filename", name))?,
    );
    Ok(PkgMeta {
        name: name.clone(),
        section: f
            .remove("Section")
            .ok_or_else(|| format_err!("Package {} doesn't have field Section", name))?,
        description: f
            .remove("Description")
            .ok_or_else(|| format_err!("Package {} doesn't have field Description", name))?,
        version: PkgVersion::try_from(
            f.get("Version")
                .ok_or_else(|| format_err!("Package {} doesn't have field Version", name))?
                .as_str(),
        )?,
        depends: parse_pkg_list(f.get("Depends").unwrap_or(&String::new()))?,
        breaks: parse_pkg_list(f.get("Breaks").unwrap_or(&String::new()))?,
        conflicts: parse_pkg_list(f.get("Conflicts").unwrap_or(&String::new()))?,
        install_size: f
            .remove("Installed-Size")
            .ok_or_else(|| format_err!("Package {} doesn't have field Installed-Size", name))?
            .as_str()
            .parse()?,
        recommends: match f.get("Recommends") {
            Some(recomm) => Some(parse_pkg_list(recomm)?),
            None => None,
        },
        suggests: match f.get("Suggests") {
            Some(suggests) => Some(parse_pkg_list(suggests)?),
            None => None,
        },
        source: PkgSource::Http((
            path,
            f.remove("Size")
                .ok_or_else(|| format_err!("Package {} doesn't have field Size", name))?
                .as_str()
                .parse()?,
            {
                if let Some(hex) = f.get("SHA256") {
                    Checksum::from_sha256_str(hex)?
                } else if let Some(hex) = f.get("SHA512") {
                    Checksum::from_sha512_str(hex)?
                } else {
                    bail!(
                        "Package {} doesn't have field checksum (SHA256 or SHA512)",
                        name
                    )
                }
            },
        )),
    })
}
