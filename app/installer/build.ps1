[CmdletBinding()]
param([string]$Compiler, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$appRoot = Split-Path $PSScriptRoot -Parent
$repoRoot = Split-Path $appRoot -Parent
if (!$Compiler) {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { $Compiler = $command.Source }
    else { $Compiler = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe' }
}
if (!(Test-Path -LiteralPath $Compiler -PathType Leaf)) { throw 'Install Inno Setup 6.7+ or pass -Compiler with the path to ISCC.exe.' }
$Compiler = (Resolve-Path -LiteralPath $Compiler).Path
Push-Location $appRoot
try {
    if (!$SkipBuild) {
        & cargo build --release --locked --bin kitlane
        if ($LASTEXITCODE -ne 0) { throw "Cargo build failed ($LASTEXITCODE)." }
    }
    $binary = Join-Path $appRoot 'target\release\kitlane.exe'
    if (!(Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing release binary: $binary" }
    $metadata = & cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $version = ($metadata.packages | Where-Object name -eq 'kitlane').version
    $artwork = Join-Path $appRoot 'target\installer-assets'
    New-Item -ItemType Directory -Force -Path $artwork | Out-Null
    Add-Type -AssemblyName System.Drawing
    $tokens = Get-Content (Join-Path $repoRoot 'design-system\tokens\tokens.json') -Raw | ConvertFrom-Json
    $icon = [Drawing.Image]::FromFile((Join-Path $repoRoot 'design-system\assets\icons\app-icon-128.png'))
    $font = [Drawing.Font]::new('Segoe UI', 20, [Drawing.FontStyle]::Bold, [Drawing.GraphicsUnit]::Pixel)
    $caption = [Drawing.Font]::new('Segoe UI', 14, [Drawing.FontStyle]::Regular, [Drawing.GraphicsUnit]::Pixel)
    try {
        foreach ($mode in @('light', 'dark')) {
            $bitmap = [Drawing.Bitmap]::new(328, 628)
            $graphics = [Drawing.Graphics]::FromImage($bitmap)
            $graphics.SmoothingMode = 'AntiAlias'
            $graphics.TextRenderingHint = 'AntiAliasGridFit'
            $background = if ($mode -eq 'dark') { $tokens.color.navy.'$value' } else { $tokens.color.pale.'$value' }
            $foreground = if ($mode -eq 'dark') { $tokens.color.white.'$value' } else { $tokens.color.ink.'$value' }
            $textBrush = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($foreground))
            $accentBrush = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml($tokens.color.cyan.'$value'))
            try {
                $graphics.Clear([Drawing.ColorTranslator]::FromHtml($background))
                $graphics.DrawImage($icon, 30, 44, 80, 80)
                $graphics.DrawString('KITLANE', $font, $textBrush, 30, 150)
                $graphics.FillRectangle($accentBrush, 30, 198, 56, 4)
                $graphics.DrawString("Your apps.`nYour devices.`nYour workspace.", $font, $textBrush, 30, 240)
                $graphics.DrawString("Local first.`nAlways in your control.", $caption, $textBrush, 30, 536)
                $bitmap.Save((Join-Path $artwork "welcome-$mode.png"), [Drawing.Imaging.ImageFormat]::Png)
            } finally { $accentBrush.Dispose(); $textBrush.Dispose(); $graphics.Dispose(); $bitmap.Dispose() }
        }
    } finally { $icon.Dispose(); $font.Dispose(); $caption.Dispose() }
    & $Compiler "/DAppVersion=$version" "/DBinaryPath=$binary" "/DArtworkPath=$artwork" (Join-Path $PSScriptRoot 'kitlane.iss')
    if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed ($LASTEXITCODE)." }
} finally { Pop-Location }
