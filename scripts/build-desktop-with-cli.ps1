param([ValidateSet('nsis','none')][string]$Bundle = 'nsis')
$ErrorActionPreference = 'Stop'
$repoRoot=Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
  & cargo build -p stillenough-cli --release --locked
  if ($LASTEXITCODE -ne 0) { throw 'CLI release 构建失败' }
  $stage=Join-Path $repoRoot 'output/cli-sidecar'
  New-Item -ItemType Directory -Force -Path $stage | Out-Null
  Copy-Item -LiteralPath 'target/release/stillenough-cli.exe' -Destination (Join-Path $stage 'stillenough-cli-x86_64-pc-windows-msvc.exe')
  $config= @{ bundle=@{ externalBin=@((Join-Path $stage 'stillenough-cli')) } } | ConvertTo-Json -Depth 4 -Compress
  $configPath=Join-Path $stage 'bundle.override.json'
  [IO.File]::WriteAllText($configPath,$config,[Text.UTF8Encoding]::new($false))
  if ($Bundle -eq 'none') { & pnpm tauri build --no-bundle --config $configPath }
  else { & pnpm tauri build --bundles nsis --config $configPath }
  if ($LASTEXITCODE -ne 0) { throw '含 CLI 的桌面分发构建失败' }
} finally { Pop-Location }
