//! The prebuilt Windows launcher carries a bounded, self-contained player archive.
use crate::{game_tools, package, workdir};
use anyhow::{ensure, Context, Result};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

const MAGIC: &[u8; 16] = b"LOMMOD-INSTALL-1";
const FOOTER_SIZE: u64 = (MAGIC.len() + 8) as u64;
const MAX_PAYLOAD: u64 = package::MAX_PACKAGE_BYTES + 4 * package::MAX_ENTRY_BYTES;

pub(super) fn append_payload(launcher: &Path, archive: &Path, target: &Path) -> Result<()> {
    let mut launcher =
        File::open(launcher).context("缺少预编译的一键安装器，请使用完整的 Windows 编辑器")?;
    let mut header = [0; 2];
    launcher.read_exact(&mut header)?;
    ensure!(&header == b"MZ", "一键安装器模板不是 Windows 可执行文件");
    launcher.rewind()?;
    let mut archive = File::open(archive)?;
    let size = archive.metadata()?.len();
    ensure!(size <= MAX_PAYLOAD, "一键安装包过大");
    let mut target = File::create(target)?;
    let launcher_size = io::copy(&mut launcher, &mut target)?;
    // Authenticode starts its certificate table on an 8-byte boundary.
    let padding = (8 - (launcher_size + size + FOOTER_SIZE) % 8) % 8;
    target.write_all(&[0; 7][..padding as usize])?;
    ensure!(
        io::copy(&mut archive, &mut target)? == size,
        "安装数据在导出期间发生变化"
    );
    target.write_all(&size.to_le_bytes())?;
    target.write_all(MAGIC)?;
    target.sync_all()?;
    Ok(())
}

pub fn unpack_play_installer(path: &Path) -> Result<tempfile::TempDir> {
    let mut source = File::open(path)?;
    let end = source.metadata()?.len();
    let start = if let Some(start) = payload_range(&mut source, end)? {
        start
    } else {
        let certificate = certificate_offset(&mut source, end)?;
        payload_range(&mut source, certificate)?
            .context("没有找到作品数据，请重新获取导出的一键安装包")?
    };
    source.seek(SeekFrom::Start(start.0))?;
    let workspace = workdir::create()?;
    let mut archive = tempfile::tempfile_in(workspace.path())?;
    ensure!(
        io::copy(&mut source.take(start.1), &mut archive)? == start.1,
        "安装包已损坏"
    );
    archive.rewind()?;
    let mut zip = zip::ZipArchive::new(archive).context("无法读取安装包中的作品数据")?;
    let mut total = 0_u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = package::canonical_archive_name(entry.name())?;
        let limit = match name.as_str() {
            "play_bundle.json"
            | "LICENSE.txt"
            | "THIRD_PARTY_NOTICES.md"
            | "LICENSE.UnityDoorstop" => package::MAX_TEXT_BYTES,
            "loaders/x86.zip" | "loaders/x64.zip" => package::MAX_ENTRY_BYTES,
            path if path
                .strip_prefix("runtime/")
                .is_some_and(|n| game_tools::HOST_FILES.contains(&n)) =>
            {
                package::MAX_ENTRY_BYTES
            }
            path if path
                .strip_prefix("mods/")
                .is_some_and(|n| !n.contains('/') && n.ends_with(".lommod")) =>
            {
                package::MAX_PACKAGE_BYTES
            }
            _ => anyhow::bail!("安装包含有意外条目：{name}"),
        };
        let expected = entry.size();
        ensure!(
            entry.is_file() && expected <= limit,
            "安装包条目无效：{name}"
        );
        total += expected;
        ensure!(total <= MAX_PAYLOAD, "安装包解压后过大");
        let target = workspace.path().join(&name);
        fs::create_dir_all(target.parent().unwrap())?;
        let mut target = File::create_new(target)?;
        ensure!(
            io::copy(&mut entry.by_ref().take(limit + 1), &mut target)? == expected,
            "安装包条目长度不符：{name}"
        );
    }
    Ok(workspace)
}

fn payload_range(source: &mut File, end: u64) -> Result<Option<(u64, u64)>> {
    let footer = end.checked_sub(FOOTER_SIZE).context("安装包不完整")?;
    source.seek(SeekFrom::Start(footer))?;
    let mut size = [0; 8];
    let mut marker = [0; MAGIC.len()];
    source.read_exact(&mut size)?;
    source.read_exact(&mut marker)?;
    if &marker != MAGIC {
        return Ok(None);
    }
    let size = u64::from_le_bytes(size);
    ensure!(
        size > 0 && size < footer && size <= MAX_PAYLOAD,
        "安装包数据长度无效"
    );
    Ok(Some((footer - size, size)))
}

fn certificate_offset(source: &mut File, end: u64) -> Result<u64> {
    source.rewind()?;
    let mut dos = [0; 64];
    source.read_exact(&mut dos).context("安装包 PE 头不完整")?;
    ensure!(&dos[..2] == b"MZ", "安装包不是 Windows 可执行文件");
    let pe = u64::from(u32::from_le_bytes(dos[60..64].try_into()?));
    source.seek(SeekFrom::Start(pe))?;
    let mut header = [0; 24];
    source.read_exact(&mut header)?;
    ensure!(&header[..4] == b"PE\0\0", "安装包 PE 头无效");
    let optional_size = u16::from_le_bytes(header[20..22].try_into()?) as usize;
    let mut optional = vec![0; optional_size];
    source.read_exact(&mut optional)?;
    let certificate_entry = match optional.get(..2) {
        Some([0x0b, 1]) => 96 + 4 * 8,  // PE32
        Some([0x0b, 2]) => 112 + 4 * 8, // PE32+
        _ => anyhow::bail!("安装包 PE 格式无效"),
    };
    let directory = optional
        .get(certificate_entry..certificate_entry + 8)
        .context("安装包中没有作品或签名数据")?;
    // Unlike the other PE directories, the security directory uses file offsets.
    let offset = u64::from(u32::from_le_bytes(directory[..4].try_into()?));
    let size = u64::from(u32::from_le_bytes(directory[4..].try_into()?));
    ensure!(
        offset >= FOOTER_SIZE && size > 0 && offset + size == end,
        "安装包中没有有效的作品数据"
    );
    Ok(offset)
}
