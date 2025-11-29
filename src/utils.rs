use std::path::PathBuf;

use anyhow::Result;

pub(crate) fn clean_invalid_characters<S>(input: S) -> String
where
    S: AsRef<str>,
{
    let invalid_chars = ['<', '>', ':', '\'', '"', '/', '\\', '|', '?', '*'];
    let allows_non_ascii = !cfg!(windows);
    input
        .as_ref()
        .chars()
        .filter(|&c| {
            !invalid_chars.contains(&c)
                && !c.is_control()
                && (allows_non_ascii || c.is_ascii())
        })
        .collect()
}

const DOT_PATH: &str = ".spotify-dl";

pub(crate) fn get_dot_path() -> Result<PathBuf> {
    let path = dirs::home_dir()
        .map(|p| p.join(DOT_PATH))
        .ok_or(anyhow::anyhow!("Could not find home directory"))?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
