# Windows PowerShell 5.1+; installs only into the current user's directory.
[CmdletBinding()]
param(
    [string]$Version = $(if ($env:TICKTOP_VERSION) { $env:TICKTOP_VERSION } else { 'latest' }),
    [string]$InstallDir = $env:TICKTOP_INSTALL_DIR,
    [switch]$NoPath = ($env:TICKTOP_NO_PATH -eq '1')
)
& {
    param($Version, $InstallDir, $NoPath)
    $ErrorActionPreference = 'Stop'
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'Use install.sh on Linux/macOS.' }
    if ($Version -ne 'latest' -and $Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$') { throw 'Invalid version; use vX.Y.Z.' }
    $architecture = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $architecture = $env:PROCESSOR_ARCHITEW6432 }
    if ($architecture -ne 'AMD64') { throw 'This Windows release requires an x64 (Intel/AMD) operating system.' }
    if (-not $InstallDir) {
        if (-not $env:LOCALAPPDATA) { throw 'LOCALAPPDATA is unavailable; set TICKTOP_INSTALL_DIR.' }
        $InstallDir = Join-Path $env:LOCALAPPDATA 'Ticktop\bin'
    }
    if (-not [IO.Path]::IsPathRooted($InstallDir) -or $InstallDir -match '[\r\n;]') { throw 'Installation directory must be an absolute path without newlines or semicolons.' }
    $InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
    $asset = 'ticktop-x86_64-pc-windows-msvc.zip'
    $base = 'https://github.com/Nyakooo/ticktop/releases'
    if ($Version -eq 'latest') { $download = "$base/latest/download" } else { $download = "$base/download/$Version" }
    $work = Join-Path ([IO.Path]::GetTempPath()) ('ticktop-install-' + [Guid]::NewGuid().ToString('N'))
    [void][IO.Directory]::CreateDirectory($work)
    $staged = $null
    $backup = $null
    $installed = $false
    $previousTls = [Net.ServicePointManager]::SecurityProtocol
    function Get-ReleaseFile($Uri, $Destination) {
        try { Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination -TimeoutSec 120 }
        catch { throw 'Download failed. Check your network/proxy and that the release exists.' }
    }
    function Has-PathEntry($Value, $Directory) {
        foreach ($entry in ($Value -split ';')) {
            if ([Environment]::ExpandEnvironmentVariables($entry.Trim().Trim('"')).TrimEnd('\') -ieq $Directory) { return $true }
        }
        return $false
    }
    try {
        [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
        Write-Host "Downloading $asset ($Version)..."
        $archive = Join-Path $work $asset
        $checksum = Join-Path $work 'checksum'
        Get-ReleaseFile "$download/$asset" $archive
        Get-ReleaseFile "$download/$asset.sha256" $checksum
        $line = [IO.File]::ReadAllText($checksum).Trim()
        if ($line -notmatch '^([a-fA-F0-9]{64})\s+\*?([^\r\n]+)$') { throw 'Invalid checksum file.' }
        $expected = $Matches[1]
        if ($Matches[2] -cne $asset) { throw 'Checksum filename mismatch.' }
        if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ine $expected) { throw 'Checksum mismatch; existing installation was not changed.' }
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = [IO.Compression.ZipFile]::OpenRead($archive)
        try {
            if ($zip.Entries.Count -ne 1 -or $zip.Entries[0].FullName -cne 'ticktop.exe') { throw 'Unexpected archive contents.' }
            $entry = $zip.Entries[0]
            if ($entry.Length -lt 2 -or $entry.Length -gt 268435456 -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000 -or ($entry.ExternalAttributes -band 0x400)) { throw 'Expected a regular executable file.' }
            $extracted = Join-Path $work 'ticktop.exe'
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $extracted, $false)
        } finally { $zip.Dispose() }
        $stream = [IO.File]::OpenRead($extracted)
        try { if ($stream.ReadByte() -ne 0x4D -or $stream.ReadByte() -ne 0x5A) { throw 'Archive does not contain a Windows executable.' } }
        finally { $stream.Dispose() }
        $reported = (& $extracted --version | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $reported -notmatch '^ticktop ([0-9]+\.[0-9]+\.[0-9]+(?:[.-][a-zA-Z0-9.-]+)?)$') { throw 'Downloaded executable cannot run on this system.' }
        if ($Version -ne 'latest' -and $Matches[1] -cne $Version.Substring(1)) { throw 'Downloaded executable version mismatch.' }
        [void][IO.Directory]::CreateDirectory($InstallDir)
        # Closed backups from previous self-updates are safe to remove; a running
        # Windows image can be renamed but cannot be overwritten or deleted.
        Get-ChildItem -LiteralPath $InstallDir -Force -File | Where-Object { $_.Name -match '^\.ticktop-old-[a-f0-9]{32}\.exe$' } | ForEach-Object {
            try { [IO.File]::Delete($_.FullName) } catch { }
        }
        $destination = Join-Path $InstallDir 'ticktop.exe'
        $staged = Join-Path $InstallDir ('.ticktop-install-' + [Guid]::NewGuid().ToString('N') + '.exe')
        [IO.File]::Copy($extracted, $staged)
        $oldUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $oldProcessPath = $env:Path
        $pathChanged = $false
        try {
            if ([IO.File]::Exists($destination)) {
                $backup = Join-Path $InstallDir ('.ticktop-old-' + [Guid]::NewGuid().ToString('N') + '.exe')
                [IO.File]::Move($destination, $backup)
            }
            [IO.File]::Move($staged, $destination)
            $installed = $true
            if (-not $NoPath) {
                if (-not (Has-PathEntry $oldUserPath $InstallDir)) {
                    $newPath = if ($oldUserPath) { $oldUserPath.TrimEnd(';') + ';' + $InstallDir } else { $InstallDir }
                    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
                    $pathChanged = $true
                }
                if (-not (Has-PathEntry $env:Path $InstallDir)) { $env:Path = $env:Path.TrimEnd(';') + ';' + $InstallDir }
            }
        } catch {
            if ($pathChanged) { [Environment]::SetEnvironmentVariable('Path', $oldUserPath, 'User') }
            $env:Path = $oldProcessPath
            if ($installed) { [IO.File]::Delete($destination) }
            if ($backup -and [IO.File]::Exists($backup) -and -not [IO.File]::Exists($destination)) { [IO.File]::Move($backup, $destination) }
            throw 'Installation failed; the previous executable was preserved. Close running Ticktop instances and retry.'
        }
        if ($backup -and [IO.File]::Exists($backup)) {
            try { [IO.File]::Delete($backup) } catch { Write-Host 'Previous executable is still in use; its backup will be cleaned on a later installation.' }
        }
        Write-Host "Installed: $destination ($reported)"
        Write-Host 'Run: ticktop. Restart other terminal applications to refresh their PATH.'
        Write-Host 'In Git Bash/MinTTY, use: winpty ticktop'
    } finally {
        [Net.ServicePointManager]::SecurityProtocol = $previousTls
        if ($staged -and [IO.File]::Exists($staged)) { [IO.File]::Delete($staged) }
        if ([IO.Directory]::Exists($work)) { Remove-Item -LiteralPath $work -Recurse -Force }
    }
} $Version $InstallDir ([bool]$NoPath)
