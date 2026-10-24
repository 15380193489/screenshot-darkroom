$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskQa = Join-Path $taskRoot '.qa'
New-Item -ItemType Directory -Path $taskQa -Force | Out-Null
$bitmap = New-Object System.Drawing.Bitmap 1000, 650
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.Clear([System.Drawing.Color]::White)
$graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$font = New-Object System.Drawing.Font 'Microsoft YaHei', 35
$graphics.DrawString('截图冲洗室 测试样片', $font, [System.Drawing.Brushes]::Black, 50, 50)
$graphics.DrawString('把灵感留在这里', $font, [System.Drawing.Brushes]::Black, 50, 150)
$graphics.DrawString('Hello Darkroom 2026', $font, [System.Drawing.Brushes]::Black, 50, 260)
$graphics.DrawString('https://example.com', $font, [System.Drawing.Brushes]::Black, 50, 370)
$bitmap.Save((Join-Path $taskQa 'ocr-fixture.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose(); $bitmap.Dispose(); $font.Dispose()
try {
  $languages = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType=WindowsRuntime]::AvailableRecognizerLanguages
  $languages | ForEach-Object { $_.LanguageTag }
  [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType=WindowsRuntime]::MaxImageDimension
} catch { Write-Output 'Language discovery will be verified by the native app.' }
