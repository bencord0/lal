use std::path::Path;

pub fn set_environment(
    component_dir: &Path,
    home: &Path,
    sticky: &lal::StickyOptions,
    env_name: &str,
) -> lal::LalResult<()> {
    let config = lal::Config::read(Some(&home))?;
    lal::env::set(&component_dir, &sticky, &config, &env_name)
}

pub fn clear_environment(component_dir: &Path) -> lal::LalResult<()> {
    lal::env::clear(&component_dir)
}
