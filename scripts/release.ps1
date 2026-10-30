# 打发布压缩包：dist\scrcpy-desktop-<版本>-win64.zip（exe + scrcpy 运行时）
param([string]$OutDir = "dist")
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
$name = "scrcpy-desktop-$version-win64"

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build 失败" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$stage = Join-Path $OutDir $name
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }

New-Item -ItemType Directory -Force -Path "$stage\scrcpy" | Out-Null
Copy-Item target\release\scrcpy-desktop.exe $stage
Copy-Item scrcpy\* "$stage\scrcpy"
Copy-Item -Path README.md, LICENSE -Destination $stage

$zip = Join-Path $OutDir "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $stage -DestinationPath $zip
Remove-Item $stage -Recurse -Force

$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
"已生成 $zip（$size MB）"
