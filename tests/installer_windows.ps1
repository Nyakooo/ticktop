# Native Windows regression tests. No public download and no permanent PATH changes.
$ErrorActionPreference = 'Stop'
$installer = Join-Path $PSScriptRoot '..\scripts\install.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) ('ticktop-test-' + [Guid]::NewGuid().ToString('N'))
$fixture = Join-Path $root 'release'
[void][IO.Directory]::CreateDirectory($fixture)
$destination = Join-Path $root ('user space [test] ' + [char]0x6D4B + '\bin')
$asset = 'ticktop-x86_64-pc-windows-msvc.zip'
$originalUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$originalProcessPath = $env:Path
$originalVersion = $env:TICKTOP_VERSION
$env:TICKTOP_VERSION = 'latest'
$holding = $null
function Assert($Condition, $Message) { if (-not $Condition) { throw $Message } }
function Invoke-WebRequest {
    param([switch]$UseBasicParsing, [string]$Uri, [string]$OutFile, [int]$TimeoutSec)
    Copy-Item -LiteralPath (Join-Path $fixture ($Uri.Split('/')[-1])) -Destination $OutFile
}
function Write-Archive($EntryName = 'ticktop.exe') {
    $archivePath = Join-Path $fixture $asset
    if (Test-Path -LiteralPath $archivePath) { Remove-Item -LiteralPath $archivePath }
    $zip = [IO.Compression.ZipFile]::Open($archivePath, [IO.Compression.ZipArchiveMode]::Create)
    try { [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, (Join-Path $root 'fixture.exe'), $EntryName) }
    finally { $zip.Dispose() }
    $hash = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    [IO.File]::WriteAllText((Join-Path $fixture "$asset.sha256"), "$hash  $asset`n")
}
function Expect-Failure($Action) {
    $failed = $false
    try { & $Action } catch { $failed = $true; Write-Host ('Expected rejection: ' + $_.Exception.Message) }
    Assert $failed 'Expected installation to fail'
}
try {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    Add-Type -TypeDefinition @'
using System;
using System.Threading;
class TicktopFixture {
    static void Main(string[] args) {
        if (args.Length > 0 && args[0] == "--hold") { Thread.Sleep(60000); }
        else { Console.WriteLine("ticktop 0.3.0"); }
    }
}
'@ -OutputAssembly (Join-Path $root 'fixture.exe') -OutputType ConsoleApplication
    Write-Archive
    & $installer -InstallDir $destination -Version v0.3.0
    & $installer -InstallDir $destination -Version v0.3.0
    $exe = Join-Path $destination 'ticktop.exe'
    Assert ([IO.File]::Exists($exe)) 'Binary missing'
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    Assert (@($userPath.Split(';') | Where-Object { $_ -ieq $destination }).Count -eq 1) 'Duplicate user PATH entry'
    Assert (@($env:Path.Split(';') | Where-Object { $_ -ieq $destination }).Count -eq 1) 'Duplicate process PATH entry'
    Write-Host 'Passed: installation, pinned version, spaces/Unicode and idempotent user PATH'
    $before = (Get-FileHash -LiteralPath $exe).Hash
    $personal = Join-Path $root 'personal.json'
    [IO.File]::WriteAllText($personal, '{"mode":"minimal","groups":["personal"]}')
    $personalHash = (Get-FileHash -LiteralPath $personal).Hash
    $holding = Start-Process -FilePath $exe -ArgumentList '--hold' -NoNewWindow -PassThru
    Start-Sleep -Milliseconds 300
    & $installer -InstallDir $destination -NoPath
    Assert (-not $holding.HasExited) 'Old running process was interrupted'
    Assert ((Get-FileHash -LiteralPath $personal).Hash -eq $personalHash) 'Personal data changed'
    Stop-Process -Id $holding.Id -Force
    $holding.WaitForExit()
    $holding = $null
    & $installer -InstallDir $destination -NoPath
    Assert (@(Get-ChildItem -LiteralPath $destination -Force | Where-Object { $_.Name -like '.ticktop-old-*' }).Count -eq 0) 'Backup cleanup failed'
    Assert ([Environment]::GetEnvironmentVariable('Path', 'User') -ceq $userPath) 'NoPath changed user PATH'
    Write-Host 'Passed: in-use executable replacement, backup cleanup and personal data preservation'
    [IO.File]::WriteAllText((Join-Path $fixture "$asset.sha256"), ('0' * 64) + "  $asset`n")
    Expect-Failure { & $installer -InstallDir $destination }
    Assert ((Get-FileHash -LiteralPath $exe).Hash -eq $before) 'Checksum failure replaced old binary'
    Assert ([Environment]::GetEnvironmentVariable('Path', 'User') -ceq $userPath) 'Checksum failure changed PATH'
    Write-Archive '../ticktop.exe'
    Expect-Failure { & $installer -InstallDir $destination }
    Write-Archive
    Expect-Failure { & $installer -InstallDir $destination -Version '../../bad' }
    Expect-Failure { & $installer -InstallDir $destination -Version v99.0.0 }
    Assert ((Get-FileHash -LiteralPath $exe).Hash -eq $before) 'Invalid archive/version changed binary'
    Write-Host 'Passed: checksum, archive traversal and version rejection preserve old installation'
} finally {
    if ($holding -and -not $holding.HasExited) { Stop-Process -Id $holding.Id -Force }
    [Environment]::SetEnvironmentVariable('Path', $originalUserPath, 'User')
    $env:Path = $originalProcessPath
    $env:TICKTOP_VERSION = $originalVersion
    if (Test-Path -LiteralPath $root) { Remove-Item -LiteralPath $root -Recurse -Force }
}
