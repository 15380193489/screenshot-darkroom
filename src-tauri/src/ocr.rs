use serde::Serialize;
use windows::{
    Globalization::Language,
    Graphics::Imaging::{
        BitmapAlphaMode, BitmapDecoder, BitmapPixelFormat, BitmapTransform, ColorManagementMode,
        ExifOrientationMode,
    },
    Media::Ocr::OcrEngine,
    Storage::Streams::{DataWriter, InMemoryRandomAccessStream},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

#[derive(Serialize)]
pub struct OcrOutput {
    pub text: String,
    pub language: String,
}

fn apartment<T>(f: impl FnOnce() -> windows::core::Result<T>) -> Result<T, String> {
    // WinRT activation factories are cached by windows-rs. Keep an MTA alive
    // for the process lifetime so a completed language lookup cannot unload
    // the factory before the next OCR request uses it on another worker.
    static MTA: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    MTA.get_or_init(|| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("darkroom-winrt".into())
            .spawn(move || {
                let result =
                    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string());
                let ready = result.is_ok();
                let _ = send.send(result);
                if ready {
                    loop {
                        std::thread::park();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        receive.recv().map_err(|e| e.to_string())?
    })
    .clone()?;
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(|e| e.to_string())?;
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            unsafe { RoUninitialize() }
        }
    }
    let _guard = Guard;
    f().map_err(|e|format!("Windows 文字识别不可用：{e}。请在 Windows 设置 → 时间和语言 → 语言选项中安装对应语言的「光学字符识别」，或手动填写便签。"))
}

pub fn languages() -> Result<Vec<String>, String> {
    apartment(|| {
        let langs = OcrEngine::AvailableRecognizerLanguages()?;
        let mut out = Vec::new();
        for i in 0..langs.Size()? {
            out.push(langs.GetAt(i)?.LanguageTag()?.to_string());
        }
        Ok(out)
    })
}

pub fn recognize(bytes: &[u8], requested: &str) -> Result<OcrOutput, String> {
    apartment(|| {
        let stream = InMemoryRandomAccessStream::new()?;

        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(bytes)?;

        writer.StoreAsync()?.get()?;

        writer.DetachStream()?;
        stream.Seek(0)?;

        let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;

        let max_dimension = OcrEngine::MaxImageDimension()?;

        let width = decoder.PixelWidth()?;

        let height = decoder.PixelHeight()?;

        let bitmap = if width.max(height) > max_dimension {
            let scale = max_dimension as f64 / width.max(height) as f64;
            let transform = BitmapTransform::new()?;
            transform.SetScaledWidth(((width as f64 * scale).floor() as u32).max(1))?;
            transform.SetScaledHeight(((height as f64 * scale).floor() as u32).max(1))?;
            decoder
                .GetSoftwareBitmapTransformedAsync(
                    BitmapPixelFormat::Bgra8,
                    BitmapAlphaMode::Ignore,
                    &transform,
                    ExifOrientationMode::RespectExifOrientation,
                    ColorManagementMode::DoNotColorManage,
                )?
                .get()?
        } else {
            decoder.GetSoftwareBitmapAsync()?.get()?
        };

        let engine = if requested == "auto" {
            Language::CreateLanguage(&HSTRING::from("zh-Hans-CN"))
                .and_then(|l| OcrEngine::TryCreateFromLanguage(&l))
                .or_else(|_| OcrEngine::TryCreateFromUserProfileLanguages())?
        } else {
            let lang = Language::CreateLanguage(&HSTRING::from(requested))?;
            OcrEngine::TryCreateFromLanguage(&lang)?
        };

        let result = engine.RecognizeAsync(&bitmap)?.get()?;

        let lines = result.Lines()?;
        let mut text = Vec::new();
        for i in 0..lines.Size()? {
            text.push(lines.GetAt(i)?.Text()?.to_string());
        }
        Ok(OcrOutput {
            text: text.join("\n"),
            language: engine.RecognizerLanguage()?.LanguageTag()?.to_string(),
        })
    })
}
