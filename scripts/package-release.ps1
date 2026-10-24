$ErrorActionPreference='Stop'
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskVersion=(Get-Content -LiteralPath (Join-Path $taskRoot 'package.json') -Raw | ConvertFrom-Json).version
$taskDist=Join-Path $taskRoot 'dist'
New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
$taskExecutable=Join-Path $taskRoot 'src-tauri/target/release/screenshot-darkroom.exe'
if(-not (Test-Path -LiteralPath $taskExecutable)){throw 'Build the application before packaging.'}
$taskInstaller=Get-ChildItem -LiteralPath (Join-Path $taskRoot 'src-tauri/target/release/bundle/nsis') -Filter '*setup.exe' | Where-Object {$_.Name -like "*${taskVersion}*"} | Select-Object -First 1
if(-not $taskInstaller){throw 'NSIS installer not found; run npm run build first.'}
$taskInstallerName="Darkroom-${taskVersion}-windows-x64-setup.exe"
Copy-Item -LiteralPath $taskInstaller.FullName -Destination (Join-Path $taskDist $taskInstallerName) -Force
$taskPortable=Join-Path $taskRoot '.qa/portable-stage/Darkroom'
New-Item -ItemType Directory -Path $taskPortable -Force | Out-Null
Copy-Item -LiteralPath $taskExecutable -Destination (Join-Path $taskPortable '截图冲洗室.exe') -Force
foreach($taskFile in @('LICENSE','THIRD-PARTY-LICENSES.txt','CHANGELOG.md')){Copy-Item -LiteralPath (Join-Path $taskRoot $taskFile) -Destination $taskPortable -Force}
@"
截图冲洗室 Darkroom v$taskVersion

双击 截图冲洗室.exe。按 Win + Shift + S 框选截图，新截图自动挂到桌面晾片绳。
主窗口不会自动弹出，点击挂件照片可继续整理、识别文字和创建便签。
重要截图可固定在晾片绳上。设置中可关闭自动挂片、调整展示数量。
关闭主窗口收起到托盘；点挂件 × 可收起挂件；右键托盘菜单可退出。

Windows 10 / 11 x64，需要 Microsoft Edge WebView2 Runtime。
如果无法启动，优先使用安装版，安装版会在缺少运行组件时下载并安装 WebView2。
数据保存在 %APPDATA%\com.darkroom.screenshot.desktop，与安装版共享。
程序不上传截图，发布包不包含任何个人截图或收藏数据。
备份包含原图，可在设置中导出；数据目录中保留上一版 collection.previous.json。

项目：https://github.com/15380193489/screenshot-darkroom
"@ | Set-Content -LiteralPath (Join-Path $taskPortable '使用说明.txt') -Encoding utf8NoBOM
$taskZipName="Darkroom-${taskVersion}-windows-x64-portable.zip"
Compress-Archive -Path $taskPortable -DestinationPath (Join-Path $taskDist $taskZipName) -Force
$taskAssets=@($taskInstallerName,$taskZipName)
$taskChecksums=foreach($taskAsset in $taskAssets){$taskHash=(Get-FileHash -LiteralPath (Join-Path $taskDist $taskAsset) -Algorithm SHA256).Hash.ToLower();"$taskHash  $taskAsset"}
$taskChecksums | Set-Content -LiteralPath (Join-Path $taskDist 'SHA256SUMS.txt') -Encoding utf8NoBOM
Get-ChildItem -LiteralPath $taskDist | Where-Object {$_.Name -in @($taskInstallerName,$taskZipName,'SHA256SUMS.txt')} | Select-Object Name,Length
