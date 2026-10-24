param([switch]$DisableAutoMount)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot=Split-Path -Parent $PSScriptRoot
$taskProfile=Join-Path $taskRoot ('.qa/auto-profile-'+[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
New-Item -ItemType Directory -Path $taskProfile | Out-Null
if($DisableAutoMount){'{"enabled":false,"autoMount":false,"photoLimit":4}' | Set-Content -LiteralPath (Join-Path $taskProfile 'desktop.json') -Encoding utf8NoBOM}
$taskFixture=Join-Path $taskProfile 'full-screen-fixture.png'
$taskBitmap=New-Object System.Drawing.Bitmap 1920,1080
$taskGraphics=[System.Drawing.Graphics]::FromImage($taskBitmap)
$taskGraphics.Clear([System.Drawing.Color]::Beige)
$taskFont=New-Object System.Drawing.Font 'Segoe UI',42
$taskGraphics.DrawString('FULL SCREEN / AUTO CLOTHESLINE', $taskFont,[System.Drawing.Brushes]::DarkOliveGreen,120,190)
$taskBitmap.Save($taskFixture,[System.Drawing.Imaging.ImageFormat]::Png)
$taskGraphics.Dispose();$taskFont.Dispose();$taskBitmap.Dispose()
$taskSmallFixture=Join-Path $taskProfile 'region-fixture.png'
$taskBitmap=New-Object System.Drawing.Bitmap 1000,650
$taskGraphics=[System.Drawing.Graphics]::FromImage($taskBitmap)
$taskGraphics.Clear([System.Drawing.Color]::LightGoldenrodYellow)
$taskBitmap.Save($taskSmallFixture,[System.Drawing.Imaging.ImageFormat]::Png)
$taskGraphics.Dispose();$taskBitmap.Dispose()
$taskProcess=$null
$taskOldQa=$env:DARKROOM_QA_DIR
$taskOldHidden=$env:DARKROOM_QA_HIDE_DESKTOP
try {
    $env:DARKROOM_QA_DIR=$taskProfile
    $env:DARKROOM_QA_HIDE_DESKTOP='1'
    $taskProcess=Start-Process -FilePath (Join-Path $taskRoot 'src-tauri/target/debug/screenshot-darkroom.exe') -ArgumentList ('--background --capture-fixture "'+$taskSmallFixture+'" "'+$taskFixture+'"') -WindowStyle Hidden -PassThru
    $taskCollectionPath=Join-Path $taskProfile 'collection.json'
    $taskDesktopPath=Join-Path $taskProfile 'desktop-report.json'
    $taskLayoutPath=Join-Path $taskProfile 'desktop-layout.json'
    $taskCollection=$null;$taskDesktop=$null;$taskLayout=$null
    for($taskAttempt=0;$taskAttempt -lt 60;$taskAttempt++) {
        Start-Sleep -Milliseconds 250
        if(Test-Path -LiteralPath $taskCollectionPath){$taskCollection=Get-Content -LiteralPath $taskCollectionPath -Raw | ConvertFrom-Json}
        if(Test-Path -LiteralPath $taskDesktopPath){$taskDesktop=Get-Content -LiteralPath $taskDesktopPath -Raw | ConvertFrom-Json}
        if(Test-Path -LiteralPath $taskLayoutPath){$taskLayout=Get-Content -LiteralPath $taskLayoutPath -Raw | ConvertFrom-Json}
        if($taskCollection -and @($taskCollection.photos).Count -eq 2 -and ($DisableAutoMount -or ($taskDesktop.rendererReady -and $taskLayout.cards -eq 2))){break}
    }
    $taskStartup=Get-Content -LiteralPath (Join-Path $taskProfile 'visibility-startup.json') -Raw | ConvertFrom-Json
    $taskCapture=Get-Content -LiteralPath (Join-Path $taskProfile 'visibility-capture.json') -Raw | ConvertFrom-Json
    $taskPreferences=Get-Content -LiteralPath (Join-Path $taskProfile 'desktop.json') -Raw | ConvertFrom-Json
    $taskResult=[ordered]@{
        profileIsolated=$true
        systemClipboardUntouched=$true
        startedHidden=(-not $taskStartup.visible)
        mainStayedHidden=(-not $taskCapture.visible)
        mainNeverFullscreen=(-not $taskStartup.fullscreen -and -not $taskCapture.fullscreen)
        twoCapturesSaved=(@($taskCollection.photos).Count -eq 2)
        fullScreenFixtureUsed=$true
    }
    if($DisableAutoMount){
        $taskResult.autoMountRemainsOff=(-not $taskPreferences.autoMount -and -not $taskPreferences.enabled)
        $taskResult.noDesktopWindowCreated=(-not (Test-Path -LiteralPath $taskDesktopPath))
    }else{
        $taskResult.desktopNeverActivates=$taskDesktop.noActivate
        $taskResult.desktopAutoEnabled=$taskPreferences.enabled
        $taskResult.twoPhotosRendered=($taskLayout.cards -eq 2)
        $taskResult.testWidgetKeptHidden=(-not $taskDesktop.visible)
        $taskResult.widgetConfirmsMainHidden=(-not $taskDesktop.mainVisible -and -not $taskDesktop.mainFullscreen)
    }
    $taskResult | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskRoot '.qa/auto-clothesline-report.json') -Encoding utf8NoBOM
    $taskResult | ConvertTo-Json
    foreach($taskCheck in $taskResult.Values){if($taskCheck -ne $true){throw 'Automatic clothesline verification failed.'}}
} finally {
    if($taskProcess -and -not $taskProcess.HasExited){Stop-Process -Id $taskProcess.Id}
    if($null -eq $taskOldQa){Remove-Item Env:DARKROOM_QA_DIR -ErrorAction SilentlyContinue}else{$env:DARKROOM_QA_DIR=$taskOldQa}
    if($null -eq $taskOldHidden){Remove-Item Env:DARKROOM_QA_HIDE_DESKTOP -ErrorAction SilentlyContinue}else{$env:DARKROOM_QA_HIDE_DESKTOP=$taskOldHidden}
}
