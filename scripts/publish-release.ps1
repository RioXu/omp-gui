param(
    [string]$Repository = 'RioXu/omp-gui',
    [switch]$PrepareOnly
)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path $PSScriptRoot -Parent
Push-Location $repoRoot
try {
    function Invoke-Checked([string]$Command, [string[]]$Arguments) {
        & $Command @Arguments | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "$Command failed ($LASTEXITCODE)" }
    }
    function Assert-Source {
        if ((git branch --show-current) -ne 'main') { throw 'Release from main only' }
        if (git status --porcelain) { throw 'Commit all source changes before releasing' }
        if ((git rev-parse HEAD) -ne $head) { throw 'HEAD changed during release preparation' }
    }
    $head = git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read source commit' }
    Assert-Source
    $version = [regex]::Match((Get-Content Cargo.toml -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Expected a stable semantic version' }
    foreach ($manifest in @('crates/omp-core/Cargo.toml', 'crates/omp-remote/Cargo.toml')) {
        if ([regex]::Match((Get-Content $manifest -Raw), '(?m)^version = "([^"]+)"').Groups[1].Value -ne $version) { throw "Version mismatch: $manifest" }
    }
    $tag = "v$version"
    $notes = Join-Path $repoRoot "docs/releases/$tag.md"
    if (-not (Test-Path -LiteralPath $notes)) { throw "Missing release notes: $notes" }
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'Install GitHub CLI and run gh auth login to download Linux artifacts and publish' }
    Invoke-Checked gh @('auth', 'status')
    $remoteHead = & gh api "repos/$Repository/git/ref/heads/main" --jq '.object.sha'
    if ($LASTEXITCODE -ne 0 -or $remoteHead -ne $head) { throw 'Publish the source commit to GitHub main first' }
    if (-not $PrepareOnly) {
        $tags = & gh api "repos/$Repository/git/matching-refs/tags/$tag" --jq '.[].ref'
        if ($LASTEXITCODE -ne 0) { throw 'Cannot check release tags' }
        if ($tags -contains "refs/tags/$tag") { throw 'Release tag already exists; do not overwrite published versions' }
    }
    $output = Join-Path $repoRoot "artifacts/local-release/$version-$([guid]::NewGuid().ToString('N').Substring(0,8))"
    $remoteRoot = Join-Path $output 'remote'
    New-Item -ItemType Directory -Path (Join-Path $remoteRoot 'linux-x86_64') -Force | Out-Null
    Invoke-Checked cargo @('fmt', '--all', '--check')
    Invoke-Checked cargo @('clippy', '--workspace', '--locked', '--all-targets', '--all-features', '--', '-D', 'warnings')
    Invoke-Checked cargo @('test', '--workspace', '--locked')
    Invoke-Checked cargo @('build', '--locked', '--release', '-p', 'omp-gui')
    $deadline = [DateTime]::UtcNow.AddMinutes(10)
    while ($true) {
        $runId = & gh run list --repo $Repository --workflow check.yml --commit $head --branch main --limit 1 --json databaseId --jq '.[0].databaseId'
        if ($LASTEXITCODE -ne 0) { throw 'Cannot find the Linux build workflow' }
        $artifactId = $null
        if ($runId -match '^\d+$') {
            $artifactId = & gh api "repos/$Repository/actions/runs/$runId/artifacts" --jq '.artifacts[] | select(.name == "linux-server-x86_64" and .expired == false) | .id'
            if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect Linux build artifacts' }
        }
        if ($artifactId) { break }
        if ($runId -match '^\d+$') {
            $linuxConclusion = & gh run view $runId --repo $Repository --json jobs --jq '.jobs[] | select(.name | startswith("linux")) | .conclusion'
            if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect Linux build status' }
            if ($linuxConclusion -in @('failure', 'cancelled', 'skipped', 'timed_out')) { throw "Linux build did not succeed: $linuxConclusion" }
        }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'Timed out waiting for this commit Linux build; inspect GitHub Checks and retry' }
        Start-Sleep -Seconds 5
    }
    $linuxDirectory = Join-Path $remoteRoot 'linux-x86_64'
    Invoke-Checked gh @('run', 'download', $runId, '--repo', $Repository, '--name', 'linux-server-x86_64', '--dir', $linuxDirectory)
    if ((Get-Content -LiteralPath (Join-Path $linuxDirectory 'commit.txt') -Raw).Trim() -ne $head) { throw 'Linux service was built from another commit' }
    $protocol = [regex]::Match((Get-Content crates/omp-core/src/remote/protocol.rs -Raw), 'pub const VERSION:\s*u32\s*=\s*(\d+)').Groups[1].Value
    if (-not $protocol -or (Get-Content -LiteralPath (Join-Path $linuxDirectory 'version.txt') -Raw).Trim() -ne "omp-gui-remote/$version protocol/$protocol") { throw 'Linux service version or protocol mismatch' }
    $compiler = & (Join-Path $PSScriptRoot 'install-inno.ps1')
    $binary = Join-Path $repoRoot 'target/release/omp-gui.exe'
    $package = & (Join-Path $PSScriptRoot 'package-release.ps1') -Binary $binary -RemoteRoot $remoteRoot -OutputDirectory $output -InnoCompiler $compiler -PassThru
    # Verify an identical payload under a temporary application ID, preserving existing installations.
    $qaId = "RioXu.OMP.GUI.QA.$([guid]::NewGuid().ToString('N'))"
    $qaOutput = Join-Path $output 'install-verification'
    Invoke-Checked $compiler @("/DAppVersion=$version", "/DPayloadDir=$($package.Payload)", "/DOutputDir=$qaOutput", "/DInstallerAppId=$qaId", (Join-Path $PSScriptRoot 'installer.iss'))
    & (Join-Path $PSScriptRoot 'verify-installer.ps1') -Installer (Join-Path $qaOutput "omp-gui-$version-windows-x86_64-setup.exe") -Binary $binary -RemoteBinary (Join-Path $remoteRoot 'linux-x86_64/omp-gui-remote') -InstallerAppId $qaId
    Assert-Source
    @{commit=$head; version=$version; installer=$package.Installer; sha256=(Get-FileHash -LiteralPath $package.Installer).Hash} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $output 'build.json') -Encoding utf8
    if ($PrepareOnly) { Write-Output "Prepared locally: $($package.Installer)"; return }
    $remoteHead = & gh api "repos/$Repository/git/ref/heads/main" --jq '.object.sha'
    if ($LASTEXITCODE -ne 0 -or $remoteHead -ne $head) { throw 'GitHub main changed during preparation; release not published' }
    Invoke-Checked gh @('api', '--method', 'POST', "repos/$Repository/git/refs", '-f', "ref=refs/tags/$tag", '-f', "sha=$head")
    Invoke-Checked gh @('release', 'create', $tag, '--repo', $Repository, '--verify-tag', '--draft', '--title', "OMP GUI $tag", '--notes-file', $notes, $package.Installer, $package.Checksums)
    Invoke-Checked gh @('release', 'edit', $tag, '--repo', $Repository, '--draft=false', '--latest')
    Write-Output "Published: https://github.com/$Repository/releases/tag/$tag"
} finally {
    Pop-Location
}
