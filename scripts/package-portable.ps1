param([string]$Destination = 'output/release')
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  $version = (Get-Content package.json -Raw | ConvertFrom-Json).version
  $destinationRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot $Destination))
  $outputRoot = [IO.Path]::GetFullPath((Join-Path $repoRoot 'output')) + [IO.Path]::DirectorySeparatorChar
  if (-not $destinationRoot.StartsWith($outputRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Destination must be inside repository output.' }
  $releaseDir = Join-Path $destinationRoot "v$version"
  $stage = Join-Path $releaseDir 'windows-x64'
  $zipName = "StillEnough-v$version-windows-x64.zip"
  $zipPath = Join-Path $releaseDir $zipName
  if ((Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $zipPath)) { throw 'Package already exists; refusing overwrite.' }
  $desktop = Get-Item -LiteralPath 'target/release/stillenough.exe'
  if ($desktop.VersionInfo.ProductVersion -ne $version) { throw 'Desktop version mismatch.' }
  $cliVersion = & ./target/release/stillenough-cli.exe version --json | ConvertFrom-Json
  if ($LASTEXITCODE -ne 0 -or $cliVersion.data.metadata.cliVersion -ne $version) { throw 'CLI version mismatch.' }
  foreach ($file in @('target/release/stillenough.exe', 'target/release/stillenough-cli.exe')) {
    $binary = [IO.File]::ReadAllBytes((Join-Path $repoRoot $file))
    $peOffset = [BitConverter]::ToInt32($binary, 0x3c)
    if ([BitConverter]::ToUInt16($binary, $peOffset + 4) -ne 0x8664) { throw 'Expected Windows x64 executable.' }
  }
  New-Item -ItemType Directory -Path $stage | Out-Null
  Copy-Item -LiteralPath $desktop.FullName -Destination (Join-Path $stage 'StillEnough.exe')
  Copy-Item -LiteralPath 'target/release/stillenough-cli.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md' -Destination $stage
  Copy-Item -LiteralPath 'public/providers/LICENSE' -Destination (Join-Path $stage 'LOBE_ICONS_LICENSE')
  Copy-Item -LiteralPath 'docs/CLI.md' -Destination (Join-Path $stage 'CLI.md')
  Copy-Item -LiteralPath 'docs/cli-response-v1.schema.json' -Destination $stage
  $readme = "StillEnough v$version - Windows x64`r`n`r`nRun StillEnough.exe for the desktop app; use stillenough-cli.exe for CLI / AI Agent access.`r`nRequires Microsoft Edge WebView2 Runtime (desktop) and the Microsoft VC runtime.`r`nExit the old version from the tray before upgrading. Existing local data is retained.`r`nEnable CLI permissions in the main window settings. See CLI.md for commands.`r`nNo PATH changes. No installer. No credentials are included.`r`n`r`nhttps://github.com/com1com4/StillEnough/releases/tag/v$version`r`n"
  [IO.File]::WriteAllText((Join-Path $stage 'README-Windows.txt'), $readme, [Text.UTF8Encoding]::new($false))
  $files = @('StillEnough.exe', 'stillenough-cli.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md', 'LOBE_ICONS_LICENSE', 'CLI.md', 'cli-response-v1.schema.json', 'README-Windows.txt')
  Compress-Archive -LiteralPath ($files | ForEach-Object { Join-Path $stage $_ }) -DestinationPath $zipPath -CompressionLevel Optimal
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $archive = [IO.Compression.ZipFile]::OpenRead($zipPath)
  try {
    if (Compare-Object ($files | Sort-Object) ($archive.Entries.FullName | Sort-Object)) { throw 'ZIP whitelist mismatch.' }
    foreach ($entry in $archive.Entries) {
      $stream = $entry.Open()
      $hasher = [Security.Cryptography.SHA256]::Create()
      try { $hash = ([BitConverter]::ToString($hasher.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
      finally { $stream.Dispose(); $hasher.Dispose() }
      $sourceHash = (Get-FileHash -LiteralPath (Join-Path $stage $entry.FullName) -Algorithm SHA256).Hash.ToLowerInvariant()
      if ($hash -ne $sourceHash) { throw 'ZIP content hash mismatch.' }
    }
  } finally { $archive.Dispose() }
  $zipHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash.ToLowerInvariant()
  [IO.File]::WriteAllText((Join-Path $releaseDir 'SHA256SUMS.txt'), "$zipHash  $zipName`n", [Text.Encoding]::ASCII)
  Write-Output "Verified portable ZIP: $zipPath"
} finally { Pop-Location }
