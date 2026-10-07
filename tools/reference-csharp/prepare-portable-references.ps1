# Reference-only tooling for the original Profile7 dependency. No product
# manifest changes, runtime shims or globally installed frameworks are made.
param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
$framework = Join-Path $Output 'framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build'
if (-not (Test-Path "$framework/.NETFramework/v4.5/mscorlib.dll")) { throw 'Expected restored original net45 references' }
$installed = "${env:ProgramFiles(x86)}/Reference Assemblies/Microsoft/Framework/.NETPortable"
$record = @{ purpose = 'original-Profile7-reference-assemblies'; product_retargeted = $false }
if (Test-Path "$installed/v4.5/Profile/Profile7/mscorlib.dll") {
    Copy-Item $installed -Destination "$framework/.NETPortable" -Recurse
    $record.source = 'installed Microsoft reference assemblies'
} else {
    # Official .NET team download, published in its 2013 PCL release announcement.
    # Fail on HTTP/download errors; no community binary replacement is used.
    $uri = 'https://aka.ms/portabledotnetrefpackage'
    $zip = Join-Path $Output 'portable-reference-assemblies.zip'
    Invoke-WebRequest -Uri $uri -OutFile $zip -TimeoutSec 90 -MaximumRedirection 5
    if ((Get-Item $zip).Length -gt 100MB) { throw 'Portable reference archive exceeds limit' }
    $record.source = $uri
    $record.archive_sha256 = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLowerInvariant()
    $unpacked = Join-Path $Output 'portable-references'
    Expand-Archive -Path $zip -DestinationPath $unpacked
    $candidate = @(Get-ChildItem $unpacked -Filter '.NETPortable' -Directory -Recurse | Where-Object { Test-Path (Join-Path $_.FullName 'v4.5/Profile/Profile7/mscorlib.dll') })
    if ($candidate.Count -ne 1) { throw 'No unique original Profile7 reference root in Microsoft archive' }
    Copy-Item $candidate[0].FullName -Destination "$framework/.NETPortable" -Recurse
}
$files = @{}
Get-ChildItem "$framework/.NETPortable" -File -Recurse | ForEach-Object {
    $files[[IO.Path]::GetRelativePath($framework,$_.FullName)] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$record.files = $files
$record | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $Output 'portable-references.json')
Write-Output "Prepared $($files.Count) original portable reference files from $($record.source)"
