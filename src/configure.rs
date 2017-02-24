use serde_json;
use chrono::{Duration, UTC, DateTime};
use std::path::{Path, PathBuf};
use std::fs;
use std::env;
use std::vec::Vec;
use std::io::prelude::*;
use std::collections::BTreeMap;
use errors::{CliError, LalResult};
use super::Container;

// helper
fn lal_dir() -> PathBuf {
    // unwrapping things that really must succeed here
    let home = env::home_dir().unwrap();
    Path::new(&home).join(".lal")
}


/// Docker volume mount representation
#[derive(Serialize, Deserialize, Clone)]
pub struct Mount {
    /// File or folder to mount
    pub src: String,
    /// Location inside the container to mount it at
    pub dest: String,
    /// Whether or not to write protect the mount inside the container
    pub readonly: bool,
}

/// Artifactory credentials
#[derive(Serialize, Deserialize, Clone)]
pub struct Credentials {
    /// Upload username
    pub username: String,
    /// Upload password
    pub password: String,
}

/// Static Artifactory locations
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Artifactory {
    /// Location of artifactory API master (for API queries)
    pub master: String,
    /// Location of artifactory slave (for fetching artifacts)
    pub slave: String,
    /// Release group name (for API queries)
    pub release: String,
    /// Virtual group (for downloads)
    pub vgroup: String,
    /// Optional publish credentials
    pub credentials: Option<Credentials>,
}

/// Representation of `~/.lal/config`
#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    /// Configuration settings for Artifactory
    pub artifactory: Artifactory,
    /// Cache directory for global and stashed builds
    pub cache: String,
    /// Environments shorthands that are allowed and their full meaning
    pub environments: BTreeMap<String, Container>,
    /// Time of last upgrade_check
    pub upgradeCheck: String,
    /// Extra volume mounts to be set for the container
    pub mounts: Vec<Mount>,
    /// Force inteactive shells
    pub interactive: bool,
}

/// Representation of a configuration defaults file
///
/// This file is being used to generate the config when using `lal configure`
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ConfigDefaults {
    /// Configuration settings for Artifactory
    pub artifactory: Artifactory,
    /// Environments shorthands that are allowed and their full meaning
    pub environments: BTreeMap<String, Container>,
    /// Extra volume mounts to be set for the container
    pub mounts: Vec<Mount>,
}

impl ConfigDefaults {
    fn read(file: &str) -> LalResult<ConfigDefaults> {
        let pth = Path::new(file);
        if !pth.exists() {
            error!("No such defaults file '{}'", file); // file open will fail below
        }
        let mut f = fs::File::open(&pth)?;
        let mut data = String::new();
        f.read_to_string(&mut data)?;
        let defaults: ConfigDefaults = serde_json::from_str(&data)?;
        Ok(defaults)
    }
}

impl Config {
    /// Initialize a Config with ConfigDefaults
    ///
    /// This will locate you homedir, and set last update check 2 days in the past.
    /// Thus, with a blank default config, you will always trigger an upgrade check.
    pub fn new(defaults: ConfigDefaults) -> Config {
        let cachepath = lal_dir().join("cache");
        let cachedir = cachepath.as_path().to_str().unwrap();

        // last update time
        let time = UTC::now() - Duration::days(2);

        // scan default mounts
        let mut mounts = vec![];
        for mount in defaults.mounts {
            let mount_path = Path::new(&mount.src);
            // only add mount if the user actually has it locally
            if mount_path.exists() {
                debug!("Configuring existing mount {}", mount.src);
                mounts.push(mount.clone());
            }
        }

        Config {
            cache: cachedir.into(),
            mounts: mounts, // the filtered defaults
            upgradeCheck: time.to_rfc3339(),
            environments: defaults.environments,
            artifactory: defaults.artifactory,
            interactive: true,
        }
    }

    /// Read and deserialize a Config from ~/.lal/config
    pub fn read() -> LalResult<Config> {
        let cfg_path = lal_dir().join("config");
        if !cfg_path.exists() {
            return Err(CliError::MissingConfig);
        }
        let mut f = fs::File::open(&cfg_path)?;
        let mut cfg_str = String::new();
        f.read_to_string(&mut cfg_str)?;
        let res: Config = serde_json::from_str(&cfg_str)?;
        if res.environments.contains_key("default") {
            return Err(CliError::InvalidEnvironment);
        }
        Ok(res)
    }
    /// Checks if it is time to perform an upgrade check
    pub fn upgrade_check_time(&self) -> bool {
        let last = self.upgradeCheck.parse::<DateTime<UTC>>().unwrap();
        let cutoff = UTC::now() - Duration::days(1);
        last < cutoff
    }
    /// Update the upgradeCheck time to avoid triggering it for another day
    pub fn performed_upgrade(&mut self) -> LalResult<()> {
        self.upgradeCheck = UTC::now().to_rfc3339();
        Ok(self.write(true)?)
    }
    /// Overwrite `~/.lal/config` with serialized data from this struct
    pub fn write(&self, silent: bool) -> LalResult<()> {
        let cfg_path = lal_dir().join("config");

        let encoded = serde_json::to_string_pretty(self)?;

        let mut f = fs::File::create(&cfg_path)?;
        write!(f, "{}\n", encoded)?;
        if silent {
            debug!("Wrote config {}: \n{}", cfg_path.display(), encoded);
        } else {
            info!("Wrote config {}: \n{}", cfg_path.display(), encoded);
        }
        Ok(())
    }

    /// Resolve an arbitrary container shorthand
    pub fn get_container(&self, env: String) -> LalResult<Container> {
        if let Some(container) = self.environments.get(&env) {
            return Ok(container.clone());
        }
        Err(CliError::MissingEnvironment(env))
    }
}

/// Helper to print the configured environments from the config
pub fn env_list(cfg: &Config) -> LalResult<()> {
    for k in cfg.environments.keys() {
        println!("{}", k);
    }
    Ok(())
}

fn create_lal_dir() -> LalResult<PathBuf> {
    let home = env::home_dir().unwrap();
    let laldir = Path::new(&home).join(".lal");
    if !laldir.is_dir() {
        fs::create_dir(&laldir)?;
    }
    Ok(laldir)
}

/// Create  `~/.lal/config` with defaults
///
/// A boolean option to discard the output is supplied for tests.
/// A defaults file must be supplied to seed the new config with defined environments
pub fn configure(save: bool, interactive: bool, defaults: &str) -> LalResult<Config> {
    let _ = create_lal_dir()?;

    let mut cfg = Config::new(ConfigDefaults::read(defaults)?);
    cfg.interactive = interactive; // need to override default for tests
    if save {
        cfg.write(false)?;
    }
    Ok(cfg)
}
