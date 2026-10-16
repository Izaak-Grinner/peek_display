# 配布用の zip を作る: リリースビルド → target/dist/peek_display にまとめる → zip
#   ./package.ps1             ビルドからやる
#   ./package.ps1 -SkipBuild  ビルド済みの exe を使う
param([switch]$SkipBuild)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$utf8 = New-Object System.Text.UTF8Encoding($false)

$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"(.+)"' | Select-Object -First 1).Matches.Groups[1].Value

if (-not $SkipBuild) {
    # 共有用ビルドは動的リンクを外す (Cargo.toml の dev feature)
    cargo build --release --no-default-features --manifest-path "$root\Cargo.toml"
    if ($LASTEXITCODE -ne 0) { throw "ビルドに失敗しました" }
}

$dist = "$root\target\dist"
$out = "$dist\peek_display"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force "$out\scenes" | Out-Null

Copy-Item "$root\target\release\peek_display.exe" $out
Copy-Item "$root\README.md" $out
Copy-Item "$root\scenes\*" "$out\scenes"
# release フォルダの固定のファイル (マーカー画像、リンク、案内)。ai_manual.md は下で README とつなげる
Get-ChildItem "$root\release" -File | Where-Object Name -ne "ai_manual.md" | Copy-Item -Destination $out

# AI用マニュアル = AI への指示と困ったときの対処 + README 全文 (1つ添付すれば済むように)
$manual = [IO.File]::ReadAllText("$root\release\ai_manual.md", $utf8) + [IO.File]::ReadAllText("$root\README.md", $utf8)
[IO.File]::WriteAllText("$out\AI用マニュアル.md", $manual, $utf8)

$zip = "$dist\peek_display_v$version.zip"
Compress-Archive -Path $out -DestinationPath $zip -Force
Write-Host "作成しました: $zip"
