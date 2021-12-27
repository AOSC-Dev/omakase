use super::UserRequest;
use crate::{
    cli,
    db::LocalDb,
    debug,
    executor::{dpkg, modifier, MachineStatus},
    info, pool,
    solver::Solver,
    success,
    types::{
        config::{Blueprints, Config, Opts},
        PkgActionModifier,
    },
    utils::downloader::Downloader,
};

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Confirm;

#[inline]
pub async fn execute(
    local_db: &LocalDb,
    downloader: &Downloader,
    blueprint: &mut Blueprints,
    opts: &Opts,
    config: &Config,
    request: UserRequest,
) -> Result<()> {
    // Check if operating in alt-root mode
    let mut alt_root = false;
    if opts.root != std::path::Path::new("/") {
        info!(
            "Operating in alternative root mode, package will only be unpacked but not configured"
        );
        alt_root = true;
    }

    debug!("Parsing deb dbs...");
    let dbs = local_db
        .get_all_package_db()
        .context("Invalid local package database")?;
    let local_repo = opts.root.join(crate::LOCAL_REPO_PATH);
    if !local_repo.is_dir() {
        std::fs::create_dir_all(&local_repo)?;
    }
    let pool = pool::source::create_pool(&dbs, &[local_repo])?;
    let solver = Solver::from(pool);

    debug!("Processing user request...");
    match request {
        UserRequest::Install(list) => {
            for install in list {
                // Add pkg to blueprint
                blueprint.add(
                    &install.pkgname,
                    install.modify,
                    None,
                    install.ver_req,
                    install.local,
                )?;
                if !install.local && install.install_recomm {
                    let choices = match solver.pool.get_pkgs_by_name(&install.pkgname) {
                        Some(pkgs) => pkgs,
                        None => bail!("Cannot add recommended packages for {}", &install.pkgname),
                    };
                    let choice = choices.get(0).unwrap();
                    let meta = solver.pool.get_pkg_by_id(*choice).unwrap();
                    if let Some(recommends) = &meta.recommends {
                        for recommend in recommends {
                            blueprint.add(
                                &recommend.0,
                                false,
                                Some(&install.pkgname),
                                Some(recommend.1.clone()),
                                false,
                            )?;
                        }
                    }
                }
            }
        }
        UserRequest::Remove(list) => {
            for (name, remove_recomm) in list {
                blueprint.remove(&name, remove_recomm)?;
            }
        }
        UserRequest::Upgrade => (),
    };

    info!("Resolving dependencies...");
    let res = solver.install(blueprint)?;
    // Translating result to list of actions
    let root = &opts.root;
    let machine_status = MachineStatus::new(root)?;
    let mut actions = machine_status.gen_actions(res.as_slice(), config.r#unsafe.purge_on_remove);
    if alt_root {
        let modifier = modifier::UnpackOnly::default();
        modifier.apply(&mut actions);
    }

    if actions.is_empty() {
        success!("There's nothing to do.");
    } else {
        info!("These following actions will be performed:");
        if opts.no_pager {
            actions.show();
        } else {
            actions.show_tables()?;
        }
        crate::WRITER.writeln("", "")?;
        actions.show_size_change();

        // Additional confirmation if removing essential packages
        if actions.remove_essential() {
            if config.r#unsafe.allow_remove_essential {
                let prefix = style("DANGER").red().to_string();
                crate::WRITER.writeln(
                    &prefix,
                    "Some Essential packages will be removed/purged. Are you REALLY sure?",
                )?;
                let confirm_msg =
                    format!("{}{}", cli::gen_prefix(""), "Is this supposed to happen?");
                if !Confirm::new().with_prompt(confirm_msg).interact()? {
                    bail!("User cancelled operation");
                }
            } else {
                bail!("Some essential packages will be removed. Aborting")
            }
        }

        if Confirm::new()
            .with_prompt(format!("{}{}", cli::gen_prefix(""), "Proceed?"))
            .interact()?
        {
            // Run it!
            dpkg::execute_pkg_actions(actions, &opts.root, downloader, config.r#unsafe.unsafe_io)
                .await?;
        } else {
            crate::utils::lock::unlock(&opts.root)?;
            std::process::exit(2);
        }
    }

    Ok(())
}
