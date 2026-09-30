use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

const MAX_SETTINGS_BYTES: u64 = 8 * 1024 * 1024;
const IMPORT_MARKER: &str = ".ftl-zap-settings-import-checked";

/// 只复制可解析的便携设置快照；不共享数据库、不搬移密钥，也不覆盖 FTL 文件。
pub(super) fn import_legacy_settings(source: &Path, destination: &Path) -> io::Result<bool> {
    let directory = destination.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Settings directory is unavailable",
        )
    })?;
    let marker = directory.join(IMPORT_MARKER);
    if marker.exists() {
        return Ok(false);
    }
    let mut imported = false;
    if !destination.exists() && source.exists() {
        let file = File::open(source)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Legacy settings must be a regular file",
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Legacy settings exceed the size limit",
            ));
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Legacy settings are not UTF-8")
        })?;
        // 解析失败不记录内容，避免把用户配置写入日志。
        let _: toml::Value = toml::from_str(text).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Legacy settings are not valid TOML",
            )
        })?;
        fs::create_dir_all(directory)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".ftl-settings-import-")
            .tempfile_in(directory)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        match temporary.persist_noclobber(destination) {
            Ok(_) => imported = true,
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.error),
        }
    }
    fs::create_dir_all(directory)?;
    match OpenOptions::new().write(true).create_new(true).open(marker) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    Ok(imported)
}

#[cfg(test)]
#[path = "legacy_profile_tests.rs"]
mod tests;
