use std::path::Path;

pub async fn fetch_input(
    component_dir: &Path,
    env_name: &str,
    backend: &dyn lal::CachedBackend,
) -> lal::LalResult<()> {
    let manifest = lal::Manifest::read(&component_dir)?;
    debug!("Component manifest: {:?}", manifest);

    lal::fetch(&component_dir, &manifest, backend, true, &env_name).await
}

pub async fn fetch_dev_input(
    component_dir: &Path,
    env_name: &str,
    backend: &dyn lal::CachedBackend,
) -> lal::LalResult<()> {
    let manifest = lal::Manifest::read(&component_dir)?;

    lal::fetch(&component_dir, &manifest, backend, false, &env_name).await
}
