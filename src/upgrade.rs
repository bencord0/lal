use std::fs;
use std::path::Path;
use std::process::Command;

use semver::Version;
use util::artifactory::find_latest_lal_version;
use install::download_to_path;
use {LalResult, Config, CliError};

/// Check for updated versions of the lal binary on artifactory
pub fn upgrade(cfg: Config, prefix: &str, dry_run: bool, silent: bool) -> LalResult<()> {
    let latest = try!(find_latest_lal_version());
    let current = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    if latest > current {
        // New version found - always full output now
        info!("A new version of lal is available: {}", latest);
        if cfg!(target_family = "musl") {
            if dry_run {
                // Don't want to silently upgrade from the silent upgrade check
                info!("Your version is compiled with musl");
                info!("Type `lal upgrade` to upgrade");
            }
            else {
                // We are using musl binaries - just install into prefix
                info!("Installing to {}", prefix);
                let uri = format!("{}/lal/{}/lal", cfg.artifactory, latest);
                let bindir = Path::new(prefix).join("bin");
                if !bindir.is_dir() {
                    try!(fs::create_dir(&bindir));
                }
                let dest = bindir.join("lal");
                try!(download_to_path(&uri, dest.to_str().unwrap()));
                // make executable
                // NB: in the futere we can do: try!(fs::chmod(dest, io::UserExec));
                let s = try!(Command::new("chmod").arg("+x").arg(dest).status());
                if !s.success() {
                    return Err(CliError::SubprocessFailure(s.code().unwrap_or(1001)));
                }
                info!("lal {} successfully installed", latest);
                info!("Run `which lal` to ensure it comes from {}", prefix);
            }
        }
        else {
            // Source install - just tell the user what to do:
            info!("Your version is compiled from source");
            info!("Please `git pull` and `cargo build --release` in the source checkout");
        }
    } else {
        // No new version
        if silent {
            debug!("You are running the latest version of lal");
        } else {
            info!("You are running the latest version of lal");
        }
    }
    Ok(())
}
