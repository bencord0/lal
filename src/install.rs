use std::io::{self, Error, ErrorKind};
use std::fs;
use std::path::Path;

// use util::globalroot::get_tarball_uri;
use util::artifactory::get_tarball_uri;
use Manifest;
use configure::Config;
use errors::{CliError, LalResult};

pub struct Component {
    pub name: String,
    pub version: u32,
    pub tarball: String,
}

pub fn download_to_path(uri: &str, save: &str) -> io::Result<()> {
    use curl::http;
    use std::io::prelude::*;

    debug!("GET {}", uri);
    // unwrapping has seen this error:
    // "Problem with the SSL CA cert (path? access rights?)""
    let resp = match http::handle().get(uri).exec() {
        Ok(r) => r,
        Err(e) => return Err(Error::new(ErrorKind::Other, format!("Failed to download file {}", e))),
    };

    if resp.get_code() == 200 {
        let r = resp.get_body();
        let path = Path::new(save);
        let mut f = try!(fs::File::create(&path));
        try!(f.write_all(r));
        Ok(())
    } else {
        Err(Error::new(ErrorKind::Other, format!("Failed to download file {}", uri)))
    }
}

fn fetch_component(cfg: Config, name: &str, version: Option<u32>) -> LalResult<Component> {
    use tar::Archive;
    use flate2::read::GzDecoder;
    use cache;

    trace!("Fetch component {}", name);
    let component = try!(get_tarball_uri(name, version));
    let tarname = ["./", name, ".tar"].concat();

    // always just download for now - TODO: eventually check cache
    try!(download_to_path(&component.tarball, &tarname));

    debug!("Unpacking tarball {}", tarname);
    let data = try!(fs::File::open(&tarname));
    let decompressed = try!(GzDecoder::new(data)); // decoder reads data
    let mut archive = Archive::new(decompressed); // Archive reads decoded

    let extract_path = Path::new("./INPUT").join(&name);
    try!(fs::create_dir_all(&extract_path));
    try!(archive.unpack(&extract_path));

    // Move tarball into cfg.cache
    try!(cache::store_tarball(&cfg, name, component.version));
    Ok(component)
}

fn clean_input() {
    let input = Path::new("./INPUT");
    if input.is_dir() {
        let _ = fs::remove_dir_all(&input).unwrap();
    }
}

/// Install specific dependencies outside the manifest
///
/// Multiple "components=version" strings can be supplied, where the version is optional.
/// If no version is supplied, latest is fetched.
///
/// If installation was successful, the fetched tarballs are unpacked into `./INPUT`.
/// If one `save` or `savedev` was set, the fetched versions are also updated in the
/// manifest. This provides an easy way to not have to deal with strict JSON manually.
pub fn install(manifest: Manifest,
               cfg: Config,
               components: Vec<&str>,
               save: bool,
               savedev: bool)
               -> LalResult<()> {
    debug!("Install specific deps: {:?}", components);

    let mut error = None;
    let mut installed = Vec::with_capacity(components.len());
    for comp in &components {
        info!("Fetch {}", comp);
        if comp.contains("=") {
            let pair: Vec<&str> = comp.split("=").collect();
            if let Ok(n) = pair[1].parse::<u32>() {
                match fetch_component(cfg.clone(), pair[0], Some(n)) {
                    Ok(c) => {
                        installed.push(c);
                    }
                    Err(e) => {
                        warn!("Failed to install {} ({})", pair[0], e);
                        error = Some(e);
                    }
                }
            } else {
                // TODO: this should try to install from stash!
                warn!("Failed to install {} labelled {} build from stash",
                      pair[1],
                      pair[0]);
                error = Some(CliError::InstallFailure);
            }
        } else {
            match fetch_component(cfg.clone(), &comp, None) {
                Ok(c) => installed.push(c),
                Err(e) => {
                    warn!("Failed to install {} ({})", &comp, e);
                    error = Some(e);
                }
            }
        }
    }
    if error.is_some() {
        return Err(error.unwrap());
    }

    // Update manifest if saving in any way
    if save || savedev {
        let mut mf = manifest.clone();
        // find reference to correct list
        let mut hmap = if save { mf.dependencies.clone() } else { mf.devDependencies.clone() };
        for c in &installed {
            debug!("Successfully installed {} at version {}",
                   &c.name,
                   c.version);
            if hmap.contains_key(&c.name) {
                *hmap.get_mut(&c.name).unwrap() = c.version;
            } else {
                hmap.insert(c.name.clone(), c.version);
            }
        }
        if save {
            mf.dependencies = hmap;
        } else {
            mf.devDependencies = hmap;
        }
        try!(mf.write());
    }
    Ok(())
}

/// Remove specific components from `./INPUT` and the manifest.
///
/// This takes multiple components strings (without versions), and if the component
/// is found in `./INPUT` it is deleted.
///
/// If one of `save` or `savedev` was set, `manifest.json` is also updated to remove
/// the specified components from the corresponding dictionary.
pub fn uninstall(manifest: Manifest, xs: Vec<&str>, save: bool, savedev: bool) -> LalResult<()> {
    debug!("Removing dependencies {:?}", xs);

    // remove entries in xs from manifest.
    if save || savedev {
        let mut mf = manifest.clone();
        let mut hmap = if save { mf.dependencies.clone() } else { mf.devDependencies.clone() };
        for component in xs.clone() {
            // We could perhaps allow people to just specify ANY dependency
            // and have a generic save flag, which we could infer from
            // thus we could modify both maps if listing many components

            // This could work, but it's not currently what install does, so not doing it.
            // => all components uninstalled from either dependencies, or all from devDependencies
            // if doing multiple components from different maps, do multiple calls
            if !hmap.contains_key(component) {
                return Err(CliError::MissingComponent(component.to_string()));
            }
            debug!("Removing {} from manifest", component);
            hmap.remove(component);
        }
        if save {
            mf.dependencies = hmap;
        } else {
            mf.devDependencies = hmap;
        }
        info!("Updating manifest with removed dependencies");
        try!(mf.write());
    }

    // delete the folder (ignore if the folder does not exist)
    let input = Path::new("./INPUT");
    if !input.is_dir() {
        return Ok(());
    }
    for component in xs {
        let pth = Path::new(&input).join(component);
        if pth.is_dir() {
            debug!("Deleting INPUT/{}", component);
            try!(fs::remove_dir_all(&pth));
        }
    }
    Ok(())
}

/// Install all dependencies from `manifest.json`
///
/// This will read, and HTTP GET all the `dependencies` at the specified versions.
/// If the `dev` bool is set, then `devDependencies` are also installed.
pub fn install_all(manifest: Manifest, cfg: Config, dev: bool) -> LalResult<()> {
    debug!("Installing dependencies{}",
           if dev { " and devDependencies" } else { "" });
    clean_input();

    // create the joined hashmap of dependencies and possibly devdependencies
    let mut deps = manifest.dependencies.clone();
    if dev {
        for (k, v) in &manifest.devDependencies {
            deps.insert(k.clone(), v.clone());
        }
    }
    let mut err = None;
    for (k, v) in deps {
        info!("Fetch {} {}", k, v);
        let _ = fetch_component(cfg.clone(), &k, Some(v)).map_err(|e| {
            warn!("Failed to completely install {} ({})", k, e);
            // likely symlinks inside tarball that are being dodgy
            // this is why we clean_input
            err = Some(e);
        });
    }

    if err.is_some() {
        return Err(CliError::InstallFailure);
    }
    Ok(())
}
