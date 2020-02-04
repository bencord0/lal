use std::path::Path;

pub fn fetch_input(
    component_dir: &Path,
    env_name: &str,
    backend: &dyn lal::CachedBackend,
) -> lal::LalResult<()> {
    let manifest = lal::Manifest::read(&component_dir)?;
    let env_name = env_name.to_str()
        // Convert Option to Result, until try_trait is stable
        // https://doc.rust-lang.org/std/option/enum.Option.html#impl-Try
        .ok_or(lal::CliError::OptionIsNone)?;

    lal::fetch(&component_dir, &manifest, backend, true, &env_name)
}

pub fn fetch_dev_input(
    component_dir: &Path,
    env_name: &str,
    backend: &dyn lal::CachedBackend,
) -> lal::LalResult<()> {
    let manifest = lal::Manifest::read(&component_dir)?;
    let env_name = env_name.to_str()
        // Convert Option to Result, until try_trait is stable
        // https://doc.rust-lang.org/std/option/enum.Option.html#impl-Try
        .ok_or(lal::CliError::OptionIsNone)?;

    lal::fetch(&component_dir, &manifest, backend, false, &env_name)
}
