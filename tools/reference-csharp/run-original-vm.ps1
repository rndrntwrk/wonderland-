# Run a test-only witness against the complete, separately built original DLLs.
# The witness cannot authenticate users or write production effects. Only its
# text trace, command outputs and binary/source hashes are retained as evidence.
param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
$src = Join-Path $Output 'source'
$bin = Join-Path $src 'TSOClient/tso.simantics/bin/Release'
$reference = Join-Path $Output 'framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build/.NETFramework/v4.5'
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$msbuild = & $vswhere -latest -requires Microsoft.Component.MSBuild -find MSBuild/**/Bin/MSBuild.exe | Select-Object -First 1
$csc = Join-Path (Split-Path $msbuild) 'Roslyn/csc.exe'
$exe = Join-Path $bin 'swarm-f-original-vm.exe'
$arguments = @('/nologo','/target:exe','/platform:anycpu','/nostdlib+','/warn:4','/warnaserror+',"/out:$exe")
foreach ($name in @('mscorlib','System','System.Core')) { $arguments += "/reference:$reference/$name.dll" }
foreach ($name in @('FSO.SimAntics','FSO.LotView','FSO.Content','FSO.Files','FSO.Common','FSO.Vitaboy','FSO.Vitaboy.Engine','FSO.HIT')) {
    if (-not (Test-Path "$bin/$name.dll")) { throw "Missing real original assembly: $name" }
    $arguments += "/reference:$bin/$name.dll"
}
# Original projects mark this actual package reference Private=false. The test
# executable still needs the runtime binary; no alternate framework is invented.
$monogame = Join-Path $src 'TSOClient/packages/MonoGame.Framework.Portable.3.6.0.1625/lib/portable-net45+win8+wpa81/MonoGame.Framework.dll'
if (-not (Test-Path $monogame)) { throw 'Declared MonoGame dependency missing' }
Copy-Item $monogame "$bin/MonoGame.Framework.dll"
$arguments += "/reference:$bin/MonoGame.Framework.dll"
$arguments += (Join-Path $src 'tools/reference-csharp/vm-bootstrap.cs')
& $csc @arguments 2>&1 | Tee-Object -FilePath "$Output/witness-compile.log"
if ($LASTEXITCODE) { throw 'Original VM witness compilation failed' }
$identities = @{}
Get-ChildItem $bin -File | Where-Object { $_.Extension -in @('.dll','.exe') } | ForEach-Object {
    $identities[$_.Name] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
}
$results = @()
foreach ($index in @(1,2)) {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $exe
    $info.ArgumentList.Add($src)
    $info.WorkingDirectory = $bin
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.Environment['WONDERLAND_ORIGINAL_VM_TEST_ONLY'] = '1'
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    if (-not $process.Start()) { throw 'Original VM witness did not start' }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(60000)) { $process.Kill($true); throw 'Original VM witness exceeded 60 second deadline' }
    $text = $stdout.GetAwaiter().GetResult().Replace("`r`n","`n")
    $errors = $stderr.GetAwaiter().GetResult()
    if ($text.Length -gt 1MB -or $errors.Length -gt 1MB) { throw 'Original VM output limit' }
    [IO.File]::WriteAllText("$Output/vm-trace-$index.tsv",$text,[Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText("$Output/vm-trace-$index.stderr",$errors,[Text.UTF8Encoding]::new($false))
    if ($process.ExitCode -ne 0) { Write-Output $errors; throw "Original VM witness failed with exit $($process.ExitCode)" }
    if ($errors.Length -ne 0) { throw 'Unexpected original VM diagnostic output' }
    $rows = $text.TrimEnd("`n").Split("`n")
    if ($rows.Count -ne 17 -or $rows[0] -ne "tick`tobject_id`tattribute_2`tstack_count`tclock_ticks`trng`tentities") { throw 'Incomplete/unexpected original VM trace' }
    $results += @{ exit = $process.ExitCode; rows = 16; sha256 = (Get-FileHash "$Output/vm-trace-$index.tsv" -Algorithm SHA256).Hash.ToLowerInvariant() }
    Write-Output "Original VM execution $index passed; 16 complete scheduled ticks"
    $process.Dispose()
}
if ($results[0].sha256 -ne $results[1].sha256) { throw 'Original VM executions differ' }
@{
    schema = 1; passed = $true;
    scope = 'complete-original-assembly-with-explicit-test-content-provider';
    original_content_installation = 'not-tested'; native_wasm_equivalence = 'not-tested';
    source_bhav = 'Casino_2-Tile_Bar_CC.iff:4110';
    authored_metadata = '8x8 TS1 lot; single object; original BHAV bound as Init/Main; attribute reset before each tick';
    product_source_rewritten = $false; runs = $results; executable_sha256 = $identities;
    framework_runtime = [Runtime.InteropServices.RuntimeInformation]::FrameworkDescription
} | ConvertTo-Json -Depth 8 | Set-Content "$Output/vm-execution.json"
Get-Content "$Output/vm-trace-1.tsv"
