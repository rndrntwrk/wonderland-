# Test-only runner against complete, separately built original DLLs. Only text
# traces, diagnostics and source/executable identities are retained as evidence.
param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference = 'Stop'
$src = Join-Path $Output 'source'
$bin = Join-Path $src 'TSOClient/tso.simantics/bin/Release'
$reference = Join-Path $Output 'framework/Microsoft.NETFramework.ReferenceAssemblies.net45.1.0.3/build/.NETFramework/v4.5'
$vswhere = "${env:ProgramFiles(x86)}/Microsoft Visual Studio/Installer/vswhere.exe"
$msbuild = & $vswhere -latest -requires Microsoft.Component.MSBuild -find MSBuild/**/Bin/MSBuild.exe | Select-Object -First 1
$csc = Join-Path (Split-Path $msbuild) 'Roslyn/csc.exe'
$baseArguments = @('/nologo','/target:exe','/platform:anycpu','/nostdlib+','/warn:4','/warnaserror+')
foreach ($name in @('mscorlib','System','System.Core')) { $baseArguments += "/reference:$reference/$name.dll" }
foreach ($name in @('FSO.SimAntics','FSO.LotView','FSO.Content','FSO.Files','FSO.Common','FSO.Vitaboy','FSO.Vitaboy.Engine','FSO.HIT')) {
    if (-not (Test-Path "$bin/$name.dll")) { throw "Missing real original assembly: $name" }
    $baseArguments += "/reference:$bin/$name.dll"
}
# The declared original MonoGame reference is Private=false in the projects.
# Supply that exact runtime dependency, not another package version or a shim.
$monogame = Join-Path $src 'TSOClient/packages/MonoGame.Framework.Portable.3.6.0.1625/lib/portable-net45+win8+wpa81/MonoGame.Framework.dll'
if (-not (Test-Path $monogame)) { throw 'Declared MonoGame dependency missing' }
Copy-Item $monogame "$bin/MonoGame.Framework.dll"
$baseArguments += "/reference:$bin/MonoGame.Framework.dll"
$source = Join-Path $src 'tools/reference-csharp/vm-bootstrap.cs'
$header = "tick`tobject_id`tattribute_0`tattribute_1`tattribute_2`tattribute_3`tstack_count`tclock_ticks`trng`tentities"
$seed = [System.Numerics.BigInteger]::Parse('1311768467463790320')
$record = @{
    schema = 1; passed = $false;
    scope = 'complete-original-assembly-with-explicit-test-content-provider';
    original_content_installation = 'not-tested'; native_wasm_equivalence = 'not-tested';
    full_game_parity = 'not-tested'; production_side_effects = 'not-in-cohort';
    source_bhav = 'Casino_2-Tile_Bar_CC.iff:4110';
    authored_metadata = '8x8 TS1 lot; one out-of-world object; original BHAV as Init/Main; initial [10,20,30,40]; pre-tick attributes [-tick,100+tick,30,200+tick]';
    product_source_rewritten = $false;
    launcher_runtime = [Runtime.InteropServices.RuntimeInformation]::FrameworkDescription;
    runs = @(); fault_controls = @();
}
function Run-Witness([string]$Executable,[string]$Label,[bool]$Enabled,[int]$ExpectedExit,[string]$ExpectedError,[int]$ExpectedRows) {
    $identityPath = Join-Path $Output "vm-trace-$Label.runtime.tsv"
    if ((Test-Path $identityPath) -or (Test-Path "$Output/vm-trace-$Label.tsv")) { throw 'Refusing to overwrite witness evidence' }
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Executable
    $info.ArgumentList.Add($src)
    $info.ArgumentList.Add($identityPath)
    $info.WorkingDirectory = $bin
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    if ($Enabled) { $info.Environment['WONDERLAND_ORIGINAL_VM_TEST_ONLY'] = '1' }
    else { [void]$info.Environment.Remove('WONDERLAND_ORIGINAL_VM_TEST_ONLY') }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw 'Original VM witness did not start' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(60000)) { $process.Kill($true); throw 'Original VM witness exceeded 60 second deadline' }
        $text = $stdout.GetAwaiter().GetResult().Replace("`r`n","`n")
        $errors = $stderr.GetAwaiter().GetResult()
        if ($text.Length -gt 1MB -or $errors.Length -gt 1MB) { throw 'Original VM output limit' }
        [IO.File]::WriteAllText("$Output/vm-trace-$Label.tsv",$text,[Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText("$Output/vm-trace-$Label.stderr",$errors,[Text.UTF8Encoding]::new($false))
        if ($process.ExitCode -ne $ExpectedExit) { Write-Host $errors; throw "Unexpected $Label exit $($process.ExitCode); required $ExpectedExit" }
        if (($ExpectedError -eq '' -and $errors.Length -ne 0) -or ($ExpectedError -ne '' -and -not $errors.Contains($ExpectedError))) {
            Write-Host $errors; throw "Unexpected original VM diagnostic for $Label"
        }
        $identity = @{}
        if (-not $Enabled) {
            if ($text.Length -ne 0 -or (Test-Path $identityPath)) { throw 'Unapproved witness executed beyond the opt-in gate' }
        } else {
            if (-not $text.EndsWith("`n")) { throw "Incomplete final record: $Label" }
            $rows = $text.TrimEnd("`n").Split("`n")
            if ($rows.Count -ne $ExpectedRows + 1 -or $rows[0] -ne $header) { throw "Incomplete/unexpected trace: $Label" }
            for ($i = 1; $i -le $ExpectedRows; $i++) {
                $rng = [System.Numerics.BigInteger]::Add($seed,[System.Numerics.BigInteger]$i).ToString()
                $expected = "$i`t1`t0`t0`t30`t0`t0`t$i`t$rng`t1"
                if ($rows[$i] -ne $expected) { throw "Original state mismatch: $Label row $i" }
            }
            foreach ($line in [IO.File]::ReadAllLines($identityPath)) {
                $parts = $line.Split("`t")
                if ($parts.Count -ne 2 -or $identity.ContainsKey($parts[0])) { throw 'Malformed runtime identity' }
                $identity[$parts[0]] = $parts[1]
            }
            if ($identity.Count -ne 9 -or $identity.schema -ne '1' -or $identity.clr -notmatch '^4\.0\.' -or
                $identity.pointer_bytes -notin @('4','8') -or $identity.vm_type -ne 'FSO.SimAntics.VM' -or
                $identity.thread_type -ne 'FSO.SimAntics.Engine.VMThread' -or $identity.provider -ne 'authored-in-memory-single-bhav') { throw 'Incorrect executing VM/runtime identity' }
            if ($identity.vm_sha256 -ne (Get-FileHash "$bin/FSO.SimAntics.dll" -Algorithm SHA256).Hash.ToLowerInvariant() -or
                $identity.witness_sha256 -ne (Get-FileHash $Executable -Algorithm SHA256).Hash.ToLowerInvariant() -or
                $identity.source_bhav_sha256 -ne '20b67e06940bfe0a89b689312fe001721ff3760cce10fca104373bf1122c4865') { throw 'Runtime loaded a different original/witness binary or resource' }
        }
        Write-Host "Verified $Label: exit $($process.ExitCode), $ExpectedRows accepted trace rows"
        return @{ label = $Label; exit = $process.ExitCode; expected_exit = $ExpectedExit; rows = $ExpectedRows;
            sha256 = (Get-FileHash "$Output/vm-trace-$Label.tsv" -Algorithm SHA256).Hash.ToLowerInvariant();
            diagnostic_sha256 = (Get-FileHash "$Output/vm-trace-$Label.stderr" -Algorithm SHA256).Hash.ToLowerInvariant();
            runtime_identity = $identity; expected_failure = $ExpectedError }
    } finally { $process.Dispose() }
}
try {
    foreach ($flavor in @('normal','skip-tick','deschedule')) {
        $exe = Join-Path $bin "swarm-f-original-vm-$flavor.exe"
        $arguments = $baseArguments + @("/out:$exe")
        if ($flavor -eq 'skip-tick') { $arguments += '/define:SWARM_F_FAULT_SKIP_TICK' }
        if ($flavor -eq 'deschedule') { $arguments += '/define:SWARM_F_FAULT_DESCHEDULE' }
        $arguments += $source
        & $csc @arguments 2>&1 | Tee-Object -FilePath "$Output/witness-compile.log" -Append
        if ($LASTEXITCODE) { throw "Witness compilation failed: $flavor" }
        if ($flavor -eq 'normal') {
            $record.runs += Run-Witness $exe '1' $true 0 '' 16
            $record.runs += Run-Witness $exe '2' $true 0 '' 16
            $record.fault_controls += Run-Witness $exe 'no-opt-in' $false 1 'Explicit test-only opt-in required' 0
        } elseif ($flavor -eq 'skip-tick') {
            $record.fault_controls += Run-Witness $exe $flavor $true 1 'Original VM driver/clock did not advance at tick 7' 6
        } else {
            $record.fault_controls += Run-Witness $exe $flavor $true 1 'Original scheduled BHAV tick 7 attribute result: -7,107,30,207' 6
        }
    }
    if ($record.runs[0].sha256 -ne $record.runs[1].sha256) { throw 'Fresh original VM executions differ' }
    $identities = @{}
    Get-ChildItem $bin -File | Where-Object { $_.Extension -in @('.dll','.exe') } | ForEach-Object {
        $identities[$_.Name] = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $record.executable_sha256 = $identities
    $record.passed = $true
    Get-Content "$Output/vm-trace-1.tsv"
} catch {
    $record.error = $_.Exception.Message
    throw
} finally {
    $record | ConvertTo-Json -Depth 10 | Set-Content "$Output/vm-execution.json"
}
