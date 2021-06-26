use std::io::{self, Write};

use super::{CliError, LalResult};
use crate::storage::CachedBackend;

/// Prints a list of versions associated with a component
pub async fn query(
    backend: &dyn CachedBackend,
    _env: Option<&str>,
    component: &str,
    last: bool,
) -> LalResult<()> {
    if component.to_lowercase() != component {
        return Err(CliError::InvalidComponentName(component.into()));
    }
    let env = match _env {
        None => {
            error!("query is no longer allowed without an explicit environment");
            return Err(CliError::EnvironmentUnspecified);
        }
        Some(e) => e,
    };

    if last {
        let ver = backend.get_latest_version(component, env).await?;
        println!("{}", ver);
    } else {
        let vers = backend.get_versions(component, env).await?;
        for v in vers {
            println!("{}", v);
            // needed because sigpipe handling is broken for stdout atm
            // see #36 - can probably be taken out in rust 1.16 or 1.17
            // if `lal query media-engine | head` does not crash
            if io::stdout().flush().is_err() {
                return Ok(());
            }
        }
    }
    Ok(())
}
