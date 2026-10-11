# Reference-only tooling for the original Profile7 dependency. No retargeting,
# downloaded archive fallback, runtime shims or global installation is performed.
param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
$framework = Join-Path $Output 'framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build'
if (-not (Test-Path "$framework/.NETFramework/v4.5/mscorlib.dll")) { throw 'Expected restored original net45 references' }
$installed = "${env:ProgramFiles(x86)}/Reference Assemblies/Microsoft/Framework/.NETPortable"
if (-not (Test-Path "$installed/v4.5/Profile/Profile7/mscorlib.dll")) {
    throw 'Original Microsoft Profile7 targeting references are required on this reference host'
}
Copy-Item $installed -Destination "$framework/.NETPortable" -Recurse
$files = @{}
Get-ChildItem "$framework/.NETPortable" -File -Recurse | ForEach-Object {
    $files[[IO.Path]::GetRelativePath($framework,$_.FullName)] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
@{ purpose = 'original-Profile7-reference-assemblies'; product_retargeted = $false;
   source = 'installed Microsoft reference assemblies'; files = $files
} | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $Output 'portable-references.json')
Write-Output "Prepared $($files.Count) original portable reference files from installed Microsoft reference assemblies"
