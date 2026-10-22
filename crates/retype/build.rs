//! Embeds the app icon (`assets/retype.ico`) into the Windows exe.
//!
//! No resource compiler or extra crate: the `.ico` is repacked into a `.res`
//! file (RT_ICON per image + one RT_GROUP_ICON), which the MSVC linker
//! accepts as an input. Other targets build without an icon.

use std::env;
use std::fs;
use std::path::PathBuf;

const ICON: &str = "../../assets/retype.ico";
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// Resource id of the icon group; the shell shows the lowest one.
const GROUP_ID: u16 = 1;
/// LANG_NEUTRAL, SUBLANG_NEUTRAL.
const LANGUAGE: u16 = 0;
/// MOVEABLE | PURE | DISCARDABLE, as rc.exe writes for icons.
const MEMORY_FLAGS: u16 = 0x1030;

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    let windows_msvc = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if !windows_msvc {
        return;
    }
    let ico = fs::read(ICON).unwrap_or_else(|e| panic!("cannot read {ICON}: {e}"));
    let res = res_from_ico(&ico).unwrap_or_else(|e| panic!("bad {ICON}: {e}"));
    let out_dir = env::var("OUT_DIR").unwrap_or_else(|e| panic!("OUT_DIR: {e}"));
    let out = PathBuf::from(out_dir).join("retype.res");
    fs::write(&out, res).unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// One image of the `.ico`: its directory entry (without the file offset)
/// and its data.
struct Image<'a> {
    entry: [u8; 12],
    data: &'a [u8],
}

fn res_from_ico(ico: &[u8]) -> Result<Vec<u8>, String> {
    let images = parse_ico(ico)?;
    let mut res = Vec::new();
    // A .res file starts with an empty resource that marks it as 32-bit.
    push_resource(&mut res, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&0u16.to_le_bytes()); // reserved
    group.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    let count = u16::try_from(images.len()).map_err(|_| "too many images")?;
    group.extend_from_slice(&count.to_le_bytes());
    for (index, image) in images.iter().enumerate() {
        let id = u16::try_from(index + 1).map_err(|_| "too many images")?;
        push_resource(&mut res, RT_ICON, id, image.data);
        // GRPICONDIRENTRY: the ICONDIRENTRY fields, with the resource id in
        // place of the file offset.
        group.extend_from_slice(&image.entry);
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, GROUP_ID, &group);
    Ok(res)
}

fn parse_ico(ico: &[u8]) -> Result<Vec<Image<'_>>, String> {
    let u16_at = |at: usize| -> Result<u16, String> {
        ico.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| "truncated header".to_owned())
    };
    let u32_at = |at: usize| -> Result<usize, String> {
        ico.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(|| "truncated directory".to_owned())
    };
    if u16_at(0)? != 0 || u16_at(2)? != 1 {
        return Err("not an icon file".to_owned());
    }
    let count = usize::from(u16_at(4)?);
    let mut images = Vec::with_capacity(count);
    for i in 0..count {
        let at = 6 + 16 * i;
        let size = u32_at(at + 8)?;
        let offset = u32_at(at + 12)?;
        let data = ico
            .get(offset..offset + size)
            .ok_or_else(|| format!("image {i} out of bounds"))?;
        let mut entry = [0u8; 12];
        entry.copy_from_slice(&ico[at..at + 12]);
        images.push(Image { entry, data });
    }
    Ok(images)
}

/// Appends one resource: RESOURCEHEADER with numeric type and name, then
/// the data, padded to a 4-byte boundary.
fn push_resource(res: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    let header_size: u32 = 32;
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&header_size.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&kind.to_le_bytes());
    res.extend_from_slice(&0xFFFFu16.to_le_bytes());
    res.extend_from_slice(&name.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // data version
    let flags = if kind == 0 { 0 } else { MEMORY_FLAGS };
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&LANGUAGE.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // version
    res.extend_from_slice(&0u32.to_le_bytes()); // characteristics
    res.extend_from_slice(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}
