use std::{fs, path::Path};

use chrono::{DateTime, Duration, TimeZone, Utc};
use filetime::FileTime;
use walkdir::WalkDir;

use super::LalResult;

// helper for `lal::clean`
fn clean_in_dir(cutoff: DateTime<Utc>, dirs: WalkDir) -> LalResult<()> {
    let drs = dirs
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir());

    for d in drs {
        let pth = d.path();
        trace!("Checking {:?}", pth);
        let mtime = FileTime::from_last_modification_time(&d.metadata()?);
        let mtimedate = Utc.ymd(1970, 1, 1).and_hms(0, 0, 0) + Duration::seconds(mtime.unix_seconds() as i64);

        trace!("Found {:?} with mtime {}", pth, mtimedate);
        if mtimedate < cutoff {
            debug!("Cleaning {:?}", pth);
            fs::remove_dir_all(pth)?;
        }
    }
    Ok(())
}

/// Clean old artifacts in cache directory
///
/// This does the equivalent of find CACHEDIR -mindepth 3 -maxdepth 3 -type d
/// With the correct mtime flags, then -exec deletes these folders.
pub fn clean(cache: &Path, days: i64) -> LalResult<()> {
    let cutoff = Utc::now() - Duration::days(days);
    debug!("Cleaning all artifacts from before {}", cutoff);

    // clean out environment subdirectories
    let edir = cache.join("environments");
    let edirs = WalkDir::new(&edir).min_depth(3).max_depth(3);
    clean_in_dir(cutoff, edirs)?;

    // clean out stash
    let dirs = WalkDir::new(&cache).min_depth(3).max_depth(3);
    clean_in_dir(cutoff, dirs)?;

    Ok(())
}
