#[cfg(feature = "time")]
use std::time::Instant;
use std::{env, fs, io, path::PathBuf};

use rayon::{iter::ParallelBridge, prelude::ParallelIterator};

mod is_hardlink;
use is_hardlink::IsHardLink;

const HARD_LINK_CHECK_FILE: &str = "METADATA";

/// 定义存档移除类型
enum ArchiveRemove {
    /// 没有找到 .dist-info 目录
    NoDistInfo,
    /// 不是硬链接，需要移除
    NoHardLink(String),
    /// 不需要移除
    NoNeed,
}

fn main() {
    let uv_cache_dir = get_uv_cache_dir();

    #[cfg(feature = "time")]
    let start_time = Instant::now();

    prune_archive_dir(&uv_cache_dir);

    #[cfg(feature = "time")]
    {
        let end_time = Instant::now();
        println!("用时：{:?}", end_time - start_time);
    }
}

/// 获取 uv 缓存目录
fn get_uv_cache_dir() -> PathBuf {
    env::var("UV_CACHE_DIR").unwrap_or_else(|_| {
        #[cfg(target_os = "windows")]
        (env::var("LOCALAPPDATA").unwrap() + r"\uv\cache")
    }).into()
}

/// 判断是否应该移除存档
fn should_remove_archive(archive_path: &PathBuf) -> io::Result<ArchiveRemove> {
    const DIST_INFO: &str = ".dist-info";

    let dist_info_path = fs::read_dir(archive_path)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .find(|path| path.to_string_lossy().ends_with(DIST_INFO));

    let dist_info_path = match dist_info_path {
        Some(path) => path,
        None => return Ok(ArchiveRemove::NoDistInfo),
    };

    let metadata_path = dist_info_path.join(HARD_LINK_CHECK_FILE);
    match metadata_path.is_hardlink() {
        Ok(true) => Ok(ArchiveRemove::NoNeed),
        Ok(false) => Ok(ArchiveRemove::NoHardLink(
            dist_info_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_end_matches(DIST_INFO)
                .to_string(),
        )),
        Err(e) => Err(e.into()),
    }
}

/// 清理存档目录
fn prune_archive_dir(uv_cache_dir: &PathBuf) {
    const DIR: &str = "archive-v0";

    /// 移除指定路径并打印信息
    fn remove(archive_path: &PathBuf, archive_info: &str) {
        println!("删除: {archive_info}");
        if let Err(e) = fs::remove_dir_all(&archive_path) {
            eprintln!("删除 {archive_info} 失败: {e}");
        }
    }

    let archive_dir = uv_cache_dir.join(DIR);

    if let Ok(entries) = fs::read_dir(&archive_dir) {
        entries.par_bridge().for_each(|archive_entry| {
            if let Ok(archive) = archive_entry {
                let archive_path = archive.path();
                let archive_id = archive.file_name().to_string_lossy().to_string();
                match should_remove_archive(&archive_path) {
                    Ok(ArchiveRemove::NoHardLink(name)) => {
                        let archive_info = format!("{archive_id} ({name})");
                        remove(&archive_path, &archive_info);
                    }
                    Ok(ArchiveRemove::NoDistInfo) => remove(&archive_path, &archive_id),
                    Ok(ArchiveRemove::NoNeed) => {}
                    Err(e) => {
                        eprintln!("检查 {archive_id} {HARD_LINK_CHECK_FILE} 失败: {e}");
                    }
                }
            }
        });
    } else {
        eprintln!("无法读取目录: {}", archive_dir.display());
    }
}
