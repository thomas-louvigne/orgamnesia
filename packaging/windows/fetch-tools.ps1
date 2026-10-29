# Fetches what the Windows installer brings along, into src-tauri/windows-tools/
# (see src-tauri/tauri.windows.conf.json):
#
# - MinGit, the minimal Git for Windows made for apps to bundle, with Git
#   Credential Manager beside it: git then works on a computer without git, and
#   https identifiers are asked in a window (browser sign-in for GitHub, GitLab,
#   Bitbucket; name and password or token elsewhere, Codeberg…).
# - The French and English spelling dictionaries (Windows has no Hunspell ones).
#
# Every download is checked against its SHA-256: to update one, change its URL
# and its hash together.
#
# Run from the root of the repository:  pwsh packaging/windows/fetch-tools.ps1

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Out = Join-Path $PSScriptRoot '..\..\src-tauri\windows-tools'
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) 'orgamnesia-windows-tools'

$MinGit = @{
    Url    = 'https://github.com/git-for-windows/git/releases/download/v2.56.0.windows.4/MinGit-2.56.0.4-64-bit.zip'
    Sha256 = 'e3bf78351fe777fba47ddee5950082f8a98bb0f02604fdc7ba3ee2cb68ffa877'
}
$Gcm = @{
    Url    = 'https://github.com/git-ecosystem/git-credential-manager/releases/download/v3.0.2/gcm-win-x64-3.0.2.zip'
    Sha256 = '9d2bef6e42a37654ea7ef33172bbcf1b6a1df6191fcf6a57dfbf39e1cbc27a0f'
}
# LibreOffice's dictionaries, at a fixed commit; saved under the names the app looks for
$DictsUrl = 'https://raw.githubusercontent.com/LibreOffice/dictionaries/4f1df64b2e8e14751aa7c2205d170ca016071315'
$DictFiles = @(
    @{ Path = 'fr_FR/dictionaries/fr.aff';             Name = 'fr_FR.aff';          Sha256 = 'c176610cd5dc4846806a65ddd029f422d87978bf58f224aa44222662a16a2de5' }
    @{ Path = 'fr_FR/dictionaries/fr.dic';             Name = 'fr_FR.dic';          Sha256 = 'b78a868e31dd6e373b6c3217969afb898a9acde828a5e7ef97308da42218c88c' }
    @{ Path = 'fr_FR/dictionaries/README_dict_fr.txt'; Name = 'README_fr_FR.txt';   Sha256 = '4a25abf740ad6ba5e8d0cfc0d2ffadc3c13c303f8fe389f323b061a389350f83' }
    @{ Path = 'en/en_US.aff';                          Name = 'en_US.aff';          Sha256 = 'e746c882dd6f303c2c46e7452804b9201115a6942cfeb15f18f8edf774d2e24e' }
    @{ Path = 'en/en_US.dic';                          Name = 'en_US.dic';          Sha256 = 'f0b1a234bd178bdd01875b2a392a9647f888b8fe879f79c52aae62c2759b3647' }
    @{ Path = 'en/README_en_US.txt';                   Name = 'README_en_US.txt';   Sha256 = '168b4c01cb841f766a72f310b065c04d09cff46376c37a81d799593ce751c371' }
)

function Get-Checked([string]$Url, [string]$Sha256, [string]$File) {
    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $File -UseBasicParsing
    $got = (Get-FileHash -Algorithm SHA256 $File).Hash.ToLowerInvariant()
    if ($got -ne $Sha256) { throw "Checksum mismatch for ${Url}: expected $Sha256, got $got" }
}

Remove-Item -Recurse -Force $Out, $Tmp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $Out, $Tmp | Out-Null

# Git, and Git Credential Manager where Git for Windows puts it: git runs
# `git credential-manager` (credential.helper = manager) from there
$git = Join-Path $Out 'mingit'
Get-Checked $MinGit.Url $MinGit.Sha256 (Join-Path $Tmp 'mingit.zip')
Expand-Archive (Join-Path $Tmp 'mingit.zip') $git
Get-Checked $Gcm.Url $Gcm.Sha256 (Join-Path $Tmp 'gcm.zip')
$bin = Join-Path $git 'ucrt64\bin'
if (-not (Test-Path (Join-Path $bin 'git.exe'))) { throw "MinGit's layout changed: no ucrt64\bin\git.exe" }
Expand-Archive (Join-Path $Tmp 'gcm.zip') $bin -Force

# The pages stay as written, with Unix line ends, as Emacs, Linux and macOS
# write them: no conversion to CRLF on checkout (MinGit's default)
$config = Join-Path $git 'etc\gitconfig'
$text = Get-Content -Raw $config
if ($text -notmatch 'autocrlf = true') { throw "MinGit's gitconfig changed: no 'autocrlf = true'" }
if ($text -notmatch 'helper = manager') { throw "MinGit's gitconfig changed: no 'helper = manager'" }
Set-Content -NoNewline -Path $config -Value ($text -replace 'autocrlf = true', 'autocrlf = false')

$dicts = Join-Path $Out 'dictionaries'
New-Item -ItemType Directory -Force $dicts | Out-Null
foreach ($d in $DictFiles) {
    Get-Checked "$DictsUrl/$($d.Path)" $d.Sha256 (Join-Path $dicts $d.Name)
}

Remove-Item -Recurse -Force $Tmp
Write-Host "Windows tools ready in $Out"
