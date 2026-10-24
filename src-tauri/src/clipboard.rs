use base64::{Engine, engine::general_purpose::STANDARD};
use windows::Win32::{
    Foundation::HGLOBAL,
    System::{
        DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        },
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    },
};

pub fn from_dib(dib: &[u8]) -> Result<Vec<u8>, String> {
    fn u32_at(b: &[u8], p: usize) -> Result<u32, String> {
        b.get(p..p + 4)
            .and_then(|s| s.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or("图片头不完整".into())
    }
    if dib.len() < 40 {
        return Err("图片头不完整".into());
    }
    let header = u32_at(dib, 0)? as usize;
    let width = u32_at(dib, 4)? as i32;
    let signed_height = u32_at(dib, 8)? as i32;
    let height = signed_height.unsigned_abs() as usize;
    let bits = u16::from_le_bytes([dib[14], dib[15]]) as usize;
    let compression = u32_at(dib, 16)?;
    if !(40..=124).contains(&header)
        || width <= 0
        || height == 0
        || ![24, 32].contains(&bits)
        || ![0, 3].contains(&compression)
    {
        return Err("暂不支持这种剪贴板图片格式，请另存为 PNG 后导入".into());
    }
    let width = width as usize;
    if width > 16000 || height > 16000 || width.checked_mul(height).is_none_or(|n| n > 32_000_000) {
        return Err("截图太大，请分区域截取".into());
    }
    let masks = if compression == 3 && header == 40 {
        12
    } else {
        0
    };
    if compression == 3
        && (u32_at(dib, 40)? != 0xff0000 || u32_at(dib, 44)? != 0xff00 || u32_at(dib, 48)? != 0xff)
    {
        return Err("暂不支持这种颜色掩码".into());
    }
    let offset = header + masks;
    let stride = (width * bits).div_ceil(32) * 4;
    if offset
        .checked_add(stride * height)
        .is_none_or(|n| n > dib.len())
    {
        return Err("剪贴板图片数据不完整".into());
    }
    let mut rgba = vec![255u8; width * height * 4];
    for y in 0..height {
        let source_y = if signed_height < 0 { y } else { height - 1 - y };
        for x in 0..width {
            let p = offset + source_y * stride + x * (bits / 8);
            let d = (y * width + x) * 4;
            rgba[d..d + 3].copy_from_slice(&[dib[p + 2], dib[p + 1], dib[p]]);
        }
    }
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn read_image() -> Result<Option<String>, String> {
    unsafe {
        let format = if IsClipboardFormatAvailable(17).is_ok() {
            17
        } else if IsClipboardFormatAvailable(8).is_ok() {
            8
        } else {
            return Ok(None);
        };
        OpenClipboard(None).map_err(|_| "剪贴板暂时正忙，请稍后重试".to_string())?;
        struct ClipboardGuard;
        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = CloseClipboard();
                }
            }
        }
        let _guard = ClipboardGuard;
        let handle = GetClipboardData(format).map_err(|e| e.to_string())?;
        let mem = HGLOBAL(handle.0);
        let len = GlobalSize(mem);
        if len < 40 || len > 140_000_000 {
            return Err("剪贴板图片大小异常".into());
        }
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            return Err("暂时无法读取截图".into());
        }
        let data = std::slice::from_raw_parts(ptr.cast::<u8>(), len).to_vec();
        let _ = GlobalUnlock(mem);
        // Release clipboard before encoding a potentially large image.
        drop(_guard);
        from_dib(&data).map(|png| Some(format!("data:image/png;base64,{}", STANDARD.encode(png))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dib(width: i32, height: i32, bits: u16, pixels: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&width.to_le_bytes());
        b[8..12].copy_from_slice(&height.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(pixels);
        b
    }
    #[test]
    fn rejects_truncated_and_huge() {
        assert!(from_dib(&[]).is_err());
        assert!(from_dib(&dib(50000, 50000, 32, &[])).is_err());
        assert!(from_dib(&dib(2, 2, 32, &[0; 4])).is_err());
    }
    #[test]
    fn converts_bottom_up_and_opaque_alpha() {
        let b = dib(1, 2, 32, &[255, 0, 0, 0, 0, 0, 255, 0]);
        let png = from_dib(&b).unwrap();
        let mut r = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut out = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut out).unwrap();
        assert_eq!(out, &[255, 0, 0, 255, 0, 0, 255, 255]);
    }
    #[test]
    fn converts_top_down_24_bit_with_padding() {
        let b = dib(1, -2, 24, &[0, 255, 0, 0, 255, 0, 0, 0]);
        let png = from_dib(&b).unwrap();
        let mut r = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut out = vec![0; r.output_buffer_size().unwrap()];
        r.next_frame(&mut out).unwrap();
        assert_eq!(out, &[0, 255, 0, 255, 0, 0, 255, 255]);
    }
}
