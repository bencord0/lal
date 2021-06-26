#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    vec::Vec,
};
use tokio::{fs::File as AsyncFile, io::AsyncWriteExt as _};

#[cfg(feature = "upgrade")] use semver::Version;

use hyper::{body::HttpBody, Body, Client, Method, Request, StatusCode};

use crate::core::{CliError, LalResult};

/// Artifactory credentials
#[derive(Serialize, Deserialize, Clone)]
pub struct Credentials {
    /// Upload username
    pub username: String,
    /// Upload password
    pub password: String,
}

// Impl Debug without exposing `password`
impl fmt::Debug for Credentials {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Credentals")
            .field("username", &self.username)
            .field("password", &format!("*** ({} characters)", self.password.len()))
            .finish()
    }
}

/// Static Artifactory locations
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ArtifactoryConfig {
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

// Need these to query for stored artifacts:
// This query has tons of info, but we only care about the version
// And the version is encoded in children.uri with leading slash
#[derive(Deserialize)]
struct ArtifactoryVersion {
    uri: String, // folder: bool,
}
#[derive(Deserialize)]
struct ArtifactoryStorageResponse {
    children: Vec<ArtifactoryVersion>,
}

// simple request body fetcher
async fn hyper_req(url: &str) -> LalResult<String> {
    Ok(reqwest::get(url).await.unwrap().text().await.unwrap().to_string())
}

// simple request downloader
pub async fn http_download_to_path(url: &str, save: &Path) -> LalResult<()> {
    debug!("GET {}", url);
    let client = Client::new();
    let mut res = client.get(url.parse().unwrap()).await?;
    if !res.status().is_success() {
        return Err(CliError::BackendFailure(format!(
            "GET request with {}",
            res.status()
        )));
    }

    if cfg!(feature = "progress") {
        #[cfg(feature = "progress")]
        {
            use indicatif::{ProgressBar, ProgressStyle};
            let total_size: u64 = res
                .headers()
                .get("content-length")
                .unwrap()
                .to_str()
                .unwrap()
                .parse()?;
            let mut downloaded = 0u64;
            let pb = ProgressBar::new(total_size);

            pb.set_style(
                ProgressStyle::default_bar().template("{bar:40.yellow/black} {bytes}/{total_bytes} ({eta})"),
            );

            let mut f = AsyncFile::create(save).await?;
            while let Some(chunk) = res.body_mut().data().await {
                let chunk = chunk?;
                downloaded += chunk.len() as u64;
                pb.set_position(downloaded);

                f.write_all(&chunk).await?;
            }

            f.flush().await?;
        }
    } else {
        let mut f = AsyncFile::create(save).await?;

        while let Some(chunk) = res.body_mut().data().await {
            f.write_all(&chunk?).await?;
        }

        f.flush().await?;
    }
    Ok(())
}

/// Query the Artifactory storage api
///
/// This will get, then parse all results as u32s, and return this list.
/// This assumes versoning is done via a single integer.
async fn get_storage_versions(uri: &str) -> LalResult<Vec<u32>> {
    debug!("GET {}", uri);

    let resp: String = hyper_req(uri).await.map_err(|e| {
        warn!("Failed to GET {}: {}", uri, e);
        CliError::BackendFailure("No version information found on API".into())
    })?;

    trace!("{}", format!("Got body {}", resp));

    let res: ArtifactoryStorageResponse = serde_json::from_str(&resp)?;
    let mut builds: Vec<u32> = res
        .children
        .iter()
        .map(|r| r.uri.as_str())
        .map(|r| r.trim_matches('/'))
        .filter_map(|b| b.parse().ok())
        .collect();
    builds.sort_by(|a, b| b.cmp(a)); // sort by version number descending
    Ok(builds)
}

/// Upload a tarball to artifactory
///
/// This is using a http basic auth PUT to artifactory using config credentials.
async fn upload_artifact(arti: &ArtifactoryConfig, uri: &str, f: &mut File) -> LalResult<()> {
    if let Some(creds) = arti.credentials.clone() {
        let client = Client::new();

        let mut buffer: Vec<u8> = Vec::new();
        f.read_to_end(&mut buffer)?;

        let full_uri = format!("{}/{}/{}", arti.slave, arti.release, uri);

        let mut sha = sha1::Sha1::new();
        sha.update(&buffer);

        let auth = format!("{}:{}", creds.username, creds.password);
        let auth = format!("Basic {}", base64::encode(auth));

        // upload the artifact
        info!("PUT {}", full_uri);
        let request: Request<_> = Request::builder()
            .method(Method::PUT)
            .uri(&full_uri)
            .header("Authorization", auth.clone())
            .body(Body::from(buffer))
            .unwrap();

        let resp = client.request(request).await?;

        debug!("resp={:?}", resp);
        let respstr = format!("{} from PUT {}", resp.status(), full_uri);
        if resp.status() != StatusCode::CREATED {
            return Err(CliError::UploadFailure(respstr));
        }
        debug!("{}", respstr);

        // do another request to get the hash on artifactory
        // jfrog api does not allow do do both at once - and this also creates the md5 (somehow)
        // this creates ${full_uri}.sha1 and ${full_uri}.md5 (although we just gave it the sha..)
        // This `respsha` can fail if engci-maven becomes inconsistent. NotFound has been seen.
        // And that makes no sense because the above must have returned Created to get here..
        info!("PUT {} (X-Checksum-Sha1)", full_uri);
        let request: Request<_> = Request::builder()
            .method(Method::PUT)
            .uri(&full_uri[..])
            .header("X-Checksum-Deploy", "true")
            .header("X-Checksum-Sha1", sha.digest().to_string())
            .header("Authorization", auth)
            .body(Body::empty())
            .unwrap();

        let respsha = client.request(request).await?;
        debug!("respsha={:?}", respsha);
        let respshastr = format!("{} from PUT {} (X-Checksum-Sha1)", respsha.status(), full_uri);
        if respsha.status() != StatusCode::CREATED {
            return Err(CliError::UploadFailure(respshastr));
        }
        debug!("{}", respshastr);

        Ok(())
    } else {
        Err(CliError::MissingBackendCredentials)
    }
}

/// Get the maximal version number from the storage api
async fn get_storage_as_u32(uri: &str) -> LalResult<u32> {
    if let Some(&latest) = get_storage_versions(uri).await?.iter().max() {
        Ok(latest)
    } else {
        Err(CliError::BackendFailure(
            "No version information found on API".into(),
        ))
    }
}

// The URL for a component tarball under the one of the environment trees
fn get_dependency_env_url(art_cfg: &ArtifactoryConfig, name: &str, version: u32, env: &str) -> String {
    let tar_url = format!(
        "{}/{}/env/{}/{}/{}/{}.tar.gz",
        art_cfg.slave,
        art_cfg.vgroup,
        env,
        name,
        version.to_string(),
        name
    );

    trace!("Inferring tarball location as {}", tar_url);
    tar_url
}

async fn get_dependency_url_latest(
    art_cfg: &ArtifactoryConfig,
    name: &str,
    env: &str,
) -> LalResult<Component> {
    let url = format!(
        "{}/api/storage/{}/{}/{}/{}",
        art_cfg.master, art_cfg.release, "env", env, name
    );
    let v = get_storage_as_u32(&url).await?;

    debug!("Found latest version as {}", v);
    Ok(Component {
        location: get_dependency_env_url(art_cfg, name, v, env),
        version: v,
        name: name.into(),
    })
}

// This queries the API for the default location
// if a default exists, then all our current multi-builds must exist
async fn get_latest_versions(art_cfg: &ArtifactoryConfig, name: &str, env: &str) -> LalResult<Vec<u32>> {
    let url = format!(
        "{}/api/storage/{}/{}/{}/{}",
        art_cfg.master, art_cfg.release, "env", env, name
    );

    get_storage_versions(&url).await
}

/// Main entry point for install
async fn get_tarball_uri(
    art_cfg: &ArtifactoryConfig,
    name: &str,
    version: Option<u32>,
    env: &str,
) -> LalResult<Component> {
    if let Some(v) = version {
        Ok(Component {
            location: get_dependency_env_url(art_cfg, name, v, env),
            version: v,
            name: name.into(),
        })
    } else {
        get_dependency_url_latest(art_cfg, name, env).await
    }
}

/// Latest lal version - as seen on artifactory
#[cfg(feature = "upgrade")]
pub struct LatestLal {
    /// URL of the latest tarball
    pub url: String,
    /// Semver::Version of the latest tarball
    pub version: Version,
}

/// Entry point for `lal::upgrade`
///
/// This mostly duplicates the behaviour in `get_storage_as_u32`, however,
/// it is parsing the version as a `semver::Version` struct rather than a u32.
/// This is used regardless of your used backend because we want people to use our
/// main release of lal on CME-release on cisco artifactory at the moment.
#[cfg(feature = "upgrade")]
pub fn get_latest_lal_version() -> LalResult<LatestLal> {
    // canonical latest url
    let uri = "https://engci-maven-master.cisco.com/artifactory/api/storage/CME-release/lal";
    debug!("GET {}", uri);
    let resp: String = hyper_req(uri).map_err(|e| {
        warn!("Failed to GET {}: {}", uri, e);
        CliError::BackendFailure("No version information found on API".into())
    })?;
    trace!("Got body {}", resp);

    let res: ArtifactoryStorageResponse = serde_json::from_str(&resp)?;
    let latest: Option<Version> = res
        .children
        .iter()
        .map(|r| r.uri.trim_matches('/').to_string())
        .inspect(|v| trace!("Found lal version {}", v))
        .filter_map(|v| Version::parse(&v).ok())
        .max(); // Semver::Version implements an order

    if let Some(l) = latest {
        Ok(LatestLal {
            version: l.clone(),
            url: format!(
                "https://engci-maven.cisco.com/artifactory/CME-group/lal/{}/lal.tar.gz",
                l
            ),
        })
    } else {
        warn!("Failed to parse version information from artifactory storage api for lal");
        Err(CliError::BackendFailure(
            "No version information found on API".into(),
        ))
    }
}

use super::{Backend, Component};

/// Everything we need for Artifactory to implement the Backend trait
pub struct ArtifactoryBackend {
    /// Artifactory config and credentials
    pub config: ArtifactoryConfig,
    /// Cache directory
    pub cache: PathBuf,
}

impl ArtifactoryBackend {
    pub fn new(cfg: &ArtifactoryConfig, cache: &Path) -> LalResult<Self> {
        // TODO: create hyper clients in here rather than once per download
        let backend = ArtifactoryBackend {
            config: cfg.clone(),
            cache: cache.to_path_buf(),
        };

        Ok(backend)
    }
}

/// Artifact backend trait for `ArtifactoryBackend`
///
/// This is intended to be used by the caching trait `CachedBackend`, but for
/// specific low-level use cases, these methods can be used directly.
#[async_trait::async_trait]
impl Backend for ArtifactoryBackend {
    async fn get_versions(&self, name: &str, loc: &str) -> LalResult<Vec<u32>> {
        get_latest_versions(&self.config, name, loc).await
    }

    async fn get_latest_version(&self, name: &str, loc: &str) -> LalResult<u32> {
        let latest = get_dependency_url_latest(&self.config, name, loc).await?;
        Ok(latest.version)
    }

    async fn get_component_info(&self, name: &str, version: Option<u32>, loc: &str) -> LalResult<Component> {
        get_tarball_uri(&self.config, name, version, loc).await
    }

    async fn publish_artifact(
        &self,
        _home: Option<&Path>,
        component_dir: &Path,
        name: &str,
        version: u32,
        env: &str,
    ) -> LalResult<()> {
        // this fn basically assumes all the sanity checks have been performed
        // files must exist and lockfile must be sensible
        let artdir = component_dir.join("./ARTIFACT");
        let tarball = artdir.join(format!("{}.tar.gz", name));
        let lockfile = artdir.join("lockfile.json");

        // uri prefix if specific env upload
        let prefix = format!("env/{}/", env);

        let tar_uri = format!("{}{}/{}/{}.tar.gz", prefix, name, version, name);
        let mut tarf = File::open(tarball)?;
        upload_artifact(&self.config, &tar_uri, &mut tarf).await?;

        let mut lockf = File::open(lockfile)?;
        let lf_uri = format!("{}{}/{}/lockfile.json", prefix, name, version);
        upload_artifact(&self.config, &lf_uri, &mut lockf).await?;
        Ok(())
    }

    fn get_cache_dir(&self) -> PathBuf {
        self.cache.clone()
    }

    async fn raw_fetch(&self, url: &str, dest: &Path) -> LalResult<()> {
        http_download_to_path(url, dest).await
    }
}
