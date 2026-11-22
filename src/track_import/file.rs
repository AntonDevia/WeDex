use anyhow::{bail, Context, Result};

pub fn decode(bytes: Vec<u8>) -> Result<String> {
    if bytes.len() > super::FILE_LIMIT {
        bail!("Размер списка превышает 8 МБ");
    }
    String::from_utf8(bytes).context("Выберите текстовый файл в кодировке UTF-8")
}

#[cfg(windows)]
pub fn pick() -> Result<Option<String>> {
    use std::{io::Read, mem::size_of, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::UI::{
        Controls::Dialogs::{
            CommDlgExtendedError, GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST,
            OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
        },
        WindowsAndMessaging::GetForegroundWindow,
    };

    let filter: Vec<u16> = "Списки треков\0*.json;*.csv;*.txt;*.html;*.htm\0\0"
        .encode_utf16()
        .collect();
    let mut name = vec![0_u16; 32_768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: unsafe { GetForegroundWindow() },
        lpstrFilter: filter.as_ptr(),
        lpstrFile: name.as_mut_ptr(),
        nMaxFile: name.len() as u32,
        Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        if unsafe { CommDlgExtendedError() } != 0 {
            bail!("Не удалось открыть окно выбора файла");
        }
        return Ok(None);
    }
    name.truncate(name.iter().position(|&c| c == 0).unwrap_or(name.len()));
    let path = PathBuf::from(std::ffi::OsString::from_wide(&name));
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| {
            file.take((super::FILE_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| anyhow::anyhow!("Не удалось прочитать файл"))?;
    decode(bytes).map(Some)
}
