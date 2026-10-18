param(
  [string]$Destination = "output/cli-release",
  [switch]$SkipBuild,
  [switch]$IncludeDesktop
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  $version = (Get-Content package.json -Raw | ConvertFrom-Json).version
  $destinationRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot $Destination))
  $outputRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'output')) + [IO.Path]::DirectorySeparatorChar
  if (-not $destinationRoot.StartsWith($outputRoot, [StringComparison]::OrdinalIgnoreCase)) { throw '分发产物必须位于本仓库 output 目录内' }
  $releaseDir = Join-Path $destinationRoot "v$version"
  if (Test-Path -LiteralPath $releaseDir) { throw "目标已存在，停止以保留已有产物：$releaseDir" }
  if (-not $SkipBuild) {
    & cargo build -p stillenough-cli --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'CLI release 构建失败' }
    if ($IncludeDesktop) {
      & pnpm build
      if ($LASTEXITCODE -ne 0) { throw '前端构建失败' }
      & cargo build -p stillenough --release --locked
      if ($LASTEXITCODE -ne 0) { throw '桌面 release 构建失败' }
    }
  }
  $exe = Join-Path $repoRoot 'target/release/stillenough-cli.exe'
  if (-not (Test-Path -LiteralPath $exe)) { throw 'CLI release 产物不存在' }
  $binaryVersion = & $exe --version
  if ($LASTEXITCODE -ne 0 -or $binaryVersion -ne "stillenough-cli $version") { throw 'CLI 二进制版本不匹配' }
  if ($IncludeDesktop -and -not (Test-Path -LiteralPath 'target/release/stillenough.exe')) { throw '桌面 release 产物不存在' }
  $stage = Join-Path $releaseDir 'StillEnough-CLI-windows-x64'
  New-Item -ItemType Directory -Path $stage -Force | Out-Null
  Copy-Item -LiteralPath $exe -Destination $stage
  Copy-Item -LiteralPath LICENSE -Destination $stage
  Copy-Item -LiteralPath THIRD_PARTY_NOTICES.md -Destination $stage
  Copy-Item -LiteralPath docs/CLI-IMPLEMENTATION.md -Destination $stage
  Copy-Item -LiteralPath docs/CLI.md -Destination (Join-Path $stage 'README.md')
  Copy-Item -LiteralPath docs/cli-response-v1.schema.json -Destination $stage
  [IO.File]::WriteAllText((Join-Path $stage 'stillenough-cli.cmd'), '@echo off' + "`r`n" + '@"%~dp0stillenough-cli.exe" %*' + "`r`n" + '@exit /b %errorlevel%' + "`r`n", [Text.Encoding]::ASCII)
  Compress-Archive -LiteralPath $stage -DestinationPath (Join-Path $releaseDir "StillEnough-CLI-v$version-windows-x64.zip")
  if ($IncludeDesktop) {
    $desktop = Join-Path $releaseDir 'StillEnough-windows-x64'
    New-Item -ItemType Directory -Path $desktop | Out-Null
    Copy-Item -LiteralPath 'target/release/stillenough.exe' -Destination $desktop
    Copy-Item -LiteralPath $exe -Destination $desktop
    Copy-Item -LiteralPath LICENSE -Destination $desktop
    Compress-Archive -LiteralPath $desktop -DestinationPath (Join-Path $releaseDir "StillEnough-v$version-windows-x64-with-cli.zip")
  }
  & node scripts/package-cli-npm.mjs $releaseDir
  if ($LASTEXITCODE -ne 0) { throw 'npm 本地打包失败' }
  $checksums = Get-ChildItem -LiteralPath $releaseDir -File | Where-Object { $_.Extension -in '.zip','.tgz' } | ForEach-Object { "{0}  {1}" -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name }
  [IO.File]::WriteAllLines((Join-Path $releaseDir 'SHA256SUMS.txt'), [string[]]$checksums, [Text.UTF8Encoding]::new($false))
  Write-Output "已生成本地分发产物：$releaseDir；未发布、未修改 PATH。"
} finally { Pop-Location }
