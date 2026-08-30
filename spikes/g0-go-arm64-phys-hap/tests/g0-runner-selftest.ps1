<#
.SYNOPSIS
G0-PHYS-PROBE runner selftest (tests/g0-runner-selftest.ps1 1.0.0).

.DESCRIPTION
Host-only selftest for g0-phys-probe-campaign.ps1, mirroring
tests/g0-runner-selftest.py check-for-check (42 named tests, same names and
semantics). No real hdc, no device, no network: the target-binding paths run
against fake hdc executables installed inside a per-test temp sandbox; the
DryRun sentinel hdc (separate from the fake) writes a marker file if it is
EVER executed, proving --DryRun never touches the frozen hdc path.

Coverage (spec order):
  1. HDC whitelist: exact audit argv for all 15 operations; unknown operation /
     extra parameter / missing parameter / foreign bundle / illegal
     ForceStop Reason rejections; casefold aliases.
  2. PHYS_1_TARGET token: E3-verbatim positive/negative matrix incl. leading
     '-', embedded whitespace, 'PHYS-1', '<PHYS_1_TARGET>' placeholder; and
     assert-environment rejection via the process environment.
  3. Marker parsing / pre-registered verdict mapping: pass, dlopen-blocked
     (loaderError verbatim, loaderErrno=2), drift, duplicate, missing.
  4. Freeze validation: valid load; missing key; extra key; ready without a
     pass confirmation; review pass over a pending machine confirmation;
     runner_ps1_sha256 mismatch vs the actual runner file; declared-pass
     record hash mismatch; Live requires ready; frozen-value drift.
  5. TargetBindingConfirm via subprocess + fake hdc: pass record (double-file,
     single-use), pre-record gate exit 1, tuple drift exit 2 with a blocked
     record, invalid target token exit 2.
  6. Full -DryRun (G0_DRYRUN_SCRIPT=pass): exit 0, VERDICT=pass,
     is_evidence=false, seal binds scenario-results + hash-manifest, manifest
     hashes verify, transcript line chain recomputes, raw artifacts exist,
     target token never appears in any evidence/raw file, sentinel hdc not
     executed, host hdc comm count stays 0.
  7. Full -DryRun (dlopen-rejected): verdict=blocked with reason
     dlopen-blocked and the loader error preserved verbatim.
  8. Host HDC count probe: absolute, first-column-only, OS-adaptive
     (/usr/bin/ps comm column on Linux; tasklist.exe image-name column on
     Windows); no process with comm/image 'hdc' (the fake is fake-hdc).

Run: pwsh -NoProfile -File tests/g0-runner-selftest.ps1  (exit 0 iff every
check passes)
Output: PASS/FAIL per check, then TOTAL/PASSED/FAILED counts and
SELFTEST_RESULT=pass|fail.

NOTE: the runner is dot-sourced below for its pure functions. The runner's
param() block creates type-constrained variables named $Version, $SelfTest,
$TargetBindingConfirm, $DryRun, $Live, $Freeze and $ConfirmationRecord in this
scope; this file deliberately never assigns those variable names.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# =====================================================================
# Constants
# =====================================================================

$script:SelfTestPath = $PSCommandPath
$script:SelfTestDir = $PSScriptRoot
$script:G0Dir = Split-Path -Parent $script:SelfTestDir
$script:RunnerPath = Join-Path $script:G0Dir 'g0-phys-probe-campaign.ps1'
$script:PythonRunnerPath = Join-Path $script:G0Dir 'g0-phys-probe-campaign.py'
$script:PythonSelftestPath = Join-Path $script:SelfTestDir 'g0-runner-selftest.py'

$script:StAuthId = 'AUTH-G0PHYS1API26-20260830-0001'
$script:StCampaignId = 'G0-PHYS-PROBE-20260830-0001'
$script:StEvidenceId = 'EV-G0PHYS1API26-20260830-0001'
$script:StModel = 'PLA-AL10'
$script:StBuild = 'PLA-AL10 7.0.0.102(SP8C00E102R7P3)'
$script:StHdcVersion = 'SELFTEST-G0-HDC-1.0'
$script:StTargetToken = '192.168.1.100:5555'
$script:StDlopenError = 'initial-exec TLS resolves to dynamic definition'

# =====================================================================
# Import the runner (pure functions only; dot-sourcing never executes hdc)
# =====================================================================

. $script:RunnerPath

# =====================================================================
# Small helpers
# =====================================================================

function Write-StOut {
    param([string] $Text)
    [System.Console]::Out.WriteLine($Text)
}

function Get-StSha256File {
    param([string] $Path)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            return ([System.BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        } finally {
            $stream.Dispose()
        }
    } finally {
        $sha.Dispose()
    }
}

function Get-StSha256Text {
    param([string] $Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
        return ([System.BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Write-StText {
    # UTF-8 without BOM, no newline translation.
    param([string] $Path, [string] $Text)
    [System.IO.File]::WriteAllText($Path, $Text, [System.Text.UTF8Encoding]::new($false))
}

function Assert-St {
    param([bool] $Condition, [string] $Message)
    if (-not $Condition) {
        throw ('assertion failed: ' + $Message)
    }
}

function Test-StThrows {
    param([scriptblock] $Fn)
    try {
        & $Fn
        return $false
    } catch {
        return $true
    }
}

function Test-StArrayEqual {
    param([string[]] $Actual, [string[]] $Expected)
    if ($null -eq $Actual) { $Actual = [string[]] @() }
    if ($null -eq $Expected) { $Expected = [string[]] @() }
    if ($Actual.Count -ne $Expected.Count) { return $false }
    for ($i = 0; $i -lt $Actual.Count; $i++) {
        if ($Actual[$i] -cne $Expected[$i]) { return $false }
    }
    return $true
}

function New-StSandbox {
    param([string] $Label)
    $path = [System.IO.Path]::Combine(
        [System.IO.Path]::GetTempPath(),
        ('g0-selftest-{0}-{1}' -f $Label, [System.Guid]::NewGuid().ToString('N')))
    [void] [System.IO.Directory]::CreateDirectory($path)
    return $path
}

function Remove-StSandbox {
    param([string] $Path)
    try {
        if (-not [string]::IsNullOrEmpty($Path) -and [System.IO.Directory]::Exists($Path)) {
            Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
        }
    } catch { }
}

function Invoke-RunnerSubprocess {
    # Run the runner as a real subprocess (pwsh -NoProfile -File), with
    # PHYS_1_TARGET / G0_DRYRUN_SCRIPT stripped from the child environment
    # unless explicitly provided.
    param([string[]] $RunnerArgs, [hashtable] $EnvMap = @{}, [int] $TimeoutSeconds = 300)
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = [System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    foreach ($arg in @('-NoProfile', '-NonInteractive', '-File', $script:RunnerPath) + $RunnerArgs) {
        [void] $psi.ArgumentList.Add([string] $arg)
    }
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    [void] $psi.EnvironmentVariables.Remove('PHYS_1_TARGET')
    [void] $psi.EnvironmentVariables.Remove('G0_DRYRUN_SCRIPT')
    foreach ($key in @($EnvMap.Keys)) {
        $psi.EnvironmentVariables[[string] $key] = [string] $EnvMap[$key]
    }
    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdoutTask = $proc.StandardOutput.ReadToEndAsync()
    $stderrTask = $proc.StandardError.ReadToEndAsync()
    if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
        try { $proc.Kill($true) } catch { try { $proc.Kill() } catch { } }
        throw ('runner subprocess timed out: ' + ($RunnerArgs -join ' '))
    }
    return @{
        exit_code = [int] $proc.ExitCode
        stdout    = $stdoutTask.GetAwaiter().GetResult()
        stderr    = $stderrTask.GetAwaiter().GetResult()
    }
}

function Write-StGovernanceRecord {
    # Write a pass governance record (parseable JSON) and return path + sha.
    param([string] $Sandbox, [string] $Name, [string] $Kind)
    $path = Join-Path $Sandbox $Name
    $body = @{
        schema_version = 1
        record_kind    = $Kind
        verdict        = 'pass'
        campaign_id    = $script:StCampaignId
        evidence_id    = $script:StEvidenceId
    }
    Write-StText $path ((ConvertTo-Json -InputObject $body -Depth 10) + "`n")
    return @{ path = $path; sha256 = (Get-StSha256File $path) }
}

function New-StFreeze {
    # Build a schema-valid freeze in the sandbox and return its path. The
    # frozen hdc path is the MUST-NOT-START sentinel; DryRun must never run it.
    param(
        [string] $Sandbox,
        [string] $PlanStatus = 'blocked',
        [string] $RunnerPs1Sha = $null,
        [string] $Confirmation = 'pending',
        [string] $Review = 'pending',
        [switch] $Live)
    [void] [System.IO.Directory]::CreateDirectory($Sandbox)
    $hapPath = Join-Path $Sandbox 'app.hap'
    Write-StText $hapPath "G0-FAKE-HAP-BYTES`n"
    $sentinelPath = Join-Path $Sandbox 'hdc-sentinel.sh'
    Write-StText $sentinelPath (@'
#!/bin/sh
# G0 DryRun HDC-MUST-NOT-START sentinel: if the runner ever executes the
# frozen hdc path, a marker file appears next to this script.
printf executed > "$(dirname "$0")/SENTINEL-EXECUTED" 2>/dev/null || true
exit 0
'@)
    & chmod '755' $sentinelPath
    $confirmationPath = Join-Path $Sandbox 'confirmation-record.json'
    $reviewPath = Join-Path $Sandbox 'review-record.json'
    $confirmationSha = ('0' * 64)
    $reviewSha = ('0' * 64)
    if ($Confirmation -ceq 'pass') {
        $record = Write-StGovernanceRecord $Sandbox 'confirmation-record.json' 'g0-target-binding-confirmation'
        $confirmationPath = [string] $record['path']
        $confirmationSha = [string] $record['sha256']
    }
    if ($Review -ceq 'pass') {
        $record = Write-StGovernanceRecord $Sandbox 'review-record.json' 'g0-ready-freeze-review'
        $reviewPath = [string] $record['path']
        $reviewSha = [string] $record['sha256']
    }
    $runnerPs1Value = $RunnerPs1Sha
    # [string]-typed $null defaults arrive as '' (not $null): IsNullOrEmpty
    # catches both, so the default really computes the runner file hash.
    if ([string]::IsNullOrEmpty($runnerPs1Value)) { $runnerPs1Value = (Get-StSha256File $script:RunnerPath) }
    $selftestPyValue = ('0' * 64)
    if ([System.IO.File]::Exists($script:PythonSelftestPath)) {
        $selftestPyValue = (Get-StSha256File $script:PythonSelftestPath)
    }
    $runnerPyValue = ('0' * 64)
    if ([System.IO.File]::Exists($script:PythonRunnerPath)) {
        $runnerPyValue = (Get-StSha256File $script:PythonRunnerPath)
    }
    $doc = [ordered] @{
        schema_version          = 1
        authorization_id        = $script:StAuthId
        campaign_id             = $script:StCampaignId
        evidence_id             = $script:StEvidenceId
        attempt                 = 'initial'
        plan_status             = $PlanStatus
        code_sha                = ('a' * 40)
        runner_py_sha256        = $runnerPyValue
        runner_ps1_sha256       = $runnerPs1Value
        selftest_py_sha256      = $selftestPyValue
        selftest_ps1_sha256     = (Get-StSha256File $script:SelfTestPath)
        hdc                     = @{
            path    = $sentinelPath
            sha256  = (Get-StSha256File $sentinelPath)
            version = $script:StHdcVersion
        }
        bundle                  = $script:Bundle
        ability                 = $script:Ability
        module                  = $script:Module
        staging                 = $script:Staging
        hilog_tag               = $script:HilogTag
        scenario_window_seconds = 60
        target_tuple            = @{
            distribution         = 'HarmonyOS'
            device_model         = $script:StModel
            full_system_build    = $script:StBuild
            api                  = '26'
            kernel_architecture  = 'aarch64'
            app_abi              = 'arm64-v8a'
        }
        artifacts               = @{
            hap_path            = $hapPath
            hap_sha256          = (Get-StSha256File $hapPath)
            profile_sha256      = ('c' * 64)
            certificate_sha256  = ('d' * 64)
            libgoprobe_sha256   = ('e' * 64)
            libgoloader_sha256  = ('f' * 64)
        }
        elf_profile             = @{
            pt_tls          = $true
            tprel64_count   = 1
            static_tls_flag = $false
            needed          = @('libc.so')
        }
        evidence_roots          = @{
            dry_run = (Join-Path $Sandbox 'evidence-dry')
            live    = (Join-Path $Sandbox 'evidence-live')
        }
        raw_roots               = @{
            dry_run = (Join-Path $Sandbox 'raw-dry')
            live    = (Join-Path $Sandbox 'raw-live')
        }
        confirmation            = @{
            status          = $Confirmation
            record_path     = $confirmationPath
            record_sha256   = $confirmationSha
            authorization_id = $script:StAuthId
        }
        review                  = @{
            status        = $Review
            record_path   = $reviewPath
            record_sha256 = $reviewSha
        }
        operator                = 'authorized user'
        orchestrator            = 'main agent'
    }
    $freezePath = Join-Path $Sandbox $(if ($Live) { 'freeze-live.json' } else { 'freeze.json' })
    Write-StText $freezePath ((ConvertTo-Json -InputObject $doc -Depth 20) + "`n")
    return $freezePath
}

function Read-StFreezeDoc {
    param([string] $FreezePath)
    return (([System.IO.File]::ReadAllText($FreezePath)) | ConvertFrom-Json)
}

function Write-StFreezeDoc {
    param([string] $FreezePath, $Doc)
    Write-StText $FreezePath ((ConvertTo-Json -InputObject $Doc -Depth 20) + "`n")
}

function Set-StFreezeHdc {
    # Point an existing freeze at a new hdc executable (hash recomputed).
    param([string] $FreezePath, [string] $HdcPath)
    $doc = Read-StFreezeDoc $FreezePath
    $doc.hdc = [pscustomobject] @{
        path    = $HdcPath
        sha256  = (Get-StSha256File $HdcPath)
        version = $script:StHdcVersion
    }
    Write-StFreezeDoc $FreezePath $doc
}

function Install-StFakeHdc {
    param([string] $Sandbox, [string] $Name, [string] $ScriptText)
    $path = Join-Path $Sandbox $Name
    Write-StText $path $ScriptText
    & chmod '755' $path
    return $path
}

$script:StTbcFakeHdcScript = ('#!/bin/sh
# G0 TargetBindingConfirm fake hdc: fixture answers for the three
# target-binding probes (Version / TupleModel / TupleBuild).
if [ "$1" = "-t" ]; then shift 2; fi
case "$1" in
  version)
    echo "OpenHarmony 3.2.0 {0}"
    exit 0
    ;;
  shell)
    if [ "$2" = "param" ] && [ "$4" = "const.product.model" ]; then
      echo "{1}"
      exit 0
    fi
    if [ "$2" = "param" ] && [ "$4" = "const.product.software.version" ]; then
      echo "{2}"
      exit 0
    fi
    ;;
esac
echo "tbc-fake-hdc: unsupported argv" >&2
exit 9
' -f $script:StHdcVersion, $script:StModel, $script:StBuild)

$script:StTbcFakeHdcDriftScript = $script:StTbcFakeHdcScript.Replace(
    ('echo "{0}"' -f $script:StModel), 'echo "WRONG-MODEL-9"')

# ---- marker fixtures ------------------------------------------------------------

$script:StPassMarker = ('2026-08-30 12:00:00.100  12345  67890 I G0GoProbe: ' +
    'G0_RESULT|verdict=PASS|ok=true|pid=12345|stage=complete|dlopenLoaded=true|' +
    'loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576')
$script:StDlopenMarker = ('2026-08-30 12:00:00.100  12345  67890 E G0GoProbe: ' +
    'G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|' +
    'loaderErrno=2|loaderError=' + $script:StDlopenError + '|hello=0|runtimeBytes=0')

# ---- transcript independent recompute ------------------------------------------

function Get-StTranscriptRecompute {
    # Independent recompute of line_sha256 = sha256(prev + canonical json of
    # the line without line_sha256); returns (violations, chain_head). Raw
    # token bytes are taken with System.Text.Json so the ISO-8601 ts string is
    # re-hashed verbatim.
    param([string] $TranscriptPath)
    $violations = [System.Collections.Generic.List[string]]::new()
    $prev = ('0' * 64)
    $expectedSeq = [long] 1
    $head = $prev
    foreach ($rawLine in [System.IO.File]::ReadLines($TranscriptPath)) {
        $line = ([string] $rawLine).TrimEnd("`n").TrimEnd("`r")
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $doc = [System.Text.Json.JsonDocument]::Parse($line)
        try {
            $root = $doc.RootElement
            $prevRaw = $root.GetProperty('prev_line_sha256').GetRawText()
            $lineShaRaw = $root.GetProperty('line_sha256').GetRawText()
            $core = '{"details":' + $root.GetProperty('details').GetRawText() `
                + ',"event":' + $root.GetProperty('event').GetRawText() `
                + ',"prev_line_sha256":' + $prevRaw `
                + ',"seq":' + $root.GetProperty('seq').GetRawText() `
                + ',"ts":' + $root.GetProperty('ts').GetRawText() + '}'
            $seqValue = [int64] ($root.GetProperty('seq').GetInt64())
        } finally {
            $doc.Dispose()
        }
        $storedPrev = $prevRaw.Trim('"')
        $storedLineSha = $lineShaRaw.Trim('"')
        Assert-St ($storedPrev -ceq $prev) ('transcript prev-hash break at seq ' + $seqValue)
        Assert-St ($seqValue -eq $expectedSeq) 'transcript seq order break'
        if ((Get-StSha256Text ($storedPrev + $core)) -cne $storedLineSha) {
            $violations.Add([string] $seqValue)
        }
        $prev = $storedLineSha
        $head = $prev
        $expectedSeq++
    }
    return @{ violations = $violations.ToArray(); head = $head }
}

function Assert-StNoTargetTokenAnywhere {
    param([string[]] $Roots)
    foreach ($base in $Roots) {
        foreach ($file in @(Get-ChildItem -LiteralPath $base -Recurse -File -Force)) {
            $body = [System.IO.File]::ReadAllText($file.FullName)
            Assert-St (-not $body.Contains($script:StTargetToken)) ('target token leaked into ' + $file.FullName)
        }
    }
}

# =====================================================================
# Test registry
# =====================================================================

$script:StTestRegistry = [System.Collections.Generic.List[object]]::new()

function Register-StTest {
    param([string] $Name, [scriptblock] $Body)
    $script:StTestRegistry.Add(@{ name = $Name; body = $Body })
}

# =====================================================================
# 1. whitelist
# =====================================================================

Register-StTest 'whitelist-exact-audit-argv-all-15-operations' {
    $target = $script:TargetPlaceholder
    $expected = [ordered] @{
        'Version'        = [string[]] @('version')
        'TupleModel'     = [string[]] @('-t', $target, 'shell', 'param', 'get', 'const.product.model')
        'TupleBuild'     = [string[]] @('-t', $target, 'shell', 'param', 'get', 'const.product.software.version')
        'BundleDump'     = [string[]] @('-t', $target, 'shell', 'bm', 'dump', '-n', $script:Bundle)
        'PidOf'          = [string[]] @('-t', $target, 'shell', 'pidof', $script:Bundle)
        'MkdirStaging'   = [string[]] @('-t', $target, 'shell', 'mkdir', '-p', ($script:Staging + '/hap'))
        'SendHap'        = [string[]] @('-t', $target, 'file', 'send', $script:HapPlaceholder, ($script:Staging + '/hap/g0.hap'))
        'InstallHap'     = [string[]] @('-t', $target, 'shell', 'bm', 'install', '-p', ($script:Staging + '/hap'))
        'StartEntry'     = [string[]] @('-t', $target, 'shell', 'aa', 'start', '-a', $script:Ability, '-b', $script:Bundle, '-m', $script:Module)
        'HilogStream'    = [string[]] @('-t', $target, 'shell', 'hilog', '-T', $script:HilogTag, '-v', 'year', '-v', 'zone')
        'FaultProbe'     = [string[]] @('-t', $target, 'shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', ('*{0}*' -f $script:Bundle), '-print')
        'ForceStop'      = [string[]] @('-t', $target, 'shell', 'aa', 'force-stop', $script:Bundle)
        'Uninstall'      = [string[]] @('-t', $target, 'shell', 'bm', 'uninstall', '-n', $script:Bundle)
        'RemoveStaging'  = [string[]] @('-t', $target, 'shell', 'rm', '-rf', $script:Staging)
        'StagingProbe'   = [string[]] @('-t', $target, 'shell', 'ls', '-ld', $script:Staging)
    }
    Assert-St (@($script:HdcWhitelist.Keys).Count -eq 15) 'whitelist must hold exactly 15 operations'
    foreach ($operation in $expected.Keys) {
        $parameters = @{}
        if ([string[]] $script:HdcWhitelist[$operation] -ccontains 'Bundle') { $parameters['Bundle'] = $script:Bundle }
        if ([string[]] $script:HdcWhitelist[$operation] -ccontains 'Reason') { $parameters['Reason'] = 'final-cleanup' }
        [string[]] $argv = Get-G0HdcInvocation -Operation ([string] $operation) -Parameters $parameters
        Assert-St (Test-StArrayEqual $argv ([string[]] $expected[$operation])) ('audit argv mismatch: ' + $operation)
    }
    # Audit form never leaks a real target: the placeholder is verbatim.
    Assert-St ($script:TargetPlaceholder -cne $script:StTargetToken) 'placeholder must differ from a real token'
}

Register-StTest 'whitelist-rejects-unknown-operation' {
    foreach ($operation in [string[]] @('Screenshot', 'ShellRaw', 'ListTargets', 'version!', '')) {
        Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation $operation }) ('unknown operation must be rejected: ' + $operation)
    }
}

Register-StTest 'whitelist-rejects-extra-parameter' {
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'Version' -Parameters @{ Bundle = $script:Bundle } }) 'Version must not accept Bundle'
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'TupleModel' -Parameters @{ Reason = 'x' } }) 'TupleModel must not accept Reason'
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'BundleDump' -Parameters @{ Bundle = $script:Bundle; Extra = '1' } }) 'BundleDump must not accept Extra'
}

Register-StTest 'whitelist-rejects-missing-parameter' {
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'PidOf' }) 'PidOf without Bundle must be rejected'
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'StartEntry' -Parameters @{ Bundle = '' } }) 'StartEntry with empty Bundle must be rejected'
    Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'ForceStop' -Parameters @{ Bundle = $script:Bundle } }) 'ForceStop without Reason must be rejected'
}

Register-StTest 'whitelist-rejects-foreign-bundle' {
    foreach ($operation in [string[]] @('BundleDump', 'PidOf', 'StartEntry', 'Uninstall', 'ForceStop')) {
        $parameters = @{ Bundle = 'cn.example.foreign' }
        if ($operation -ceq 'ForceStop') { $parameters['Reason'] = 'final-cleanup' }
        $capturedOperation = [string] $operation
        $capturedParameters = $parameters
        Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation $capturedOperation -Parameters $capturedParameters }) ('foreign bundle must be rejected: ' + $operation)
    }
}

Register-StTest 'whitelist-rejects-forcestop-illegal-reason' {
    foreach ($reason in [string[]] @('reboot', 'cleanup', '', 'exception-Cleanup', 'final-cleanup ')) {
        $capturedReason = [string] $reason
        Assert-St (Test-StThrows { Get-G0HdcInvocation -Operation 'ForceStop' -Parameters @{ Bundle = $script:Bundle; Reason = $capturedReason } }) ('illegal force-stop reason must be rejected: ' + $reason)
    }
    foreach ($reason in [string[]] $script:ForceStopReasons) {
        [string[]] $argv = Get-G0HdcInvocation -Operation 'ForceStop' -Parameters @{ Bundle = $script:Bundle; Reason = $reason }
        Assert-St ($argv[$argv.Count - 1] -ceq $script:Bundle) 'force-stop argv must end with the bundle'
        Assert-St ($argv -ccontains 'force-stop') 'force-stop argv must contain force-stop'
    }
}

Register-StTest 'whitelist-case-insensitive-aliases' {
    Assert-St ((ConvertTo-G0HdcOperation 'bundledump') -ceq 'BundleDump') 'lowercase alias must normalize'
    Assert-St ((ConvertTo-G0HdcOperation 'FORCESTOP') -ceq 'ForceStop') 'uppercase alias must normalize'
    [string[]] $argvLower = Get-G0HdcInvocation -Operation 'pidof' -Parameters @{ bundle = $script:Bundle }
    [string[]] $argvUpper = Get-G0HdcInvocation -Operation 'PIDOF' -Parameters @{ BUNDLE = $script:Bundle }
    $expected = [string[]] @('-t', $script:TargetPlaceholder, 'shell', 'pidof', $script:Bundle)
    Assert-St (Test-StArrayEqual $argvLower $expected) 'lowercase argv mismatch'
    Assert-St (Test-StArrayEqual $argvUpper $expected) 'uppercase argv mismatch'
}

# =====================================================================
# 2. target token
# =====================================================================

Register-StTest 'target-token-accepts-real-tokens' {
    foreach ($token in [string[]] @('192.168.1.100:5555', 'aabbccdd.eeff00112233', 'emulator-5554X')) {
        Assert-St (Test-G0PhysicalTargetToken $token) ('real token must be accepted: ' + $token)
    }
}

Register-StTest 'target-token-rejects-leading-dash' {
    foreach ($token in [string[]] @('-t', '--flag', '-192.168.1.100')) {
        Assert-St (-not (Test-G0PhysicalTargetToken $token)) ('leading dash must be rejected: ' + $token)
    }
}

Register-StTest 'target-token-rejects-whitespace' {
    foreach ($token in [string[]] @('', '   ', ' x', 'x ', 'a b', "a`tb", 'a,b', 'a;b')) {
        Assert-St (-not (Test-G0PhysicalTargetToken $token)) ('whitespace token must be rejected: [' + $token + ']')
    }
    $newlineToken = "a`nb"
    Assert-St (-not (Test-G0PhysicalTargetToken $newlineToken)) 'embedded newline token must be rejected'
    $nullToken = [string] $null
    Assert-St (-not (Test-G0PhysicalTargetToken $nullToken)) 'null token must be rejected'
}

Register-StTest 'target-token-rejects-phys-1-and-placeholders' {
    foreach ($token in [string[]] @('PHYS-1', 'phys-1', 'Phys-1', '<PHYS_1_TARGET>', '<anything>', '<T>')) {
        Assert-St (-not (Test-G0PhysicalTargetToken $token)) ('PHYS-1/placeholder must be rejected: ' + $token)
    }
}

Register-StTest 'assert-target-environment-rejects-invalid-env-values' {
    $saved = [System.Environment]::GetEnvironmentVariable('PHYS_1_TARGET')
    try {
        foreach ($bad in @($null, '', '   ', ' padded', '-leading', 'with space', 'a,b', 'a;b', 'PHYS-1', '<PHYS_1_TARGET>')) {
            if ($null -eq $bad) {
                [System.Environment]::SetEnvironmentVariable('PHYS_1_TARGET', $null)
            } else {
                $env:PHYS_1_TARGET = [string] $bad
            }
            Assert-St (Test-StThrows { Assert-G0TargetEnvironment }) ('invalid env value must be rejected: [' + $bad + ']')
        }
        $env:PHYS_1_TARGET = $script:StTargetToken
        Assert-G0TargetEnvironment
        Assert-St ($script:ActualTarget -ceq $script:StTargetToken) 'actual target must capture the env token'
    } finally {
        if ($null -eq $saved) {
            [System.Environment]::SetEnvironmentVariable('PHYS_1_TARGET', $null)
        } else {
            $env:PHYS_1_TARGET = [string] $saved
        }
        $script:ActualTarget = $null
    }
}

# =====================================================================
# 3. marker parsing / verdict mapping
# =====================================================================

Register-StTest 'marker-pass-classification' {
    $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($script:StPassMarker))
    Assert-St (($mapping['verdict'] -ceq 'pass') -and ($null -eq $mapping['fail_reason'])) 'pass marker must classify as pass'
    $fields = $mapping['fields']
    Assert-St (([string] $fields['hello']) -ceq '42') 'hello field must survive verbatim'
    Assert-St (([string] $fields['runtimeBytes']) -ceq '1048576') 'runtimeBytes field must survive verbatim'
    Assert-St (([string] $fields['stage']) -ceq 'complete') 'stage field mismatch'
    Assert-St (([string] $fields['ok']) -ceq 'true') 'ok field mismatch'
}

Register-StTest 'marker-dlopen-blocked-classification-verbatim-loader-error' {
    $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($script:StDlopenMarker))
    Assert-St (($mapping['verdict'] -ceq 'blocked') -and ($mapping['fail_reason'] -ceq 'dlopen-blocked')) 'dlopen marker must classify as dlopen-blocked'
    $fields = $mapping['fields']
    Assert-St (([string] $fields['loaderError']) -ceq $script:StDlopenError) 'loaderError must be preserved verbatim'
    Assert-St (([string] $fields['loaderErrno']) -ceq '2') 'loaderErrno must be preserved verbatim'
    # A FAIL@dlopen marker with an EMPTY loaderError is drift, not a valid result.
    $emptyLine = $script:StDlopenMarker.Replace(('loaderError=' + $script:StDlopenError), 'loaderError=')
    $empty = Get-G0MarkerMapping -MarkerLines ([object[]] @($emptyLine))
    Assert-St (($empty['verdict'] -ceq 'blocked') -and ($empty['fail_reason'] -ceq 'drift')) 'empty loaderError must be drift'
}

Register-StTest 'marker-drift-classification' {
    $cases = [string[]] @(
        'x G0_RESULT|verdict=DRIFT|ok=false|pid=1|stage=complete',
        $script:StPassMarker.Replace('hello=42', 'hello=7'),
        $script:StPassMarker.Replace('runtimeBytes=1048576', 'runtimeBytes=2048'),
        'x G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=native-throw|loaderErrno=0|loaderError=boom',
        'x G0_RESULT|verdict=FAIL|ok=false|stage=dlopen|loaderErrno=2|loaderError=',
        'x G0_RESULT|verdict=PASS|ok=false|stage=complete|hello=42|runtimeBytes=1048576'
    )
    foreach ($caseLine in $cases) {
        $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($caseLine))
        Assert-St (($mapping['verdict'] -ceq 'blocked') -and ($mapping['fail_reason'] -ceq 'drift')) ('drift classification mismatch: ' + $caseLine)
    }
}

Register-StTest 'marker-key-case-drift-classification' {
    # Field keys are matched case-sensitively: a different-case key is a
    # distinct field and must never satisfy or override the exact-case
    # pre-registered lookups (review BLOCKER-2 regression matrix).
    $cases = [string[]] @(
        $script:StPassMarker.Replace('verdict=PASS', 'Verdict=PASS'),
        $script:StPassMarker.Replace('ok=true', 'OK=true'),
        ('x G0_RESULT|VERDICT=FAIL|ok=false|pid=0|STAGE=dlopen|dlopenLoaded=false|' +
         'loaderErrno=2|LOADERERROR=boom|hello=0|runtimeBytes=0'),
        'x G0_RESULT|verdict=FAIL|Verdict=PASS',
        'x G0_RESULT|verdict=PASS|verdict=FAIL'
    )
    foreach ($caseLine in $cases) {
        $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($caseLine))
        Assert-St ($mapping['verdict'] -ceq 'blocked') ('case-drift must be blocked: ' + $caseLine)
        Assert-St ($mapping['fail_reason'] -ceq 'drift') ('case-drift must be drift: ' + $caseLine + ' -> ' + [string] $mapping['fail_reason'])
    }
}

Register-StTest 'marker-missing-classification' {
    $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @())
    Assert-St (($mapping['verdict'] -ceq 'blocked') -and ($mapping['fail_reason'] -ceq 'marker-missing')) 'zero markers must be marker-missing'
}

Register-StTest 'marker-ambiguous-classification' {
    foreach ($lines in @([object[]] @($script:StPassMarker, $script:StPassMarker), [object[]] @($script:StPassMarker, $script:StDlopenMarker))) {
        $mapping = Get-G0MarkerMapping -MarkerLines $lines
        Assert-St (($mapping['verdict'] -ceq 'blocked') -and ($mapping['fail_reason'] -ceq 'marker-ambiguous')) 'duplicate markers must be marker-ambiguous'
    }
}

Register-StTest 'marker-field-parser-handles-spaced-values' {
    $fields = ConvertFrom-G0MarkerLine -Line $script:StDlopenMarker
    Assert-St (([string] $fields['loaderError']) -ceq $script:StDlopenError) 'parser must keep the spaced loader error'
    Assert-St (([string] $fields['verdict']) -ceq 'FAIL') 'parser verdict field mismatch'
    Assert-St (([string] $fields['stage']) -ceq 'dlopen') 'parser stage field mismatch'
    # parser only consumes text from the marker token onward
    $noMarker = ConvertFrom-G0MarkerLine -Line 'prefix G0GoProbe: 2026-08-30 12:00:00'
    Assert-St (@($noMarker.Keys).Count -eq 0) 'parser must return empty without the marker token'
    $noise = ConvertFrom-G0MarkerLine -Line 'noise G0_RESULT|verdict=PASS'
    Assert-St (([string] $noise['verdict']) -ceq 'PASS') 'parser must start at the marker token'
}

# =====================================================================
# 4. freeze validation
# =====================================================================

Register-StTest 'freeze-valid-blocked-and-ready-load' {
    $sandbox = New-StSandbox 'freeze-ok'
    try {
        $blocked = New-StFreeze (Join-Path $sandbox 'b') -PlanStatus 'blocked'
        $doc = Load-G0Freeze -FreezePath $blocked -Mode 'dry-run' -RepoRootPath $null
        Assert-St ((Get-G0ObjectValue $doc 'bundle') -ceq $script:Bundle) 'blocked freeze must load'
        $ready = New-StFreeze (Join-Path $sandbox 'r') -PlanStatus 'ready' -Confirmation 'pass' -Review 'pass'
        $doc = Load-G0Freeze -FreezePath $ready -Mode 'dry-run' -RepoRootPath $null
        Assert-St ((Get-G0ObjectValue $doc 'plan_status') -ceq 'ready') 'ready freeze must load'
        # Live mode requires ready; ready freeze passes the live gate.
        Load-G0Freeze -FreezePath $ready -Mode 'live' -RepoRootPath $null | Out-Null
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-missing-key' {
    $sandbox = New-StSandbox 'freeze-missing'
    try {
        $freezePath = New-StFreeze $sandbox
        foreach ($missing in [string[]] @('staging', 'hilog_tag', 'plan_status', 'runner_py_sha256', 'review', 'operator')) {
            $doc = Read-StFreezeDoc $freezePath
            $doc.PSObject.Properties.Remove($missing)
            $path = Join-Path $sandbox ('missing-' + $missing + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('missing key must be rejected: ' + $missing)
        }
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-extra-key' {
    $sandbox = New-StSandbox 'freeze-extra'
    try {
        $freezePath = New-StFreeze $sandbox
        foreach ($extra in @(@('surprise', 1), @('spacing', 3), @('legacy', $true))) {
            $doc = Read-StFreezeDoc $freezePath
            $doc | Add-Member -NotePropertyName ([string] $extra[0]) -NotePropertyValue $extra[1]
            $path = Join-Path $sandbox ('extra-' + $extra[0] + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('extra key must be rejected: ' + $extra[0])
        }
        # nested extra keys are rejected too
        $doc = Read-StFreezeDoc $freezePath
        $doc.hdc | Add-Member -NotePropertyName 'serial' -NotePropertyValue 'X'
        $path = Join-Path $sandbox 'extra-nested.json'
        Write-StFreezeDoc $path $doc
        $capturedPath = $path
        Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) 'nested extra key must be rejected'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-ready-without-pass-confirmation' {
    $sandbox = New-StSandbox 'freeze-ready'
    try {
        foreach ($combo in @(@('pending', 'pending'), @('pending', 'pass'), @('pass', 'pending'))) {
            # ('pending','pass') is rejected by the pending-machine rule itself;
            # ('pass','pending') by the ready double-binding rule.
            $sub = Join-Path $sandbox ($combo[0] + '-' + $combo[1])
            [void] [System.IO.Directory]::CreateDirectory($sub)
            $freezePath = New-StFreeze $sub -PlanStatus 'ready' -Confirmation ([string] $combo[0]) -Review ([string] $combo[1])
            $capturedPath = $freezePath
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('ready without double pass must be rejected: ' + $combo[0] + '/' + $combo[1])
        }
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-review-pass-over-pending-machine-confirmation' {
    $sandbox = New-StSandbox 'freeze-machinepending'
    try {
        $freezePath = New-StFreeze $sandbox -PlanStatus 'blocked' -Confirmation 'pending' -Review 'pass'
        $capturedPath = $freezePath
        Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) 'review pass over pending machine confirmation must be rejected'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-runner-ps1-hash-mismatch' {
    $sandbox = New-StSandbox 'freeze-runnersha'
    try {
        $wrongValues = @(
            ('f' * 64),
            (Get-StSha256File $script:SelfTestPath),
            'abc',
            ('A' * 64)
        )
        foreach ($wrong in $wrongValues) {
            $freezePath = New-StFreeze $sandbox -RunnerPs1Sha ([string] $wrong)
            $capturedPath = $freezePath
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('runner_ps1_sha256 mismatch must be rejected: [' + $wrong + ']')
        }
        # The empty-string escape is GONE (review round-2 NEW-MINOR-1): the
        # builder canonicalizes -RunnerPs1Sha '' to the real hash, so the
        # empty value is injected by direct doc mutation instead.
        $doc = Read-StFreezeDoc (New-StFreeze $sandbox)
        $doc.runner_ps1_sha256 = ''
        $emptyPath = Join-Path $sandbox 'runner-ps1-empty.json'
        Write-StFreezeDoc $emptyPath $doc
        $capturedEmpty = $emptyPath
        Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedEmpty -Mode 'dry-run' -RepoRootPath $null }) 'runner_ps1_sha256 empty string must be rejected'
        # The recomputed runner hash must equal this very runner file.
        $actualPath = New-StFreeze $sandbox
        Load-G0Freeze -FreezePath $actualPath -Mode 'dry-run' -RepoRootPath $null | Out-Null
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-empty-or-wrong-parity-hashes' {
    # All four implementation hashes are REQUIRED and recomputed against
    # their sibling files: the empty-string escape is gone (review round-2
    # NEW-MINOR-1), and drifted values are rejected on this runner too.
    $sandbox = New-StSandbox 'freeze-paritysha'
    try {
        $mutations = @(
            @('runner_py_sha256', ''),
            @('runner_py_sha256', ('e' * 64)),
            @('selftest_py_sha256', ''),
            @('selftest_ps1_sha256', ('b' * 64))
        )
        foreach ($mutation in $mutations) {
            $key = [string] $mutation[0]
            $value = [string] $mutation[1]
            $doc = Read-StFreezeDoc (New-StFreeze $sandbox)
            $doc.$key = $value
            $safeValue = $value -replace '[^A-Za-z0-9]', 'X'
            $path = Join-Path $sandbox ('parity-' + $key + '-' + $safeValue + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('parity hash drift must be rejected: ' + $key)
        }
        # The real sibling hashes are accepted.
        $realPath = New-StFreeze $sandbox
        Load-G0Freeze -FreezePath $realPath -Mode 'dry-run' -RepoRootPath $null | Out-Null
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-declared-pass-record-hash-mismatch' {
    $sandbox = New-StSandbox 'freeze-recordhash'
    try {
        $freezePath = New-StFreeze $sandbox -PlanStatus 'ready' -Confirmation 'pass' -Review 'pass'
        foreach ($field in [string[]] @('confirmation', 'review')) {
            $doc = Read-StFreezeDoc $freezePath
            $doc.$field.record_sha256 = ('f' * 64)
            $path = Join-Path $sandbox ('wronghash-' + $field + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('declared-pass hash mismatch must be rejected: ' + $field)
            # a missing record file is rejected as well
            $doc = Read-StFreezeDoc $freezePath
            $doc.$field.record_path = (Join-Path $sandbox 'does-not-exist.json')
            $path2 = Join-Path $sandbox ('nofile-' + $field + '.json')
            Write-StFreezeDoc $path2 $doc
            $capturedPath2 = $path2
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath2 -Mode 'dry-run' -RepoRootPath $null }) ('missing record file must be rejected: ' + $field)
        }
        # the matching pair loads
        Load-G0Freeze -FreezePath $freezePath -Mode 'dry-run' -RepoRootPath $null | Out-Null
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-live-requires-ready' {
    $sandbox = New-StSandbox 'freeze-live'
    try {
        $blocked = New-StFreeze (Join-Path $sandbox 'b') -PlanStatus 'blocked'
        $capturedBlocked = $blocked
        Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedBlocked -Mode 'live' -RepoRootPath $null }) 'live with blocked freeze must be rejected'
        $ready = New-StFreeze (Join-Path $sandbox 'r') -PlanStatus 'ready' -Confirmation 'pass' -Review 'pass'
        Load-G0Freeze -FreezePath $ready -Mode 'live' -RepoRootPath $null | Out-Null
        # DryRun accepts both blocked and ready.
        Load-G0Freeze -FreezePath $blocked -Mode 'dry-run' -RepoRootPath $null | Out-Null
        Load-G0Freeze -FreezePath $ready -Mode 'dry-run' -RepoRootPath $null | Out-Null
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'freeze-rejects-frozen-value-drift' {
    $sandbox = New-StSandbox 'freeze-drift'
    try {
        $freezePath = New-StFreeze $sandbox
        $mutations = @(
            @('bundle', 'cn.example.other'),
            @('ability', 'OtherAbility'),
            @('module', 'other'),
            @('staging', '/data/local/tmp/other'),
            @('hilog_tag', 'OtherTag'),
            @('scenario_window_seconds', 59),
            @('scenario_window_seconds', '60'),
            @('plan_status', 'nope'),
            @('attempt', 'retry-1'),
            @('code_sha', 'short'),
            @('authorization_id', 'AUTH-OTHER'),
            @('campaign_id', 'OTHER-CAMPAIGN'),
            @('evidence_id', 'OTHER-EVIDENCE'),
            @('operator', 'someone else')
        )
        foreach ($mutation in $mutations) {
            $key = [string] $mutation[0]
            $value = $mutation[1]
            $doc = Read-StFreezeDoc $freezePath
            $doc.$key = $value
            $safeValue = ([string] $value) -replace '[^A-Za-z0-9]', '_'
            $path = Join-Path $sandbox ('drift-' + $key + '-' + $safeValue + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('frozen value drift must be rejected: ' + $key)
        }
        $nested = @(
            @('target_tuple', 'device_model', 'PIXEL-9'),
            @('target_tuple', 'full_system_build', 'OTHER BUILD'),
            @('target_tuple', 'kernel_architecture', 'x86_64'),
            @('elf_profile', 'pt_tls', $false),
            @('elf_profile', 'tprel64_count', 2),
            @('elf_profile', 'static_tls_flag', $true)
        )
        foreach ($mutation in $nested) {
            $section = [string] $mutation[0]
            $key = [string] $mutation[1]
            $value = $mutation[2]
            $doc = Read-StFreezeDoc $freezePath
            $doc.$section.$key = $value
            $path = Join-Path $sandbox ('drift-' + $section + '-' + $key + '.json')
            Write-StFreezeDoc $path $doc
            $capturedPath = $path
            Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) ('nested drift must be rejected: ' + $section + '.' + $key)
        }
        # needed list drift
        $doc = Read-StFreezeDoc $freezePath
        $doc.elf_profile.needed = @('libc.so', 'libdl.so')
        $path = Join-Path $sandbox 'drift-needed.json'
        Write-StFreezeDoc $path $doc
        $capturedPath = $path
        Assert-St (Test-StThrows { Load-G0Freeze -FreezePath $capturedPath -Mode 'dry-run' -RepoRootPath $null }) 'needed list drift must be rejected'
    } finally {
        Remove-StSandbox $sandbox
    }
}

# =====================================================================
# 5. TargetBindingConfirm (subprocess + fake hdc)
# =====================================================================

Register-StTest 'tbc-pass-writes-single-use-double-file-record' {
    $sandbox = New-StSandbox 'tbc-pass'
    try {
        $freezePath = New-StFreeze $sandbox
        $hdcPath = Install-StFakeHdc $sandbox 'tbc-fake-hdc.sh' $script:StTbcFakeHdcScript
        Set-StFreezeHdc $freezePath $hdcPath
        $recordPath = Join-Path $sandbox 'confirmation' 'record.json'
        [void] [System.IO.Directory]::CreateDirectory((Split-Path -Parent $recordPath))
        $proc = Invoke-RunnerSubprocess `
            -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath, '-ConfirmationRecord', $recordPath) `
            -EnvMap @{ PHYS_1_TARGET = $script:StTargetToken }
        Assert-St ($proc['exit_code'] -eq 0) ('TBC pass exit code mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
        Assert-St ($proc['stdout'].Contains('RUNNER_RESULT=pass')) 'TBC pass stdout must contain RUNNER_RESULT=pass'
        Assert-St ($proc['stdout'].Contains('COMMAND_ATTEMPTED=3')) 'TBC pass must attempt 3 commands'
        Assert-St ($proc['stdout'].Contains('COMMAND_COMPLETED=3')) 'TBC pass must complete 3 commands'
        Assert-St ($proc['stdout'].Contains('IS_EVIDENCE=false')) 'TBC record must be non-evidence'
        Assert-St ([System.IO.File]::Exists($recordPath)) 'TBC record file must exist'
        $companionPath = $recordPath + '.sha256'
        Assert-St ([System.IO.File]::Exists($companionPath)) 'TBC companion file must exist'
        Assert-St (([System.IO.File]::ReadAllText($companionPath).Trim()) -ceq (Get-StSha256File $recordPath)) 'companion sha must match the record bytes'
        $record = [System.IO.File]::ReadAllText($recordPath) | ConvertFrom-Json
        Assert-St (($record.record_kind) -ceq 'g0-target-binding-confirmation') 'record kind mismatch'
        Assert-St (($record.is_evidence -eq $false) -and (($record.verdict) -ceq 'pass')) 'record must be a pass non-evidence record'
        Assert-St (([int] $record.command_attempted -eq 3) -and ([int] $record.command_completed -eq 3)) 'record command counters mismatch'
        Assert-St ($record.target_redacted -eq $true) 'record must be target-redacted'
        Assert-St ((($record.expected_model) -ceq $script:StModel) -and (($record.observed_model) -ceq $script:StModel)) 'record model mismatch'
        Assert-St ((($record.expected_build) -ceq $script:StBuild) -and (($record.observed_build) -ceq $script:StBuild)) 'record build mismatch'
        # created_at carries the fixed +08:00 zone (checked on raw bytes: the
        # ISO string parses to DateTime via ConvertFrom-Json).
        $recordText = [System.IO.File]::ReadAllText($recordPath)
        Assert-St ($recordText -cmatch '"created_at": "[^"]*\+08:00"') 'created_at must carry the +08:00 offset'
        Assert-St ((($record.authorization_id) -ceq $script:StAuthId) -and (($record.campaign_id) -ceq $script:StCampaignId) -and (($record.evidence_id) -ceq $script:StEvidenceId)) 'record id mismatch'
        Assert-St (($record.code_sha) -ceq ('a' * 40)) 'record code_sha mismatch'
        Assert-St (($record.runner_py_sha256) -ceq (Get-StSha256File $script:PythonRunnerPath)) 'record runner_py_sha256 mismatch'
        # the real target never enters the record
        Assert-St (-not $recordText.Contains($script:StTargetToken)) 'real target must not enter the record'
        # single-use: rerun hits the pre-record gate -> exit 1, record untouched
        $shaBefore = Get-StSha256File $recordPath
        $proc = Invoke-RunnerSubprocess `
            -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath, '-ConfirmationRecord', $recordPath) `
            -EnvMap @{ PHYS_1_TARGET = $script:StTargetToken }
        Assert-St ($proc['exit_code'] -eq 1) 'single-use rerun must exit 1'
        Assert-St ((Get-StSha256File $recordPath) -ceq $shaBefore) 'single-use rerun must not touch the record'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'tbc-tuple-drift-exits-2-with-blocked-record' {
    $sandbox = New-StSandbox 'tbc-drift'
    try {
        $freezePath = New-StFreeze $sandbox
        $hdcPath = Install-StFakeHdc $sandbox 'tbc-fake-hdc.sh' $script:StTbcFakeHdcDriftScript
        Set-StFreezeHdc $freezePath $hdcPath
        $recordPath = Join-Path $sandbox 'record.json'
        $proc = Invoke-RunnerSubprocess `
            -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath, '-ConfirmationRecord', $recordPath) `
            -EnvMap @{ PHYS_1_TARGET = $script:StTargetToken }
        Assert-St ($proc['exit_code'] -eq 2) ('TBC drift exit code mismatch: ' + $proc['exit_code'])
        Assert-St ($proc['stdout'].Contains('RUNNER_RESULT=blocked')) 'TBC drift stdout must contain RUNNER_RESULT=blocked'
        $record = [System.IO.File]::ReadAllText($recordPath) | ConvertFrom-Json
        Assert-St ((($record.verdict) -ceq 'blocked') -and (($record.reason) -cne 'N/A')) 'drift record must be blocked with a reason'
        Assert-St (([string] $record.reason).ToLowerInvariant().IndexOf('model', [System.StringComparison]::Ordinal) -ge 0) 'drift reason must mention model'
        Assert-St (([System.IO.File]::ReadAllText(($recordPath + '.sha256')).Trim()) -ceq (Get-StSha256File $recordPath)) 'drift record companion must match'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'tbc-invalid-target-token-exits-2-with-blocked-record' {
    $sandbox = New-StSandbox 'tbc-token'
    try {
        $freezePath = New-StFreeze $sandbox
        $hdcPath = Install-StFakeHdc $sandbox 'tbc-fake-hdc.sh' $script:StTbcFakeHdcScript
        Set-StFreezeHdc $freezePath $hdcPath
        foreach ($bad in @('-leading-dash', 'has space', 'PHYS-1', '<PHYS_1_TARGET>', '')) {
            $safe = (([string] $bad).ToLowerInvariant() -replace '[^a-z0-9]+', '-')
            $recordPath = Join-Path $sandbox ('record-' + $safe + '.json')
            $envMap = @{}
            if ([string]::IsNullOrEmpty([string] $bad)) { $envMap = @{} }
            else { $envMap = @{ PHYS_1_TARGET = [string] $bad } }
            $proc = Invoke-RunnerSubprocess `
                -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath, '-ConfirmationRecord', $recordPath) `
                -EnvMap $envMap
            Assert-St ($proc['exit_code'] -eq 2) ('invalid token exit code mismatch for [' + $bad + ']: ' + $proc['exit_code'])
            $record = [System.IO.File]::ReadAllText($recordPath) | ConvertFrom-Json
            Assert-St ((($record.verdict) -ceq 'blocked') -and ([int] $record.command_attempted -eq 0)) ('invalid token record mismatch for [' + $bad + ']')
        }
    } finally {
        Remove-StSandbox $sandbox
    }
}

# =====================================================================
# 6./7. full DryRun
# =====================================================================

Register-StTest 'dryrun-pass-end-to-end' {
    $sandbox = New-StSandbox 'dryrun-pass'
    try {
        $freezePath = New-StFreeze $sandbox
        $evidence = Join-Path $sandbox 'evidence-dry'
        $raw = Join-Path $sandbox 'raw-dry'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $freezePath) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'pass' }
        Assert-St ($proc['exit_code'] -eq 0) ('DryRun pass exit code mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
        $stdoutLines = @(Get-G0TextLines ([string] $proc['stdout']))
        Assert-St (($stdoutLines[$stdoutLines.Count - 1]) -ceq 'VERDICT=pass') 'DryRun pass must end with VERDICT=pass'
        Assert-St ($proc['stdout'].Contains('hdc_process_starts=17')) 'DryRun pass must report 17 hdc process starts'
        # evidence layout
        $resultsPath = Join-Path $evidence 'scenario-results.json'
        $manifestPath = Join-Path $evidence 'hash-manifest.json'
        $sealPath = Join-Path $evidence 'campaign-seal.json'
        $transcriptPath = Join-Path $evidence 'transcript.redacted.jsonl'
        foreach ($path in @($resultsPath, $manifestPath, $sealPath, $transcriptPath)) {
            Assert-St ([System.IO.File]::Exists($path)) ('evidence file missing: ' + $path)
        }
        $record = [System.IO.File]::ReadAllText($resultsPath) | ConvertFrom-Json
        Assert-St ((($record.verdict) -ceq 'pass') -and ($null -eq $record.fail_reason)) 'record verdict mismatch'
        Assert-St (($record.is_evidence -eq $false) -and (($record.execution_mode) -ceq 'dry-run')) 'record must be dry-run non-evidence'
        Assert-St (($record.non_evidence_reason) -cne 'N/A') 'non-evidence reason must be set'
        # the evidence record now carries BOTH runner hashes (NEW-MINOR-1)
        Assert-St (([string] $record.runner_py_sha256) -ceq (Get-StSha256File $script:PythonRunnerPath)) 'record runner_py_sha256 mismatch'
        Assert-St (([string] $record.runner_ps1_sha256) -ceq (Get-StSha256File $script:RunnerPath)) 'record runner_ps1_sha256 mismatch'
        Assert-St ([int] $record.markers.count -eq 1) 'marker count mismatch'
        Assert-St (([string] $record.markers.fields.hello) -ceq '42') 'marker hello mismatch'
        Assert-St (([string] $record.markers.fields.runtimeBytes) -ceq '1048576') 'marker runtimeBytes mismatch'
        # array-valued evidence fields stay arrays through serialization even
        # with a single element (review MAJOR-1 parity contract): a collapsed
        # scalar would deserialize as [string], not [object[]]
        Assert-St ($record.markers.raw_lines -is [System.Array]) 'single-element raw_lines must stay an array'
        Assert-St (@($record.markers.raw_lines).Count -eq 1) 'raw_lines element count mismatch'
        Assert-St (([string] @($record.markers.raw_lines)[0]).Contains('G0_RESULT|')) 'raw_lines element must be the marker line'
        Assert-St (@($record.integrity_violations).Count -eq 0) 'integrity must be empty'
        Assert-St ([int] $record.markers.window_seconds -eq 60) 'marker window mismatch'
        Assert-St ([int] $record.hdc_execution.command_attempted -eq 17) 'command_attempted mismatch'
        Assert-St ([int] $record.hdc_execution.command_completed -eq 17) 'command_completed mismatch'
        Assert-St ([int] $record.hdc_execution.logical_calls -eq 17) 'logical_calls mismatch'
        Assert-St (([int] $record.hdc_execution.operations.HilogStream) -eq 1) 'HilogStream operation count mismatch'
        Assert-St (($record.cleanup.status) -ceq 'verified-clean') 'cleanup status mismatch'
        Assert-St ((($record.absent_probes.bundle_dump) -ceq 'absent') -and (($record.absent_probes.pidof) -ceq 'absent') -and (($record.absent_probes.staging) -ceq 'absent')) 'absent probes mismatch'
        Assert-St (@($record.integrity_violations).Count -eq 0) 'integrity violations must be empty'
        # the pass marker declares loaderError= (empty) - preserved verbatim
        Assert-St (($null -eq $record.loader_error) -or (([string] $record.loader_error) -ceq '')) 'loader_error must be empty for pass'
        Assert-St (($null -eq $record.loader_errno) -or (([string] $record.loader_errno) -ceq '0')) 'loader_errno must be empty/0 for pass'
        Assert-St (($record.fault_probe.status) -ceq 'no-fault-lines') 'fault probe status mismatch'
        # seal binds record + manifest bytes
        $seal = [System.IO.File]::ReadAllText($sealPath) | ConvertFrom-Json
        Assert-St (($seal.record.path) -ceq 'scenario-results.json') 'seal record path mismatch'
        Assert-St (($seal.record.sha256) -ceq (Get-StSha256File $resultsPath)) 'seal record sha mismatch'
        Assert-St (($seal.manifest.path) -ceq 'hash-manifest.json') 'seal manifest path mismatch'
        Assert-St (($seal.manifest.sha256) -ceq (Get-StSha256File $manifestPath)) 'seal manifest sha mismatch'
        Assert-St ((($seal.run_status) -ceq 'completed') -and ([int] $seal.final_exit_code -eq 0)) 'seal run status mismatch'
        Assert-St (($null -eq $seal.fail_reason) -and (($seal.verdict) -ceq 'pass')) 'seal verdict mismatch'
        Assert-St (($seal.transcript_chain_head) -ceq ($record.transcript_reference.chain_head)) 'seal chain head mismatch'
        # manifest verifies every produced file (evidence + raw)
        $manifest = [System.IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
        Assert-St (($manifest.algorithm) -ceq 'SHA-256') 'manifest algorithm mismatch'
        $hasTranscriptEntry = $false
        foreach ($entry in @($manifest.files)) {
            Assert-St ((Get-StSha256File (Join-Path $evidence ([string] $entry.path))) -ceq ([string] $entry.sha256)) ('manifest hash mismatch: ' + $entry.path)
            if (([string] $entry.path) -ceq 'transcript.redacted.jsonl') { $hasTranscriptEntry = $true }
        }
        Assert-St $hasTranscriptEntry 'manifest must list the transcript'
        foreach ($entry in @($manifest.external_raw_files)) {
            Assert-St ((Get-StSha256File (Join-Path $raw ([string] $entry.path))) -ceq ([string] $entry.sha256)) ('manifest raw hash mismatch: ' + $entry.path)
        }
        # transcript chain: runner verifier + independent recompute agree
        Assert-St (@(Test-G0TranscriptIntegrity -TranscriptPath $transcriptPath).Count -eq 0) 'runner transcript verifier must report no violations'
        $recompute = Get-StTranscriptRecompute -TranscriptPath $transcriptPath
        Assert-St (@($recompute['violations']).Count -eq 0) 'independent transcript recompute must find no violations'
        Assert-St (([string] $recompute['head']) -ceq ([string] $seal.transcript_chain_head)) 'independent chain head mismatch'
        # raw artifacts: hilog kept the marker, per-command stdout/stderr exist
        $hilogRaw = [System.IO.File]::ReadAllText((Join-Path $raw 'hilog-raw.txt'))
        Assert-St ($hilogRaw.Contains('G0_RESULT') -and $hilogRaw.Contains($script:HilogTag)) 'hilog raw must keep the marker and tag'
        Assert-St ([System.IO.File]::Exists((Join-Path $raw '09-startentry.stdout.txt'))) 'raw 09-startentry.stdout.txt missing'
        Assert-St ([System.IO.File]::Exists((Join-Path $raw '17-stagingprobe.stderr.txt'))) 'raw 17-stagingprobe.stderr.txt missing'
        # transcript/audit argv keep placeholders only
        $transcriptText = [System.IO.File]::ReadAllText($transcriptPath)
        Assert-St ($transcriptText.Contains($script:TargetPlaceholder)) 'transcript must keep the target placeholder'
        Assert-St ($transcriptText.Contains($script:HapPlaceholder)) 'transcript must keep the hap placeholder'
        # redaction: the real target token appears in NO evidence/raw output
        Assert-StNoTargetTokenAnywhere ([string[]] @($evidence, $raw))
        # the frozen (sentinel) hdc was never executed by -DryRun
        Assert-St (-not [System.IO.File]::Exists((Join-Path $sandbox 'SENTINEL-EXECUTED'))) 'frozen hdc sentinel must never run in DryRun'
        # host process table: the fake never appears as comm 'hdc'
        Assert-St ((Get-G0HdcProcessCount) -eq 0) 'host hdc process count must stay 0'
        Assert-St ([int] $record.host_hdc_processes_after -eq 0) 'record host hdc count mismatch'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'dryrun-dlopen-rejected-end-to-end' {
    $sandbox = New-StSandbox 'dryrun-dlopen'
    try {
        $freezePath = New-StFreeze $sandbox
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $freezePath) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'dlopen-rejected' }
        Assert-St ($proc['exit_code'] -eq 0) ('DryRun dlopen exit code mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
        $stdoutLines = @(Get-G0TextLines ([string] $proc['stdout']))
        Assert-St (($stdoutLines[$stdoutLines.Count - 1]) -ceq 'VERDICT=blocked') 'DryRun dlopen must end with VERDICT=blocked'
        $resultsPath = Join-Path $sandbox 'evidence-dry' 'scenario-results.json'
        $record = [System.IO.File]::ReadAllText($resultsPath) | ConvertFrom-Json
        Assert-St ((($record.verdict) -ceq 'blocked') -and (($record.fail_reason) -ceq 'dlopen-blocked')) 'dlopen verdict mismatch'
        Assert-St ($record.is_evidence -eq $false) 'dlopen record must be non-evidence'
        Assert-St (([string] $record.loader_error) -ceq $script:StDlopenError) 'loader error must be preserved verbatim'
        Assert-St (([string] $record.loader_errno) -ceq '2') 'loader errno mismatch'
        Assert-St ([int] $record.markers.count -eq 1) 'dlopen marker count mismatch'
        Assert-St (([string] $record.markers.fields.stage) -ceq 'dlopen') 'dlopen marker stage mismatch'
        Assert-St (([string] $record.markers.fields.verdict) -ceq 'FAIL') 'dlopen marker verdict mismatch'
        Assert-St (($record.cleanup.status) -ceq 'verified-clean') 'dlopen cleanup mismatch'
        Assert-St (@($record.integrity_violations).Count -eq 0) 'dlopen integrity must be empty'
        $seal = [System.IO.File]::ReadAllText((Join-Path $sandbox 'evidence-dry' 'campaign-seal.json')) | ConvertFrom-Json
        Assert-St ((($seal.fail_reason) -ceq 'dlopen-blocked') -and (($seal.verdict) -ceq 'blocked')) 'dlopen seal mismatch'
        Assert-St (($seal.record.sha256) -ceq (Get-StSha256File $resultsPath)) 'dlopen seal record sha mismatch'
        # loader error also preserved verbatim in the raw hilog capture
        $hilogRaw = [System.IO.File]::ReadAllText((Join-Path $sandbox 'raw-dry' 'hilog-raw.txt'))
        Assert-St ($hilogRaw.Contains($script:StDlopenError)) 'raw hilog must keep the loader error verbatim'
        # dlopen-rejected yields a faultlogger line, recorded without changing the mapping
        Assert-St (($record.fault_probe.status) -ceq 'fault-lines-present') 'dlopen fault probe mismatch'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'dryrun-install-fails-end-to-end' {
    # A G0BlockedError ending must complete with a SEALED blocked record
    # (exit 0, scenario-results/hash-manifest/campaign-seal all present),
    # never crash before sealing (review BLOCKER-1 regression).
    $sandbox = New-StSandbox 'dryrun-install'
    try {
        $freezePath = New-StFreeze $sandbox
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $freezePath) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'install-fails' }
        Assert-St ($proc['exit_code'] -eq 0) ('install-fails exit code mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
        $stdoutLines = @(Get-G0TextLines ([string] $proc['stdout']))
        Assert-St (($stdoutLines[$stdoutLines.Count - 1]) -ceq 'VERDICT=blocked') 'install-fails must end with VERDICT=blocked'
        $evidenceDir = Join-Path $sandbox 'evidence-dry'
        $resultsPath = Join-Path $evidenceDir 'scenario-results.json'
        $sealPath = Join-Path $evidenceDir 'campaign-seal.json'
        $manifestPath = Join-Path $evidenceDir 'hash-manifest.json'
        foreach ($p in @($resultsPath, $sealPath, $manifestPath)) {
            Assert-St ([System.IO.File]::Exists($p)) ('sealed artifact missing: ' + $p)
        }
        $record = [System.IO.File]::ReadAllText($resultsPath) | ConvertFrom-Json
        Assert-St (($record.verdict) -ceq 'blocked') 'install-fails verdict mismatch'
        Assert-St (([string] $record.fail_reason) -ceq 'installhap-success-marker-missing') ('install-fails reason mismatch: ' + [string] $record.fail_reason)
        Assert-St ($record.is_evidence -eq $false) 'install-fails record must be non-evidence'
        Assert-St ([int] $record.markers.count -eq 0) 'install-fails marker count mismatch'
        # FaultProbe never ran on this path: status must say so (MINOR-1)
        Assert-St (($record.fault_probe.status) -ceq 'not-run') 'install-fails fault status must be not-run'
        Assert-St ($null -eq $record.fault_probe.fault_lines) 'install-fails fault_lines must be null'
        Assert-St (($record.cleanup.status) -ceq 'verified-clean') 'install-fails cleanup mismatch'
        Assert-St (@($record.integrity_violations).Count -eq 0) 'install-fails integrity must be empty'
        $seal = [System.IO.File]::ReadAllText($sealPath) | ConvertFrom-Json
        Assert-St (($seal.verdict) -ceq 'blocked') 'install-fails seal verdict mismatch'
        Assert-St (([string] $seal.fail_reason) -ceq 'installhap-success-marker-missing') 'install-fails seal reason mismatch'
        Assert-St (($seal.run_status) -ceq 'completed') 'install-fails seal run status mismatch'
        Assert-St ([int] $seal.final_exit_code -eq 0) 'install-fails seal exit mismatch'
        Assert-St (($seal.record.sha256) -ceq (Get-StSha256File $resultsPath)) 'install-fails seal record sha mismatch'
        # empty arrays stay arrays in the serialized evidence (MAJOR-1 contract)
        $resultsText = [System.IO.File]::ReadAllText($resultsPath)
        Assert-St ($resultsText -cmatch '"raw_lines":\s*\[\s*\]') 'install-fails raw_lines must serialize as []'
        Assert-St ($resultsText -cmatch '"integrity_violations":\s*\[\s*\]') 'install-fails integrity must serialize as []'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'dryrun-ready-freeze-double-binding-accepted' {
    $sandbox = New-StSandbox 'dryrun-ready'
    try {
        $freezePath = New-StFreeze $sandbox -PlanStatus 'ready' -Confirmation 'pass' -Review 'pass'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $freezePath) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'pass' }
        Assert-St ($proc['exit_code'] -eq 0) ('ready freeze DryRun exit mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
        $stdoutLines = @(Get-G0TextLines ([string] $proc['stdout']))
        Assert-St (($stdoutLines[$stdoutLines.Count - 1]) -ceq 'VERDICT=pass') 'ready freeze DryRun must pass'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'dryrun-ready-freeze-with-broken-review-binding-exits-1' {
    $sandbox = New-StSandbox 'dryrun-readybad'
    try {
        $freezePath = New-StFreeze $sandbox -PlanStatus 'ready' -Confirmation 'pass' -Review 'pass'
        $doc = Read-StFreezeDoc $freezePath
        $doc.review.record_sha256 = ('f' * 64)
        $brokenPath = Join-Path $sandbox 'freeze-broken.json'
        Write-StFreezeDoc $brokenPath $doc
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $brokenPath) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'pass' }
        Assert-St ($proc['exit_code'] -eq 1) 'broken review binding must exit 1'
        Assert-St (-not [System.IO.Directory]::Exists((Join-Path $sandbox 'evidence-dry'))) 'no evidence root may be created on validation failure'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'dryrun-rejects-invalid-or-missing-script-env' {
    $sandbox = New-StSandbox 'dryrun-env'
    try {
        $freezePath = New-StFreeze $sandbox
        foreach ($scriptEnv in @($null, 'bogus', 'PASS', '')) {
            $envMap = @{}
            if ($null -ne $scriptEnv) { $envMap = @{ G0_DRYRUN_SCRIPT = [string] $scriptEnv } }
            $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', $freezePath) -EnvMap $envMap
            Assert-St ($proc['exit_code'] -eq 1) ('invalid script env must exit 1: [' + $scriptEnv + ']')
            Assert-St (-not [System.IO.Directory]::Exists((Join-Path $sandbox 'evidence-dry'))) ('no evidence root for invalid script env: [' + $scriptEnv + ']')
        }
    } finally {
        Remove-StSandbox $sandbox
    }
}

# =====================================================================
# CLI gates
# =====================================================================

Register-StTest 'cli-mode-mutual-exclusion-and-required-args' {
    $sandbox = New-StSandbox 'cli'
    try {
        $freezePath = New-StFreeze $sandbox
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Live', '-Freeze', $freezePath)
        Assert-St ($proc['exit_code'] -eq 1) 'DryRun+Live must be mutually exclusive (exit 1)'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath)
        Assert-St ($proc['exit_code'] -eq 1) 'TBC without a confirmation record must exit 1'
        $proc = Invoke-RunnerSubprocess `
            -RunnerArgs @('-TargetBindingConfirm', '-Freeze', $freezePath, '-ConfirmationRecord', (Join-Path $sandbox 'r.json'), '-DryRun') `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'pass' }
        Assert-St ($proc['exit_code'] -eq 1) 'TBC+DryRun must be mutually exclusive (exit 1)'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun')
        Assert-St ($proc['exit_code'] -eq 1) 'DryRun without a freeze must exit 1'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-ConfirmationRecord', (Join-Path $sandbox 'r.json'), '-Freeze', $freezePath)
        Assert-St ($proc['exit_code'] -eq 1) 'ConfirmationRecord without a mode must exit 1'
        $proc = Invoke-RunnerSubprocess -RunnerArgs @('-DryRun', '-Freeze', (Join-Path $sandbox 'missing.json')) `
            -EnvMap @{ G0_DRYRUN_SCRIPT = 'pass' }
        Assert-St ($proc['exit_code'] -eq 1) 'missing freeze file must exit 1'
    } finally {
        Remove-StSandbox $sandbox
    }
}

Register-StTest 'cli-version-flag-exact-output' {
    $proc = Invoke-RunnerSubprocess -RunnerArgs @('-Version')
    Assert-St (($proc['exit_code'] -eq 0) -and (([string] $proc['stdout']).Trim() -ceq 'g0-phys-probe-campaign.ps1 1.0.0')) ('version output mismatch: [' + $proc['stdout'] + ']')
}

Register-StTest 'runner-embedded-selftest-passes' {
    $proc = Invoke-RunnerSubprocess -RunnerArgs @('-SelfTest')
    Assert-St ($proc['exit_code'] -eq 0) ('embedded selftest exit mismatch: ' + $proc['exit_code'] + ' stderr=' + $proc['stderr'])
    Assert-St ($proc['stdout'].Contains('SELFTEST_RESULT=pass HDC_PROCESSES=0')) 'embedded selftest must end pass with HDC_PROCESSES=0'
}

# =====================================================================
# host HDC process count probe
# =====================================================================

Register-StTest 'host-hdc-count-probe-first-column-only' {
    # synthetic table: only the exact first-column 'hdc' counts
    $synthetic = "hdc -t foo`n" +
        "fake-hdc -t bar`n" +
        "python3 /tmp/fake-hdc`n" +
        "hdcx wrapper`n" +
        "sshd: /usr/sbin/sshd -D`n" +
        "chrome /home/x --flag`n"
    Assert-St ((Get-G0HdcCountFromPsOutput $synthetic) -eq 1) 'synthetic ps table must count exactly one hdc'
    Assert-St ((Get-G0HdcCountFromPsOutput '') -eq 0) 'empty ps table must count zero'
    Assert-St ((Get-G0HdcCountFromPsOutput "hdc`nhdc shell hilog") -eq 2) 'bare hdc lines must each count'
    # Windows twin parser: `tasklist /FO CSV /NH`, first (image-name) field only
    $csv = '"hdc.exe","4012","Console","1","5,552 K"' + "`n" +
        '"fake-hdc.exe","4013","Console","1","4,120 K"' + "`n" +
        '"python3.11.exe","4014","Console","1","22,016 K"' + "`n" +
        '"HDC.EXE","4015","Console","1","6,000 K"' + "`n" +
        '"hdcd.exe","4016","Console","1","3,000 K"'
    Assert-St ((Get-G0HdcCountFromTasklistOutput $csv) -eq 2) 'synthetic tasklist must count hdc.exe and HDC.EXE only'
    Assert-St ((Get-G0HdcCountFromTasklistOutput '') -eq 0) 'empty tasklist must count zero'
    Assert-St ((Get-G0HdcCountFromTasklistOutput '"chrome.exe","1","x","1","1 K"') -eq 0) 'unrelated image must not count'
    # the fixed absolute host probe is used (OS-adaptive), count must be 0
    if (Test-G0WindowsHost) {
        Assert-St ([System.IO.File]::Exists((Join-Path ([string] $env:SystemRoot) 'System32\tasklist.exe'))) 'tasklist.exe must exist for the Windows host probe'
    } else {
        Assert-St ([System.IO.File]::Exists('/usr/bin/ps')) '/usr/bin/ps must exist for the host probe'
    }
    $count = Get-G0HdcProcessCount
    Assert-St ($count -eq 0) ('expected no host process with comm hdc, got ' + $count)
}

# =====================================================================
# Runner
# =====================================================================

function Invoke-StAll {
    $passed = 0
    $failures = [System.Collections.Generic.List[object]]::new()
    foreach ($test in $script:StTestRegistry) {
        try {
            & ([scriptblock] $test['body'])
            $passed++
            Write-StOut ('PASS ' + [string] $test['name'])
        } catch {
            $failures.Add(@{ name = [string] $test['name']; message = [string] $_.Exception.Message; stack = [string] $_.ScriptStackTrace })
            Write-StOut ('FAIL ' + [string] $test['name'] + ': ' + $_.Exception.Message)
        }
    }
    Write-StOut '---- summary ----'
    Write-StOut ('TOTAL={0} PASSED={1} FAILED={2}' -f @($script:StTestRegistry).Count, $passed, @($failures).Count)
    foreach ($failure in $failures) {
        Write-StOut ('FAILED-DETAIL ' + $failure['name'])
        Write-StOut ([string] $failure['message'])
        Write-StOut ([string] $failure['stack'])
    }
    $resultText = 'fail'
    if (@($failures).Count -eq 0) { $resultText = 'pass' }
    Write-StOut ('SELFTEST_RESULT=' + $resultText)
    if (@($failures).Count -gt 0) { return 1 }
    return 0
}

exit (Invoke-StAll)
