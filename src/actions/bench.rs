use crate::{
    db::LocalDb,
    info, msg, success,
    types::{
        config::{Config, Mirror, Opts},
        Checksum, ChecksumValidator,
    },
    utils::{downloader::Downloader, pager::Pager},
};

use anyhow::{bail, Result};
use console::style;
use indicatif::HumanBytes;
use reqwest::{Client, ClientBuilder};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};
use tabled::{Alignment, Column, Full, Head, Header, Modify, Style, Table, Tabled};
use toml_edit::{value, Document};

pub async fn bench(
    opts: &Opts,
    config: &Config,
    db: LocalDb,
    downloader: &Downloader,
) -> Result<()> {
    // First, update local db
    db.update(downloader).await?;

    info!("Starting benchmarks...");
    // Set reqwest parameters
    let clientbuilder = ClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30));
    let client = clientbuilder.build()?;

    let mut config = config.clone();
    let mut results = Vec::new();
    for (name, repo) in &mut config.repo {
        let (urls, _) = match &repo.source {
            Mirror::Simple(_) => {
                msg!(
                    "Skipping repository {} because it only has one mirror.",
                    style(name).bold()
                );
                continue;
            }
            Mirror::MirrorList {
                preferred: _,
                mirrorlist: _,
            } => repo.get_mirrors()?,
        };

        msg!("Running benchmark for repository {}...", style(name).bold());
        let mut res = Vec::new();
        // Fetch Contents-all.gz for specified repo
        let contents_filename = format!(
            "Contents_{}_{}_{}.gz",
            repo.distribution, repo.components[0], config.arch
        );
        // Get ChecksumValidator for this file
        let local = db.get_contents_db(name)?;
        let local_paths: Vec<PathBuf> = local
            .into_iter()
            .filter(|(_, path)| path.ends_with(&contents_filename))
            .map(|(_, path)| path)
            .collect();
        if local_paths.is_empty() {
            bail!(
                "Internal Error: Local repository don't have benchmark file {}",
                contents_filename
            );
        }
        let local_path = &local_paths[0];
        let size = fs::metadata(&local_path)?.len();
        let local_hash = Checksum::from_file_sha256(local_path)?;
        let validator = local_hash.get_validator();

        for (name, mirror) in urls {
            let contents_url = format!(
                "{}/dists/{}/{}/Contents-{}.gz",
                mirror.url, repo.distribution, repo.components[0], config.arch
            );
            // Start counting
            let start = Instant::now();
            match try_download(&contents_url, &client, validator.clone()).await {
                Ok(_) => {
                    let time = start.elapsed();
                    res.push((name.clone(), mirror.url.clone(), Some(time)));
                }
                Err(e) => {
                    msg!("Mirror {name} failed to complete benchmark: {e}");
                    res.push((name.clone(), mirror.url.clone(), None));
                }
            }
        }
        // Sort result based on time
        res.sort_by_key(|(_, _, time)| time.unwrap_or(Duration::MAX));
        // Push result of this repo to results
        results.push((name.as_str(), size, res));
    }

    // Show results
    show_bench_results(&results, opts.no_pager)?;

    // Ask if to write back results
    if crate::cli::ask_confirm(opts, "Apply optimal mirrors based on benchmark result?")? {
        let config_path = opts
            .root
            .join(&opts.config_root)
            .canonicalize()
            .unwrap()
            .join("config.toml");
        let original_toml = fs::read_to_string(&config_path)?;
        let mut new_config = original_toml.parse::<Document>()?;
        for (repo_name, _, result) in &results {
            let new_preferred = &result[0].0;
            new_config["repo"][repo_name]["source"]["preferred"] = value(new_preferred);
        }
        let new_config = new_config.to_string();
        std::fs::write(config_path, new_config)?;
        success!(
            "New repository configuration has been written to {}.",
            style("config.toml").bold()
        );
    }

    Ok(())
}

#[inline]
async fn try_download(url: &str, client: &Client, mut validator: ChecksumValidator) -> Result<()> {
    let mut resp = client.get(url).send().await?;
    while let Some(chunk) = resp.chunk().await? {
        validator.update(&chunk);
    }

    if !validator.finish() {
        bail!("Checksum mismatched.");
    }

    Ok(())
}

#[derive(Tabled)]
struct BenchResultRow {
    #[header("Best")]
    best: String,
    #[header("Mirror Name")]
    name: String,
    #[header("URL")]
    url: String,
    #[header("Speed")]
    speed: String,
}

#[inline]
fn show_bench_results(
    results: &[(&str, u64, Vec<(String, String, Option<Duration>)>)],
    no_pager: bool,
) -> Result<()> {
    info!("Benchmark result:");

    let mut pager = Pager::new(no_pager)?;
    let pager_name = pager.pager_name().to_owned();
    let mut writer = pager.get_writer()?;

    if pager_name == Some("less") {
        writeln!(
            writer,
            "Press {} to finish reviewing benchmark result.",
            style("q").bold()
        )?;
        writeln!(writer)?;
    }

    for (name, size, repo_results) in results {
        let mut rows = Vec::new();
        for (i, (name, url, time)) in repo_results.iter().enumerate() {
            let speed = if let Some(duration) = time {
                let ms = duration.as_millis();
                // *1024 because ms to s
                let bytes_per_sec: u128 = *size as u128 / ms * 1024;
                format!("{}/s", HumanBytes(bytes_per_sec as u64))
            } else {
                style("FAILED").red().bold().to_string()
            };
            let best = if i == 0 {
                style("*").green().bold().to_string()
            } else {
                String::new()
            };
            let row = BenchResultRow {
                best,
                name: name.clone(),
                url: url.clone(),
                speed,
            };
            rows.push(row);
        }
        let table = Table::new(&rows)
            .with(Header(format!(
                "Benchmark Result for {}",
                style(name).bold()
            )))
            .with(Modify::new(Full).with(Alignment::left()))
            .with(Modify::new(Head).with(Alignment::center_horizontal()))
            // Best column should be aligned to the center
            .with(Modify::new(Column(0..1)).with(Alignment::center_horizontal()))
            .with(Modify::new(Column(1..)).with(|s: &str| format!(" {} ", s)))
            .with(Style::PSEUDO_CLEAN);
        writeln!(writer, "{table}\n")?;
    }

    drop(writer);
    pager.wait_for_exit()?;
    msg!("");

    Ok(())
}
