#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$FreezeManifest,
    [string]$EvidenceRoot,
    [string]$RawRoot,
    [string]$HapA,
    [string]$HapB,
    [string]$HdcPath,
    [int]$HdcTimeoutSeconds = 20,
    [int]$OperatorTimeoutSeconds = 300,
    [switch]$DryRun,
    [switch]$LiveSimulation,
    [string]$SimulationFixture,
    [switch]$SelfTest,
    [switch]$TargetBindingConfirm,
    [string]$ConfirmationRecord
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:BundleA = 'cn.alfadb.netbird.e3physvpna'
$script:BundleB = 'cn.alfadb.netbird.e3physvpnb'
$script:Ability = 'EntryAbility'
$script:Module = 'entry'
$script:Staging = '/data/local/tmp/e3-phys-preflight'
$script:WindowSeconds = 60
$script:NoDeviceMode = [bool]($DryRun -or $LiveSimulation -or $SelfTest)
$script:ExecutionMode = if ($TargetBindingConfirm) { 'target-binding-confirm' } elseif ($DryRun) { 'dry-run' } elseif ($LiveSimulation) { 'live-simulation' } else { 'live' }
$script:HdcProcessStartCount = 0
$script:HdcLogicalCallCount = 0
$script:HdcOperationCounts = @{}
$script:InfrastructureReasonObserved = $null
$script:TranscriptIndex = 0
$script:TranscriptPreviousHash = ('0' * 64)
$script:ProjectionTranscript = $null
$script:ActualTarget = $null
$script:RepoRoot = $null
$script:EvidencePath = $null
$script:RawPath = $null
$script:InstalledA = $false
$script:InstalledB = $false
$script:StagingSent = $false
$script:StagingMayExist = $false
$script:CampaignStarted = $false
$script:CampaignCapture = $null
$script:PartialScenarios = @()
$script:CaptureDegraded = [Collections.Generic.List[object]]::new()
$script:ObservationOnlyDegraded = [Collections.Generic.List[object]]::new()
$script:CaptureArtifacts = [Collections.Generic.List[object]]::new()
$script:SimulationLayoutFirstAttempt = [Collections.Generic.Dictionary[string, DateTimeOffset]]::new()
$script:RawHilogArtifacts = [Collections.Generic.List[object]]::new()
$script:FaultArtifacts = [Collections.Generic.List[object]]::new()
$script:CleanupActions = [Collections.Generic.List[object]]::new()
$script:CleanupVerification = [ordered]@{ status = 'not-run'; verified_absent = $false; bundles = @() }
$script:PublicVersionLiterals = @('PLA-AL10 7.0.0.100(SP8C00E32R7P2)')
$script:Simulation = $null
$script:VirtualSeconds = 0.0
$script:VirtualBase = [DateTimeOffset]::Parse('2099-01-01T00:00:00+00:00')
$script:DeviceClockSkewToleranceSeconds = 3.0
$script:FrozenDeviceZoneMap = [ordered]@{ CST = '+08:00' }
$script:CampaignPhase = 'preflight'
$script:SimulationInstalledA = $false
$script:SimulationInstalledB = $false
$script:SimulationStagingPresent = $false
$script:PriorBlockedBinding = $null
$script:Freeze = $null
$script:ProbeContexts = @{}
$script:CurrentWindowEnd = $null
$script:OperatorWaitHistory = [Collections.Generic.List[object]]::new()
$script:OperatorActions = [Collections.Generic.List[object]]::new()
$script:OperatorWaitCurrent = $null
$script:ScenarioInvalid = $null
$script:VerifiedRequests = @{}
# ADJ-20260808-0003: cross-step / cross-scenario operator action guard checkpoint. Any
# UI_START / UI_STOP / UI_STOP_SKIPPED event observed after the last verified point but not
# owned by the current mechanical step makes the scenario invalid immediately, before the
# next prompt. Auto StartEntry ENTRY events are not UI actions and never trigger the guard.
$script:OperatorActionGuardFrom = $null
$script:LastCaptureInfrastructure = $false
# ADJ-20260810-0001 (C6): the current authorization fixes one AUTH, one candidate pair, and
# attempt=initial. TargetBindingConfirm (producer) and every consumer of this AUTH's confirmation
# (ready Live / ready DryRun) enforce the exact pair and initial attempt with retry N/A; any later
# retry requires new governance and a new authorization and can never consume this AUTH path.
$script:AuthId = 'AUTH-E3-PHYS1API26-20260815-0005'
$script:CandidateCampaignId = 'E3-PHYS-PREFLIGHT-20260815-0005'
$script:CandidateEvidenceId = 'EV-E3-PHYS1API26-20260815-0005'
$script:MachineFreshConfirmation = $null
$script:IndependentReviewRecord = $null
$script:SimulationActiveBundles = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
$script:SimulationScenarioStepsWritten = @{}

function Get-TextSha256 {
    param([Parameter(Mandatory)][string]$Text)
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($Text))).ToLowerInvariant()
}

function Get-FileSha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-NormalizedPath {
    param([Parameter(Mandatory)][string]$Path)
    return [IO.Path]::GetFullPath($Path).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
}

function Test-IsUnderPath {
    param([Parameter(Mandatory)][string]$Candidate, [Parameter(Mandatory)][string]$Parent)
    $candidatePath = (Get-NormalizedPath $Candidate) + [IO.Path]::DirectorySeparatorChar
    $parentPath = (Get-NormalizedPath $Parent) + [IO.Path]::DirectorySeparatorChar
    return $candidatePath.StartsWith($parentPath, [StringComparison]::OrdinalIgnoreCase)
}

function Get-OptionalProperty {
    param($Object, [Parameter(Mandatory)][string]$Name, $Default = $null)
    if ($null -eq $Object) { return $Default }
    if ($Object -is [Collections.IDictionary]) {
        if ($Object.Contains($Name)) { return $Object[$Name] }
        return $Default
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $Default }
    return $property.Value
}

function Get-OptionalJsonBoolean {
    param($Object, [Parameter(Mandatory)][string]$Name, [bool]$Default = $false)
    if ($null -eq $Object -or $null -eq $Object.PSObject.Properties[$Name]) { return $Default }
    $value = $Object.PSObject.Properties[$Name].Value
    if ($null -eq $value -or $value.GetType() -ne [bool]) { throw "simulation hook '$Name' must be a JSON Boolean" }
    return [bool]$value
}

function Get-RequiredProperty {
    param([Parameter(Mandatory)]$Object, [Parameter(Mandatory)][string]$Name)
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { throw "freeze manifest missing property: $Name" }
    return $property.Value
}

function Assert-JsonBoolean {
    param([Parameter(Mandatory)]$Object, [Parameter(Mandatory)][string]$Name, [bool]$Expected)
    $value = Get-RequiredProperty $Object $Name
    if ($null -eq $value -or $value.GetType() -ne [bool] -or $value -ne $Expected) {
        throw "$Name must be the JSON Boolean $($Expected.ToString().ToLowerInvariant())"
    }
}

function Test-JsonInteger {
    param($Value)
    # ADJ-20260810-0001 (C6): JSON integer gate. PowerShell's ConvertFrom-Json yields Int32 for
    # small integers and Int64 for large ones, so both are accepted; strings, floats, booleans and
    # null are rejected (a string that casts to a number must never pass an integer schema check).
    return $null -ne $Value -and ($Value.GetType() -eq [int] -or $Value.GetType() -eq [long])
}

function Convert-ToDateTimeOffset {
    param($Value)
    # ADJ-20260810-0001 (C6): strict timestamp coercion for governance records (confirmation and
    # independent review). Accepts ISO-8601 strings, [DateTime], and [DateTimeOffset] inputs and
    # returns a DateTimeOffset, or $null when unparseable. Never round-trips through
    # [string][datetime] (which loses subsecond precision and depends on the host locale); an
    # already-typed DateTimeOffset passes through untouched. Both consumers use this helper so the
    # machine confirmation and the review record parse timestamps identically.
    if ($Value -is [DateTimeOffset]) { return $Value }
    if ($Value -is [DateTime]) {
        try { return [DateTimeOffset]::new([DateTime]$Value) } catch { return $null }
    }
    if ($Value -is [string] -and -not [string]::IsNullOrWhiteSpace($Value)) {
        $parsed = [DateTimeOffset]::MinValue
        if ([DateTimeOffset]::TryParse([string]$Value, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AllowWhiteSpaces, [ref]$parsed)) { return $parsed }
    }
    return $null
}

function Protect-SensitiveText {
    param([AllowNull()][string]$Text)
    if ($null -eq $Text) { return '' }
    $safe = $Text
    $literalTokens = [ordered]@{}
    $literalIndex = 0
    foreach ($literal in @($script:PublicVersionLiterals | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } | Sort-Object Length -Descending -Unique)) {
        $token = "__E3_PUBLIC_VERSION_$literalIndex`__"
        $safe = $safe.Replace([string]$literal, $token, [StringComparison]::Ordinal)
        $literalTokens[$token] = [string]$literal
        $literalIndex++
    }
    if (-not [string]::IsNullOrWhiteSpace($script:ActualTarget)) {
        $safe = [regex]::Replace($safe, [regex]::Escape($script:ActualTarget), '<REDACTED_TARGET>', [Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    $rules = @(
        @{ Pattern = '(?i)"(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)"\s*:\s*"[^"]*"'; Replacement = '"$1":"<REDACTED>"' },
        @{ Pattern = '(?i)"(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)"\s*:\s*\[[^\]]*\]'; Replacement = '"$1":["<REDACTED>"]' },
        @{ Pattern = '(?i)\b(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)\s*[:=]\s*[^\s,;|"]+'; Replacement = '$1=<REDACTED>' },
        @{ Pattern = '(?i)\b(?:SN|UDID|SERIAL)[-_][A-Z0-9._-]{6,}\b'; Replacement = '<REDACTED_SERIAL>' },
        @{ Pattern = '(?i)\b(?:tcp|udp|hdc)://[^\s|]+'; Replacement = '<REDACTED_ENDPOINT>' },
        @{ Pattern = '(?i)\b(target|endpoint|host|address)\s*[:=]\s*[^\s,;|]+'; Replacement = '$1=<REDACTED_ENDPOINT>' },
        @{ Pattern = '(?i)\b(port)\s*[:=]\s*[0-9]{1,5}\b'; Replacement = '$1=<REDACTED_PORT>' },
        @{ Pattern = '(?i)\[[0-9a-f:]+\](?::[0-9]{1,5})?'; Replacement = '<REDACTED_IPV6>' },
        @{ Pattern = '(?i)(?<![0-9a-f:])(?=[0-9a-f:]*[a-f])(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}(?![0-9a-f:])'; Replacement = '<REDACTED_IPV6>' },
        @{ Pattern = '(?i)(?<![0-9a-f:])::1(?![0-9a-f:])'; Replacement = '<REDACTED_IPV6>' },
        @{ Pattern = '(?<![0-9])(?:(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})\.){3}(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(?::[0-9]{1,5})?(?![0-9])'; Replacement = '<REDACTED_IPV4>' },
        @{ Pattern = '(?i)\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b'; Replacement = '<REDACTED_MAC>' }
    )
    foreach ($rule in $rules) { $safe = [regex]::Replace($safe, $rule.Pattern, $rule.Replacement) }
    $safe = [regex]::Replace($safe, '(?i)(?<![0-9a-z:])[0-9a-f:]*:[0-9a-f:]+(?![0-9a-z:])', [Text.RegularExpressions.MatchEvaluator]{
        param($match)
        $parsedAddress = $null
        if ([Net.IPAddress]::TryParse($match.Value, [ref]$parsedAddress) -and $parsedAddress.AddressFamily -eq [Net.Sockets.AddressFamily]::InterNetworkV6) { return '<REDACTED_IPV6>' }
        return $match.Value
    })
    foreach ($token in $literalTokens.Keys) { $safe = $safe.Replace([string]$token, [string]$literalTokens[$token], [StringComparison]::Ordinal) }
    return $safe
}

function Protect-SensitiveData {
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) { return Protect-SensitiveText $Value }
    if ($Value -is [Collections.IDictionary]) {
        $copy = [ordered]@{}
        foreach ($key in $Value.Keys) {
            $keyName = [string]$key
            $copy[$keyName] = if ($keyName -match '^(?i:target|endpoint|host|address|port)$') { '<REDACTED_ENDPOINT>' } else { Protect-SensitiveData $Value[$key] }
        }
        return $copy
    }
    if ($Value -is [pscustomobject]) {
        $copy = [ordered]@{}
        foreach ($property in $Value.PSObject.Properties) {
            $copy[$property.Name] = if ($property.Name -match '^(?i:target|endpoint|host|address|port)$') { '<REDACTED_ENDPOINT>' } else { Protect-SensitiveData $property.Value }
        }
        return $copy
    }
    if ($Value -is [Collections.IEnumerable] -and $Value -isnot [string]) {
        [object[]]$items = foreach ($item in $Value) { Protect-SensitiveData $item }
        Write-Output -NoEnumerate $items
        return
    }
    return $Value
}

function Get-Now {
    if ($LiveSimulation) { return $script:VirtualBase.AddSeconds($script:VirtualSeconds) }
    return [DateTimeOffset]::Now
}

function Wait-Until {
    param([Parameter(Mandatory)][DateTimeOffset]$Target)
    if ($LiveSimulation) {
        $delta = ($Target - (Get-Now)).TotalSeconds
        if ($delta -gt 0) { $script:VirtualSeconds += $delta }
        return
    }
    while ((Get-Now) -lt $Target) {
        $remaining = ($Target - (Get-Now)).TotalMilliseconds
        Start-Sleep -Milliseconds ([Math]::Max(10, [Math]::Min(250, [int]$remaining)))
    }
}

function Add-TranscriptRecord {
    param([Parameter(Mandatory)][string]$Kind, [Parameter(Mandatory)]$Data)
    if ($null -eq $script:ProjectionTranscript) { return }
    $safeData = Protect-SensitiveData $Data
    $payload = [ordered]@{
        index = ++$script:TranscriptIndex
        host_observed_at = (Get-Now).ToString('o')
        kind = $Kind
        data = $safeData
        previous_hash = $script:TranscriptPreviousHash
    }
    $payloadCanonical = $payload | ConvertTo-Json -Depth 30 -Compress
    $entryHash = Get-TextSha256 $payloadCanonical
    $record = [ordered]@{ payload = $payload; payload_canonical = $payloadCanonical; entry_hash = $entryHash }
    $line = $record | ConvertTo-Json -Depth 32 -Compress
    [IO.File]::AppendAllText($script:ProjectionTranscript, $line + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $script:TranscriptPreviousHash = $entryHash
}

function Write-JsonFile {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)]$Object)
    $json = $Object | ConvertTo-Json -Depth 40
    [IO.File]::WriteAllText($Path, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
}

function Write-OperatorWaitState {
    param(
        [Parameter(Mandatory)][ValidateSet('waiting', 'operator-complete', 'verifying', 'captured', 'invalid', 'complete')][string]$Phase,
        [AllowNull()][int]$Scenario = $null,
        [AllowNull()][int]$StepIndex = $null,
        [AllowNull()][string]$StepId = $null,
        [AllowNull()][string]$ExpectedAction = $null,
        [AllowNull()]$CaptureBefore = $null,
        [AllowNull()]$CaptureAfter = $null,
        [AllowNull()]$MachinePrecondition = $null,
        [AllowNull()]$MachinePostcondition = $null
    )
    if ($null -eq $script:EvidencePath) { return }
    $updatedAt = (Get-Now).ToString('o')
    $current = [ordered]@{
        scenario = $Scenario
        step_index = $StepIndex
        step_id = $StepId
        expected_action = $ExpectedAction
        phase = $Phase
        capture_before = $(if ($null -eq $CaptureBefore) { [ordered]@{ status = 'not-required' } } else { $CaptureBefore })
        capture_after = $(if ($null -eq $CaptureAfter) { [ordered]@{ status = 'not-required' } } else { $CaptureAfter })
        machine_precondition = $(if ($null -eq $MachinePrecondition) { [ordered]@{ status = 'not-evaluated' } } else { $MachinePrecondition })
        machine_postcondition = $(if ($null -eq $MachinePostcondition) { [ordered]@{ status = 'not-evaluated' } } else { $MachinePostcondition })
        updated_at = $updatedAt
    }
    $script:OperatorWaitCurrent = $current
    $script:OperatorWaitHistory.Add($current)
    $payload = [ordered]@{
        schema_version = 2
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $(if ($null -ne $script:Freeze) { [string]$script:Freeze.campaign_id } else { $null })
        evidence_id = $(if ($null -ne $script:Freeze) { [string]$script:Freeze.evidence_id } else { $null })
        execution_mode = $script:ExecutionMode
        trust_model = 'mechanical-action-only-machine-verified-v1'
        scenario = $current.scenario
        step_index = $current.step_index
        step_id = $current.step_id
        expected_action = $current.expected_action
        phase = $current.phase
        capture_before = $current.capture_before
        capture_after = $current.capture_after
        machine_precondition = $current.machine_precondition
        machine_postcondition = $current.machine_postcondition
        updated_at = $current.updated_at
        complete = ($Phase -eq 'complete')
        completed_at = $(if ($Phase -eq 'complete') { $updatedAt } else { $null })
        history = @($script:OperatorWaitHistory)
    }
    Write-JsonFile (Join-Path $script:EvidencePath 'operator-wait-state.json') $payload
}

function Throw-ScenarioInvalid {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][string]$Reason,
        [AllowNull()][int]$StepIndex = $null,
        [AllowNull()][string]$StepId = $null,
        [AllowNull()][string]$ExpectedAction = $null,
        [AllowNull()]$MachinePrecondition = $null,
        [AllowNull()]$MachinePostcondition = $null,
        [AllowNull()]$CaptureBefore = $null,
        [AllowNull()]$CaptureAfter = $null
    )
    $safeReason = Protect-SensitiveText $Reason
    $script:ScenarioInvalid = [ordered]@{ scenario = $Scenario; step_index = $StepIndex; step_id = $StepId; reason = $safeReason; detected_at = (Get-Now).ToString('o') }
    Write-OperatorWaitState 'invalid' -Scenario $Scenario -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -CaptureAfter $CaptureAfter -MachinePrecondition $MachinePrecondition -MachinePostcondition $(if ($null -eq $MachinePostcondition) { [ordered]@{ status = 'invalid'; reason = $safeReason } } else { $MachinePostcondition })
    Add-TranscriptRecord 'scenario-invalid' $script:ScenarioInvalid
    throw "SCENARIO_INVALID scenario=$Scenario reason=$safeReason"
}

function Get-GitRepositoryRoot {
    $rootText = (& git -C $PSScriptRoot rev-parse --show-toplevel 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($rootText)) { throw 'unable to resolve repository root with git rev-parse' }
    return Get-NormalizedPath $rootText
}

function Get-RepositoryState {
    $head = (& git -C $script:RepoRoot rev-parse HEAD 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-f]{40}$') { throw 'unable to read repository HEAD' }
    $status = (& git -C $script:RepoRoot status --porcelain=v2 --untracked-files=all 2>&1 | Out-String)
    if ($LASTEXITCODE -ne 0) { throw 'unable to read repository state' }
    return [pscustomobject]@{ Head = $head; Status = $status; Clean = [string]::IsNullOrEmpty($status); Fingerprint = Get-TextSha256 ($head + "`n" + $status) }
}

function Assert-NoReparseAncestor {
    param([Parameter(Mandatory)][string]$Path)
    $cursor = Get-NormalizedPath $Path
    while (-not (Test-Path -LiteralPath $cursor)) {
        $parent = Split-Path $cursor -Parent
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $cursor) { break }
        $cursor = $parent
    }
    while (-not [string]::IsNullOrWhiteSpace($cursor)) {
        $item = Get-Item -LiteralPath $cursor -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "output path has a junction or symlink ancestor: $($item.FullName)"
        }
        $parent = Split-Path $item.FullName -Parent
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $item.FullName) { break }
        $cursor = $parent
    }
}

function Assert-OutputCandidate {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Label)
    $normalized = Get-NormalizedPath $Path
    if (Test-Path -LiteralPath $normalized) { throw "$Label already exists; existing evidence is immutable and selective rerun is forbidden" }
    Assert-NoReparseAncestor $normalized
    if ($normalized -eq $script:RepoRoot -or (Test-IsUnderPath $normalized $script:RepoRoot)) {
        throw "$Label must be outside the git repository"
    }
    return $normalized
}

function Initialize-OutputRoots {
    $evidenceCandidate = Assert-OutputCandidate $EvidenceRoot 'EvidenceRoot'
    $rawCandidateInput = if ([string]::IsNullOrWhiteSpace($RawRoot)) { $evidenceCandidate + '.raw' } else { $RawRoot }
    $rawCandidate = Assert-OutputCandidate $rawCandidateInput 'RawRoot'
    if ($rawCandidate -eq $evidenceCandidate -or (Test-IsUnderPath $rawCandidate $evidenceCandidate) -or (Test-IsUnderPath $evidenceCandidate $rawCandidate)) {
        throw 'EvidenceRoot and RawRoot must be independent sibling trees'
    }
    $externalLock = $evidenceCandidate + '.campaign.lock'
    if (Test-Path -LiteralPath $externalLock) { throw 'campaign lock already exists; selective rerun is forbidden' }
    Assert-NoReparseAncestor $externalLock
    $lockStream = $null
    try {
        $lockStream = [IO.FileStream]::new($externalLock, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        $lockBytes = [Text.Encoding]::UTF8.GetBytes("E3-PHYS-PREFLIGHT`n")
        $lockStream.Write($lockBytes, 0, $lockBytes.Length)
        $lockStream.Flush($true)
    } finally {
        if ($null -ne $lockStream) { $lockStream.Dispose() }
    }
    [IO.Directory]::CreateDirectory($evidenceCandidate) | Out-Null
    [IO.Directory]::CreateDirectory($rawCandidate) | Out-Null
    Assert-NoReparseAncestor $evidenceCandidate
    Assert-NoReparseAncestor $rawCandidate
    if ($evidenceCandidate -eq $script:RepoRoot -or (Test-IsUnderPath $evidenceCandidate $script:RepoRoot) -or
        $rawCandidate -eq $script:RepoRoot -or (Test-IsUnderPath $rawCandidate $script:RepoRoot)) {
        throw 'post-create output path validation failed'
    }
    $projection = Join-Path $evidenceCandidate 'projection'
    [IO.Directory]::CreateDirectory($projection) | Out-Null
    $script:ProjectionTranscript = Join-Path $projection 'transcript.redacted.jsonl'
    $transcriptStream = [IO.FileStream]::new($script:ProjectionTranscript, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::Read)
    $transcriptStream.Dispose()
    Write-JsonFile (Join-Path $evidenceCandidate 'campaign-lock.json') ([ordered]@{
        exception = 'E3-PHYS-PREFLIGHT'
        execution_mode = $script:ExecutionMode
        created_at = (Get-Now).ToString('o')
        external_lock_sha256 = Get-FileSha256 $externalLock
    })
    $script:EvidencePath = $evidenceCandidate
    $script:RawPath = $rawCandidate
    return [pscustomobject]@{ Evidence = $evidenceCandidate; Raw = $rawCandidate; ExternalLock = $externalLock }
}

function Assert-FileHash {
    param([Parameter(Mandatory)][string]$Label, [Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Expected)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label file missing" }
    if ($Expected -notmatch '^[0-9a-f]{64}$') { throw "$Label expected SHA-256 is not final" }
    if ((Get-FileSha256 $Path) -ne $Expected) { throw "$Label SHA-256 mismatch" }
}

function Get-FreezeContract {
    param([Parameter(Mandatory)]$Freeze)
    # ADJ-20260810-0001 (C6): null-safe projection so minimal test freezes can be hashed without
    # StrictMode property errors; a complete freeze yields byte-identical output to the historical
    # direct-access form (same ordered fields, same values).
    return [ordered]@{
        exception = Get-OptionalProperty $Freeze 'exception' $null
        campaign_id = Get-OptionalProperty $Freeze 'campaign_id' $null
        scenario_window_seconds = Get-OptionalProperty $Freeze 'scenario_window_seconds' $null
        device_alias = Get-OptionalProperty $Freeze 'device_alias' $null
        target_tuple = Get-OptionalProperty $Freeze 'target_tuple' $null
        settings_reallow_expected_path = Get-OptionalProperty $Freeze 'settings_reallow_expected_path' $null
        settings_reallow_path_policy = Get-OptionalProperty $Freeze 'settings_reallow_path_policy' $null
        settings_revoke_mechanism = Get-OptionalProperty $Freeze 'settings_revoke_mechanism' $null
        settings_vpn_page_policy = Get-OptionalProperty $Freeze 'settings_vpn_page_policy' $null
        destroy_terminal_policy = Get-OptionalProperty $Freeze 'destroy_terminal_policy' $null
        process_absent_required_count = Get-OptionalProperty $Freeze 'process_absent_required_count' $null
        process_absent_probe_spacing_seconds = Get-OptionalProperty $Freeze 'process_absent_probe_spacing_seconds' $null
        process_probe_target = Get-OptionalProperty $Freeze 'process_probe_target' $null
        operator_trust_model = Get-OptionalProperty $Freeze 'operator_trust_model' $null
        scenario_invalid_policy = Get-OptionalProperty $Freeze 'scenario_invalid_policy' $null
        layout_verification_profile = Get-OptionalProperty $Freeze 'layout_verification_profile' $null
        vpn_conflict_rejection_codes = Get-OptionalProperty $Freeze 'vpn_conflict_rejection_codes' $null
        signing = Get-OptionalProperty $Freeze 'signing' $null
        artifact_sha256 = Get-OptionalProperty $Freeze 'artifact_sha256' $null
        source = Get-OptionalProperty $Freeze 'source' $null
        sdk = Get-OptionalProperty $Freeze 'sdk' $null
        hdc = Get-OptionalProperty $Freeze 'hdc' $null
        runner_sha256 = Get-OptionalProperty $Freeze 'runner_sha256' $null
        code_sha = Get-OptionalProperty $Freeze 'code_sha' $null
        preflight_inputs_frozen_at = Get-OptionalProperty $Freeze 'preflight_inputs_frozen_at' $null
        cleanup_baseline_frozen = Get-OptionalProperty $Freeze 'cleanup_baseline_frozen' $null
        collection_ready = Get-OptionalProperty $Freeze 'collection_ready' $null
        independent_review_ready = Get-OptionalProperty $Freeze 'independent_review_ready' $null
        operator_role = Get-OptionalProperty $Freeze 'operator_role' $null
        independent_reviewer_role = Get-OptionalProperty $Freeze 'independent_reviewer_role' $null
    }
}

function Get-FreezeContractSha256 {
    param([Parameter(Mandatory)]$Freeze)
    return Get-TextSha256 ((Get-FreezeContract $Freeze) | ConvertTo-Json -Depth 30 -Compress)
}

function Get-ConfirmationContract {
    param([Parameter(Mandatory)]$Freeze)
    # ADJ-20260810-0001 (C6): stable two-phase projection. The full Get-FreezeContract includes
    # governance/time fields that legitimately differ between the blocked confirmation freeze and
    # the final ready freeze (preflight_inputs_frozen_at advances past the machine confirmation
    # and independent review end times, plan_status flips blocked->ready, independent_review_ready
    # is a phase readiness marker), so a confirmation/review record bound to the full contract
    # would be rejected by the ready-phase consumer (contract hash changed) or by its own time
    # gate (frozen_at not advanced). Get-ConfirmationContract covers the execution core, the exact
    # candidate pair, external inputs, code, runner, HDC, and roles - everything that must be
    # byte-identical across the two phases - and deliberately excludes plan_status,
    # preflight_inputs_frozen_at, machine_fresh_confirmation, independent_review_record, and
    # independent_review_ready. cleanup_baseline_frozen / collection_ready are static execution
    # prerequisites (both true in every freeze) and operator/reviewer roles are confirmation-time
    # facts, so they stay in the projection.
    return [ordered]@{
        exception = Get-OptionalProperty $Freeze 'exception' $null
        campaign_id = Get-OptionalProperty $Freeze 'campaign_id' $null
        evidence_id = Get-OptionalProperty $Freeze 'evidence_id' $null
        scenario_window_seconds = Get-OptionalProperty $Freeze 'scenario_window_seconds' $null
        device_alias = Get-OptionalProperty $Freeze 'device_alias' $null
        target_tuple = Get-OptionalProperty $Freeze 'target_tuple' $null
        settings_reallow_expected_path = Get-OptionalProperty $Freeze 'settings_reallow_expected_path' $null
        settings_reallow_path_policy = Get-OptionalProperty $Freeze 'settings_reallow_path_policy' $null
        settings_revoke_mechanism = Get-OptionalProperty $Freeze 'settings_revoke_mechanism' $null
        settings_vpn_page_policy = Get-OptionalProperty $Freeze 'settings_vpn_page_policy' $null
        destroy_terminal_policy = Get-OptionalProperty $Freeze 'destroy_terminal_policy' $null
        process_absent_required_count = Get-OptionalProperty $Freeze 'process_absent_required_count' $null
        process_absent_probe_spacing_seconds = Get-OptionalProperty $Freeze 'process_absent_probe_spacing_seconds' $null
        process_probe_target = Get-OptionalProperty $Freeze 'process_probe_target' $null
        operator_trust_model = Get-OptionalProperty $Freeze 'operator_trust_model' $null
        scenario_invalid_policy = Get-OptionalProperty $Freeze 'scenario_invalid_policy' $null
        layout_verification_profile = Get-OptionalProperty $Freeze 'layout_verification_profile' $null
        vpn_conflict_rejection_codes = Get-OptionalProperty $Freeze 'vpn_conflict_rejection_codes' $null
        signing = Get-OptionalProperty $Freeze 'signing' $null
        artifact_sha256 = Get-OptionalProperty $Freeze 'artifact_sha256' $null
        source = Get-OptionalProperty $Freeze 'source' $null
        sdk = Get-OptionalProperty $Freeze 'sdk' $null
        hdc = Get-OptionalProperty $Freeze 'hdc' $null
        runner_sha256 = Get-OptionalProperty $Freeze 'runner_sha256' $null
        code_sha = Get-OptionalProperty $Freeze 'code_sha' $null
        cleanup_baseline_frozen = Get-OptionalProperty $Freeze 'cleanup_baseline_frozen' $null
        collection_ready = Get-OptionalProperty $Freeze 'collection_ready' $null
        operator_role = Get-OptionalProperty $Freeze 'operator_role' $null
        independent_reviewer_role = Get-OptionalProperty $Freeze 'independent_reviewer_role' $null
    }
}

function Get-ConfirmationContractSha256 {
    param([Parameter(Mandatory)]$Freeze)
    return Get-TextSha256 ((Get-ConfirmationContract $Freeze) | ConvertTo-Json -Depth 30 -Compress)
}

function Assert-FreezeManifest {
    param([Parameter(Mandatory)]$Freeze, [Parameter(Mandatory)][string]$FreezePath)
    $schemaVersion = Get-RequiredProperty $Freeze 'schema_version'
    if (-not (Test-JsonInteger $schemaVersion) -or [long]$schemaVersion -ne 2) { throw 'unsupported freeze schema_version; strong operator state machine requires schema_version 2 as a JSON integer' }
    $planStatus = [string](Get-RequiredProperty $Freeze 'plan_status')
    if ($DryRun -or $TargetBindingConfirm) {
        if ($planStatus -notin @('blocked', 'ready')) { throw 'DryRun and TargetBindingConfirm plan_status must be blocked or ready' }
    } elseif ($planStatus -ne 'ready') {
        throw 'Live and LiveSimulation require plan_status ready'
    }
    if ([string](Get-RequiredProperty $Freeze 'exception') -ne 'E3-PHYS-PREFLIGHT') { throw 'exception mismatch' }
    $evidenceId = [string](Get-RequiredProperty $Freeze 'evidence_id')
    $campaignId = [string](Get-RequiredProperty $Freeze 'campaign_id')
    if ($evidenceId -notmatch '^EV-E3-[A-Z0-9-]+-[0-9]{8}-[0-9]{4}$') { throw 'evidence_id format invalid' }
    if ($campaignId -notmatch '^E3-PHYS-PREFLIGHT-[A-Z0-9-]+$') { throw 'campaign_id format invalid' }
    $attempt = [string](Get-RequiredProperty $Freeze 'attempt')
    if ($attempt -notin @('initial', 'infrastructure-blocked-retry-1')) { throw 'attempt invalid' }
    $retry = Get-RequiredProperty $Freeze 'retry'
    if ($attempt -eq 'initial') {
        if ([string](Get-RequiredProperty $retry 'basis') -ne 'N/A' -or [string](Get-RequiredProperty $retry 'infrastructure_reason') -ne 'N/A') {
            throw 'initial attempt retry fields must be N/A'
        }
    } else {
        $reason = [string](Get-RequiredProperty $retry 'infrastructure_reason')
        if ($reason -notin @('hdc-usb-interruption', 'collection-storage-failure', 'runner-host-failure')) {
            throw 'retry reason is not in the infrastructure-only allowlist'
        }
        $priorPath = Get-NormalizedPath ([string](Get-RequiredProperty $retry 'prior_record_path'))
        Assert-FileHash 'prior blocked record' $priorPath ([string](Get-RequiredProperty $retry 'prior_record_sha256'))
        $prior = Get-Content -LiteralPath $priorPath -Raw | ConvertFrom-Json -Depth 40
        $priorIsEvidence = Get-RequiredProperty $prior 'is_evidence'
        $priorArtifactCanonical = (Get-RequiredProperty $prior 'artifact_sha256') | ConvertTo-Json -Depth 10 -Compress
        $frozenArtifactCanonical = $Freeze.artifact_sha256 | ConvertTo-Json -Depth 10 -Compress
        if ([string](Get-RequiredProperty $prior 'campaign_id') -ne $campaignId -or
            [string](Get-RequiredProperty $prior 'attempt') -ne 'initial' -or
            [string](Get-RequiredProperty $prior 'execution_mode') -ne 'live' -or
            $null -eq $priorIsEvidence -or $priorIsEvidence.GetType() -ne [bool] -or -not $priorIsEvidence -or
            [string](Get-RequiredProperty $prior 'record_status') -ne 'blocked' -or
            [string](Get-RequiredProperty $prior 'overall') -ne 'blocked' -or
            [string](Get-RequiredProperty $prior 'verdict') -ne 'blocked' -or
            [string](Get-RequiredProperty $prior 'infrastructure_reason') -ne $reason -or
            [string](Get-RequiredProperty $prior 'code_sha') -ne [string]$Freeze.code_sha -or
            [string](Get-RequiredProperty $prior 'runner_sha256') -ne [string]$Freeze.runner_sha256 -or
            $priorArtifactCanonical -ne $frozenArtifactCanonical -or
            [string](Get-RequiredProperty $prior 'freeze_contract_sha256') -ne (Get-FreezeContractSha256 $Freeze)) {
            throw 'prior record does not authorize the single infrastructure-blocked retry'
        }
    }
    # ADJ-20260810-0001 (C6): the current AUTH fixes one candidate pair and attempt=initial. The
    # TargetBindingConfirm producer enforces the exact pair and initial attempt with retry N/A; the
    # generic infrastructure retry branch never applies to this AUTH path (any retry requires new
    # governance and a new authorization, and can never be issued under the current AUTH).
    if ($TargetBindingConfirm) {
        if ($campaignId -ne $script:CandidateCampaignId -or $evidenceId -ne $script:CandidateEvidenceId) {
            throw "TargetBindingConfirm under $($script:AuthId) requires the fixed candidate pair $($script:CandidateCampaignId) / $($script:CandidateEvidenceId)"
        }
        if ($attempt -ne 'initial') { throw 'TargetBindingConfirm under the current AUTH fixes attempt=initial; retries require new governance and cannot enter this path' }
        if ([string](Get-RequiredProperty $retry 'basis') -ne 'N/A' -or [string](Get-RequiredProperty $retry 'infrastructure_reason') -ne 'N/A') {
            throw 'TargetBindingConfirm under the current AUTH fixes retry.basis/infrastructure_reason=N/A'
        }
    }
    if ([int](Get-RequiredProperty $Freeze 'scenario_window_seconds') -ne 60) { throw 'scenario window must be exactly 60 seconds' }
    if ([string](Get-RequiredProperty $Freeze 'device_alias') -ne 'PHYS-1') { throw 'device alias must be PHYS-1' }
    $tuple = Get-RequiredProperty $Freeze 'target_tuple'
    $expectedTuple = [ordered]@{
        distribution = 'HarmonyOS'
        device_model = 'PLA-AL10'
        full_system_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
        api = '26'
        kernel_arch = 'aarch64'
        app_abi = 'arm64-v8a'
    }
    foreach ($key in $expectedTuple.Keys) {
        if ([string](Get-RequiredProperty $tuple $key) -ne $expectedTuple[$key]) { throw "frozen target tuple mismatch: $key" }
    }
    if ([string](Get-RequiredProperty $Freeze 'settings_reallow_expected_path') -notin @('direct-system-activation', 'system-reauthorization-UI')) {
        throw 'settings re-allow path invalid'
    }
    if ([string](Get-RequiredProperty $Freeze 'settings_reallow_path_policy') -ne 'observation-only') {
        throw 'settings_reallow_path_policy must be observation-only'
    }
    # ADJ-20260807-0003 decision fields: strict, fixed values. Old freezes without these fields are
    # historical only and are rejected for every mode (DryRun included), never usable for a new live.
    if ([string](Get-RequiredProperty $Freeze 'settings_revoke_mechanism') -ne 'settings-app-info-force-stop') {
        throw 'settings_revoke_mechanism must be settings-app-info-force-stop'
    }
    if ([string](Get-RequiredProperty $Freeze 'settings_vpn_page_policy') -ne 'observation-only') {
        throw 'settings_vpn_page_policy must be observation-only'
    }
    if ([string](Get-RequiredProperty $Freeze 'destroy_terminal_policy') -ne 'callback-or-strict-process-boundary') {
        throw 'destroy_terminal_policy must be callback-or-strict-process-boundary'
    }
    if ([int](Get-RequiredProperty $Freeze 'process_absent_required_count') -ne 2) {
        throw 'process_absent_required_count must be 2'
    }
    # Legacy `spacing` is an unknown field and is never compatibly reused. A freeze carrying only
    # the old `spacing` is rejected as missing the required new field by Get-RequiredProperty below.
    if ($null -ne $Freeze.PSObject.Properties['spacing']) {
        throw 'legacy spacing field is not part of the freeze schema; use process_absent_probe_spacing_seconds'
    }
    if ([double](Get-RequiredProperty $Freeze 'process_absent_probe_spacing_seconds') -ne 3) {
        throw 'process_absent_probe_spacing_seconds must be 3 seconds'
    }
    # ADJ-20260808-0001 decision field: pidof targets the <bundle>:vpn Extension ability process,
    # never the bundle UI process. Old freezes without this field are rejected for every mode.
    if ([string](Get-RequiredProperty $Freeze 'process_probe_target') -ne '<bundle>:vpn') {
        throw 'process_probe_target must be <bundle>:vpn (ADJ-20260808-0001 extension-process probe target)'
    }
    if ([string](Get-RequiredProperty $Freeze 'operator_trust_model') -ne 'mechanical-action-only-machine-verified-v1') {
        throw 'operator_trust_model must be mechanical-action-only-machine-verified-v1'
    }
    if ([string](Get-RequiredProperty $Freeze 'scenario_invalid_policy') -ne 'stop-and-finally-cleanup-seal') {
        throw 'scenario_invalid_policy must be stop-and-finally-cleanup-seal'
    }
    if ([string](Get-RequiredProperty $Freeze 'layout_verification_profile') -ne 'deterministic-layout-v1') {
        throw 'layout_verification_profile must be deterministic-layout-v1'
    }
    $conflictCodes = @((Get-RequiredProperty $Freeze 'vpn_conflict_rejection_codes') | ForEach-Object { [int]$_ })
    if ($conflictCodes.Count -lt 1 -or ($conflictCodes -join ',') -ne '2203002') {
        throw 'vpn_conflict_rejection_codes must freeze the explicit supported list [2203002]'
    }
    $signing = Get-RequiredProperty $Freeze 'signing'
    if ([string](Get-RequiredProperty $signing 'type') -ne 'ordinary-development') { throw 'signing type must be ordinary-development' }
    Assert-JsonBoolean $signing 'device_in_profile' $true
    $profileBasis = [string](Get-RequiredProperty $signing 'device_in_profile_basis')
    if ([string]::IsNullOrWhiteSpace($profileBasis) -or $profileBasis.Contains('<')) { throw 'signing.device_in_profile_basis incomplete' }
    foreach ($field in @('public_fingerprint', 'verification_result')) {
        $value = [string](Get-RequiredProperty $signing $field)
        if ([string]::IsNullOrWhiteSpace($value) -or $value.Contains('<')) { throw "signing.$field incomplete" }
    }
    if ([string]$signing.verification_result -ne 'pass') { throw 'local signature verification_result must be pass' }
    $artifacts = Get-RequiredProperty $Freeze 'artifact_sha256'
    $hapAHash = [string](Get-RequiredProperty $artifacts 'hap_a')
    $hapBHash = [string](Get-RequiredProperty $artifacts 'hap_b')
    if ((Get-NormalizedPath $HapA) -eq (Get-NormalizedPath $HapB) -or $hapAHash -eq $hapBHash) { throw 'FINAL HAP A/B must be distinct files and hashes' }
    Assert-FileHash 'FINAL HAP A' (Get-NormalizedPath $HapA) $hapAHash
    Assert-FileHash 'FINAL HAP B' (Get-NormalizedPath $HapB) $hapBHash
    $source = Get-RequiredProperty $Freeze 'source'
    Assert-FileHash 'source archive' ([string](Get-RequiredProperty $source 'archive_path')) ([string](Get-RequiredProperty $source 'archive_sha256'))
    Assert-FileHash 'source manifest' ([string](Get-RequiredProperty $source 'manifest_path')) ([string](Get-RequiredProperty $source 'manifest_sha256'))
    $sdk = Get-RequiredProperty $Freeze 'sdk'
    foreach ($field in @('version', 'api', 'syscap_basis')) {
        if ([string]::IsNullOrWhiteSpace([string](Get-RequiredProperty $sdk $field))) { throw "sdk.$field incomplete" }
    }
    $sdkFiles = @(Get-RequiredProperty $sdk 'files')
    if ($sdkFiles.Count -lt 1) { throw 'sdk hash map is empty' }
    foreach ($sdkFile in $sdkFiles) {
        Assert-FileHash 'SDK input' ([string](Get-RequiredProperty $sdkFile 'path')) ([string](Get-RequiredProperty $sdkFile 'sha256'))
    }
    $hdc = Get-RequiredProperty $Freeze 'hdc'
    $hdcVersion = [string](Get-RequiredProperty $hdc 'version')
    if ([string]::IsNullOrWhiteSpace($hdcVersion) -or $hdcVersion.Contains('<')) { throw 'hdc.version incomplete' }
    Assert-FileHash 'HDC executable' (Get-NormalizedPath $HdcPath) ([string](Get-RequiredProperty $hdc 'sha256'))
    Assert-FileHash 'runner' $PSCommandPath ([string](Get-RequiredProperty $Freeze 'runner_sha256'))
    if ([string](Get-RequiredProperty $Freeze 'code_sha') -notmatch '^[0-9a-f]{40}$') { throw 'code_sha incomplete' }
    $frozenAt = Convert-ToDateTimeOffset (Get-RequiredProperty $Freeze 'preflight_inputs_frozen_at')
    if ($null -eq $frozenAt) {
        throw 'preflight_inputs_frozen_at invalid'
    }
    foreach ($role in @('operator_role', 'independent_reviewer_role')) {
        $roleValue = [string](Get-RequiredProperty $Freeze $role)
        if ([string]::IsNullOrWhiteSpace($roleValue) -or $roleValue.Contains('<')) { throw "$role incomplete" }
    }
    if ([string]$Freeze.operator_role -eq [string]$Freeze.independent_reviewer_role) { throw 'operator and independent reviewer roles must differ' }
    Assert-JsonBoolean $Freeze 'cleanup_baseline_frozen' $true
    Assert-JsonBoolean $Freeze 'collection_ready' $true
    Assert-JsonBoolean $Freeze 'independent_review_ready' $true
    # ADJ-20260810-0001 (C6): independent review record gate. A blocked confirmation freeze keeps
    # independent_review_ready=true as a static contract/role readiness marker and does NOT need a
    # review record; ready Live / ready DryRun require a real pass review record with an
    # out-of-repo JSON + matching .sha256 companion (enforced in Assert-IndependentReviewRecord).
    $reviewRecord = Get-OptionalProperty $Freeze 'independent_review_record' $null
    if ($null -eq $reviewRecord) {
        if (-not $TargetBindingConfirm) { throw 'freeze manifest missing property: independent_review_record' }
    } else {
        $reviewStatus = [string](Get-OptionalProperty $reviewRecord 'status' '')
        if ($reviewStatus -notin @('pending', 'pass')) { throw 'independent_review_record.status must be pending or pass' }
        if ($reviewStatus -eq 'pass') {
            if ([string]::IsNullOrWhiteSpace([string](Get-OptionalProperty $reviewRecord 'record_path' '')) -or [string]::IsNullOrWhiteSpace([string](Get-OptionalProperty $reviewRecord 'record_sha256' ''))) {
                throw 'independent_review_record with status=pass requires record_path and record_sha256'
            }
        }
    }
    if (-not (Test-Path -LiteralPath $FreezePath -PathType Leaf)) { throw 'FreezeManifest file missing' }
    [void](Get-PriorBlockedBinding $Freeze)
    [void](Assert-MachineFreshConfirmation $Freeze)
}

function Assert-ModeExclusivity {
    # ADJ-20260810-0001: TargetBindingConfirm is a host-governed, mutually exclusive single-purpose
    # mode. It runs real HDC target-binding probes and therefore can never be combined with the
    # host-only modes (DryRun / LiveSimulation / SelfTest); ConfirmationRecord belongs only to it;
    # and confirm mode never initializes campaign roots, so EvidenceRoot/RawRoot are rejected
    # explicitly rather than silently ignored. This gate runs before the SelfTest early exit so
    # invalid switch combinations are rejected even when -SelfTest is present.
    if ($TargetBindingConfirm -and ($DryRun -or $LiveSimulation -or $SelfTest)) { throw 'TargetBindingConfirm is mutually exclusive with DryRun, LiveSimulation, and SelfTest' }
    if ($TargetBindingConfirm -and [string]::IsNullOrWhiteSpace($ConfirmationRecord)) { throw 'TargetBindingConfirm requires ConfirmationRecord' }
    if (-not $TargetBindingConfirm -and -not [string]::IsNullOrWhiteSpace($ConfirmationRecord)) { throw 'ConfirmationRecord is only valid with TargetBindingConfirm' }
    if ($TargetBindingConfirm -and -not [string]::IsNullOrWhiteSpace($EvidenceRoot)) { throw 'EvidenceRoot is not allowed with TargetBindingConfirm' }
    if ($TargetBindingConfirm -and -not [string]::IsNullOrWhiteSpace($RawRoot)) { throw 'RawRoot is not allowed with TargetBindingConfirm' }
    if ($DryRun -and $LiveSimulation) { throw 'DryRun and LiveSimulation are mutually exclusive' }
}

function Assert-MachineFreshConfirmation {
    param([Parameter(Mandatory)]$Freeze)
    # ADJ-20260810-0001 host-governed fresh confirmation gate. TargetBindingConfirm IS the
    # confirmation producer, so it may consume a pending/absent machine_fresh_confirmation object
    # on a blocked freeze. Live (real device) and DryRun with plan_status ready require the
    # object to be status=pass, bound to AUTH-E3-PHYS1API26-20260815-0005 and the fixed candidate
    # pair E3-PHYS-PREFLIGHT-20260815-0005 / EV-E3-PHYS1API26-20260815-0005, with a real
    # out-of-repository double-file record (JSON plus a matching .sha256 companion; a lone record
    # is never consumable) whose content agrees with the freeze on schema/record kind/
    # is_evidence=false/exception/code/runner/HDC/contract/candidate-IDs/attempt=initial/
    # model/build and whose time anchors satisfy started_at <= ended_at <=
    # preflight_inputs_frozen_at. No arbitrary age window is imposed: freshness is anchored only
    # by ordering plus the freeze preflight_inputs_frozen_at, and Live independently re-executes
    # the three target-binding probes during its own preflight (fresh double anchor). DryRun with
    # a blocked plan_status and LiveSimulation (synthetic fixtures have no physical record) do not
    # require it. Any retry under this AUTH is impossible: attempt is fixed to initial and the
    # generic infrastructure retry branch never applies to this confirmation path.
    if ($TargetBindingConfirm) { return $null }
    $planStatus = [string]$Freeze.plan_status
    $requirePass = ($script:ExecutionMode -eq 'live' -and -not $LiveSimulation) -or ($DryRun -and $planStatus -eq 'ready')
    $confirmation = Get-OptionalProperty $Freeze 'machine_fresh_confirmation' $null
    $confirmationStatus = if ($null -eq $confirmation) { '' } else { [string](Get-OptionalProperty $confirmation 'status' '') }
    # ADJ-20260810-0001 (C6): a blocked DryRun that declares status=pass is FULLY validated too
    # (a blocked DryRun can never hide a broken binding); status=pending or absent is allowed and
    # skipped on a blocked DryRun.
    $validateMachine = $requirePass -or ($DryRun -and $planStatus -eq 'blocked' -and $confirmationStatus -eq 'pass')
    if (-not $validateMachine) {
        # ADJ-20260810-0001 (C6): a blocked DryRun may skip a pending/absent machine confirmation,
        # but a declared-pass independent review can never ride on it: the review record binds the
        # machine confirmation hash, so a pending/absent machine side makes the declared pass
        # unverifiable and is rejected outright.
        if ($DryRun -and $planStatus -eq 'blocked') {
            $review = Get-OptionalProperty $Freeze 'independent_review_record' $null
            $reviewStatus = if ($null -eq $review) { '' } else { [string](Get-OptionalProperty $review 'status' '') }
            if ($reviewStatus -eq 'pass') { throw 'independent_review_record.status=pass requires machine_fresh_confirmation.status=pass; a pending/absent machine confirmation cannot anchor a declared-pass review' }
        }
        return $null
    }
    if ($requirePass -and $null -eq $confirmation) { throw 'a ready plan_status requires machine_fresh_confirmation with status=pass and a bound target-binding confirmation record' }
    if ($confirmationStatus -ne 'pass') { throw 'machine_fresh_confirmation.status must be pass for a ready plan_status' }
    $authorizationId = [string](Get-OptionalProperty $confirmation 'authorization_id' '')
    if ($authorizationId -ne $script:AuthId) { throw "machine_fresh_confirmation.authorization_id does not match $($script:AuthId)" }
    if ([string](Get-OptionalProperty $Freeze 'campaign_id') -ne $script:CandidateCampaignId -or [string](Get-OptionalProperty $Freeze 'evidence_id') -ne $script:CandidateEvidenceId) {
        throw "AUTH $($script:AuthId) fixes the candidate pair $($script:CandidateCampaignId) / $($script:CandidateEvidenceId); a ready freeze outside that pair cannot consume its confirmation"
    }
    if ([string](Get-OptionalProperty $Freeze 'attempt') -ne 'initial') { throw 'the current AUTH fixes attempt=initial; retries require new governance and can never consume this confirmation' }
    $frozenRetry = Get-OptionalProperty $Freeze 'retry' $null
    if ($null -ne $frozenRetry) {
        if ([string](Get-OptionalProperty $frozenRetry 'basis' '') -ne 'N/A' -or [string](Get-OptionalProperty $frozenRetry 'infrastructure_reason' '') -ne 'N/A') {
            throw 'the current AUTH fixes retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this confirmation path'
        }
    }
    $recordPathInput = [string](Get-OptionalProperty $confirmation 'record_path' '')
    if ([string]::IsNullOrWhiteSpace($recordPathInput)) { throw 'machine_fresh_confirmation.record_path missing' }
    $recordPath = Get-NormalizedPath $recordPathInput
    if (-not (Test-Path -LiteralPath $recordPath -PathType Leaf)) { throw 'confirmation record file missing' }
    if ($null -ne $script:RepoRoot -and ($recordPath -eq $script:RepoRoot -or (Test-IsUnderPath $recordPath $script:RepoRoot))) {
        throw 'confirmation record must be outside the git repository'
    }
    Assert-NoReparseAncestor $recordPath
    $companionPath = $recordPath + '.sha256'
    Assert-NoReparseAncestor $companionPath
    if (-not (Test-Path -LiteralPath $companionPath -PathType Leaf)) { throw 'confirmation record .sha256 companion missing; a lone record is never consumable' }
    $companionValue = [string](Get-Content -LiteralPath $companionPath -Raw).Trim()
    if ($companionValue -notmatch '^[0-9a-f]{64}$') { throw 'confirmation record companion does not contain a final SHA-256' }
    if ($companionValue -ne (Get-FileSha256 $recordPath)) { throw 'confirmation record .sha256 companion does not match the record bytes' }
    Assert-FileHash 'confirmation record' $recordPath ([string](Get-OptionalProperty $confirmation 'record_sha256' ''))
    $record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json -Depth 40
    # ADJ-20260810-0001 (C6): exact-schema gate - any unknown top-level field (e.g. a target/serial/
    # secret canary smuggled into the record) makes the record un-consumable.
    $confirmationAllowedFields = @('schema_version', 'record_kind', 'is_evidence', 'authorization_id', 'exception', 'campaign_id', 'evidence_id', 'attempt', 'retry', 'plan_status', 'device_alias', 'target_redacted', 'code_sha', 'runner_sha256', 'freeze_manifest_sha256', 'confirmation_contract_sha256', 'hdc_sha256', 'hdc_version', 'expected_model', 'expected_build', 'observed_model', 'observed_build', 'started_at', 'ended_at', 'command_attempted', 'command_completed', 'command_count', 'repository_fingerprint', 'verdict', 'reason')
    foreach ($recordProperty in $record.PSObject.Properties) {
        if ($recordProperty.Name -notin $confirmationAllowedFields) {
            throw "confirmation record has an unknown top-level field: $($recordProperty.Name)"
        }
    }
    function Get-ConfirmationField {
        param($Object, [Parameter(Mandatory)][string]$Name)
        $value = [string](Get-OptionalProperty $Object $Name '')
        if ([string]::IsNullOrWhiteSpace($value)) { throw "confirmation record missing or empty field: $Name" }
        return $value
    }
    $recordSchema = Get-OptionalProperty $record 'schema_version' $null
    if (-not (Test-JsonInteger $recordSchema) -or [long]$recordSchema -ne 1) { throw 'confirmation record schema_version must be 1' }
    if ((Get-ConfirmationField $record 'record_kind') -ne 'target-binding-confirmation') { throw 'confirmation record record_kind mismatch' }
    if ((Get-ConfirmationField $record 'exception') -ne 'E3-PHYS-PREFLIGHT') { throw 'confirmation record exception mismatch' }
    $recordIsEvidence = Get-OptionalProperty $record 'is_evidence' $null
    if ($null -eq $recordIsEvidence -or $recordIsEvidence.GetType() -ne [bool] -or $recordIsEvidence) { throw 'confirmation record must be is_evidence=false' }
    if ((Get-ConfirmationField $record 'verdict') -ne 'pass') { throw 'confirmation record verdict must be pass' }
    if ((Get-ConfirmationField $record 'reason') -ne 'N/A') { throw 'confirmation record reason must be N/A for a pass verdict' }
    if ((Get-ConfirmationField $record 'authorization_id') -ne $authorizationId) { throw 'confirmation record authorization_id mismatch' }
    if ((Get-ConfirmationField $record 'campaign_id') -ne $script:CandidateCampaignId -or (Get-ConfirmationField $record 'evidence_id') -ne $script:CandidateEvidenceId) {
        throw 'confirmation record candidate IDs do not match the fixed AUTH candidate pair'
    }
    if ((Get-ConfirmationField $record 'attempt') -ne 'initial') { throw 'confirmation record attempt must be initial under the current AUTH' }
    $recordRetry = Get-OptionalProperty $record 'retry' $null
    if ($null -eq $recordRetry -or [string](Get-OptionalProperty $recordRetry 'basis' '') -ne 'N/A' -or [string](Get-OptionalProperty $recordRetry 'infrastructure_reason' '') -ne 'N/A') {
        throw 'confirmation record retry.basis/infrastructure_reason must be N/A under the current AUTH'
    }
    if ((Get-ConfirmationField $record 'device_alias') -ne 'PHYS-1') { throw 'confirmation record device_alias must be PHYS-1' }
    $recordTargetRedacted = Get-OptionalProperty $record 'target_redacted' $null
    if ($null -eq $recordTargetRedacted -or $recordTargetRedacted.GetType() -ne [bool] -or -not $recordTargetRedacted) { throw 'confirmation record target_redacted must be true' }
    if ((Get-ConfirmationField $record 'code_sha') -ne [string]$Freeze.code_sha) { throw 'confirmation record code_sha does not match the freeze' }
    if ((Get-ConfirmationField $record 'runner_sha256') -ne [string]$Freeze.runner_sha256) { throw 'confirmation record runner_sha256 does not match the freeze' }
    if ((Get-ConfirmationField $record 'hdc_sha256') -ne [string]$Freeze.hdc.sha256) { throw 'confirmation record hdc_sha256 does not match the freeze' }
    if ((Get-ConfirmationField $record 'hdc_version') -ne [string]$Freeze.hdc.version) { throw 'confirmation record hdc_version does not match the freeze' }
    if ((Get-ConfirmationField $record 'confirmation_contract_sha256') -ne (Get-ConfirmationContractSha256 $Freeze)) { throw 'confirmation record confirmation_contract_sha256 does not match the current confirmation contract' }
    $expectedModel = (Get-ConfirmationField $record 'expected_model')
    $expectedBuild = (Get-ConfirmationField $record 'expected_build')
    $observedModel = (Get-ConfirmationField $record 'observed_model')
    $observedBuild = (Get-ConfirmationField $record 'observed_build')
    if ($expectedModel -ne [string]$Freeze.target_tuple.device_model -or $expectedBuild -ne [string]$Freeze.target_tuple.full_system_build) { throw 'confirmation record expected model/build do not match the freeze tuple' }
    if ($observedModel -ne $expectedModel -or $observedBuild -ne $expectedBuild) { throw 'confirmation record observed model/build do not match the expected frozen tuple' }
    $attempted = Get-OptionalProperty $record 'command_attempted' $null
    $completed = Get-OptionalProperty $record 'command_completed' $null
    if (-not (Test-JsonInteger $attempted) -or [long]$attempted -ne 3 -or -not (Test-JsonInteger $completed) -or [long]$completed -ne 3) {
        throw 'confirmation record command_attempted and command_completed must both be exactly 3 for a pass'
    }
    # ADJ-20260810-0001 (C6): command_count is the producer's compatibility alias of
    # command_completed; when present it must agree (integer, equal), never drift independently.
    $commandCount = Get-OptionalProperty $record 'command_count' $null
    if ($null -ne $commandCount -and (-not (Test-JsonInteger $commandCount) -or [long]$commandCount -ne [long]$completed)) {
        throw 'confirmation record command_count must equal command_completed (compatibility alias) for a pass'
    }
    $startedAt = Convert-ToDateTimeOffset (Get-ConfirmationField $record 'started_at')
    $endedAt = Convert-ToDateTimeOffset (Get-ConfirmationField $record 'ended_at')
    $frozenAt = Convert-ToDateTimeOffset (Get-OptionalProperty $Freeze 'preflight_inputs_frozen_at' $null)
    if ($null -eq $startedAt -or $null -eq $endedAt -or $null -eq $frozenAt) {
        throw 'confirmation record started_at/ended_at or freeze preflight_inputs_frozen_at invalid'
    }
    if ($startedAt -gt $endedAt) { throw 'confirmation record started_at must not be after ended_at' }
    if ($endedAt -gt $frozenAt) { throw 'confirmation record ended_at must be no later than freeze preflight_inputs_frozen_at' }
    # ADJ-20260810-0001 (C6): ready Live / ready DryRun additionally require the independent review
    # record mechanical gate (out-of-repo review record + companion bound to this freeze contract).
    [void](Assert-IndependentReviewRecord $Freeze)
    $script:MachineFreshConfirmation = [ordered]@{
        status = 'pass'
        authorization_id = $authorizationId
        record_sha256 = [string](Get-OptionalProperty $confirmation 'record_sha256' '')
        record_path_sha256 = Get-TextSha256 $recordPath
    }
    return [pscustomobject]@{ RecordPath = $recordPath; RecordSha256 = [string](Get-OptionalProperty $confirmation 'record_sha256' '') }
}

function Assert-IndependentReviewRecord {
    param([Parameter(Mandatory)]$Freeze)
    # ADJ-20260810-0001 (C6): ready Live / ready DryRun require a mechanical independent-review
    # record (out-of-repo JSON + matching .sha256 companion) proving the ready freeze contract was
    # reviewed by a separate reviewer role. This replaces the self-declared
    # independent_review_ready=true boolean as the execution gate: the boolean only represents
    # static contract/role readiness on a blocked confirmation freeze and never gates a ready
    # plan_status. The review record must bind the same freeze contract and the machine
    # confirmation record hash so the three objects (confirmation record, review record, freeze)
    # form one consistent chain. A blocked DryRun that declares review status=pass is FULLY
    # validated too (ValidateDeclaredPass: a blocked DryRun can never hide a broken binding);
    # status=pending or absent is allowed and skipped on a blocked DryRun, and a declared-pass
    # review can never ride on a pending/absent machine confirmation.
    if ($TargetBindingConfirm) { return $null }
    $planStatus = [string]$Freeze.plan_status
    $requirePass = ($script:ExecutionMode -eq 'live' -and -not $LiveSimulation) -or ($DryRun -and $planStatus -eq 'ready')
    $review = Get-OptionalProperty $Freeze 'independent_review_record' $null
    $reviewStatus = if ($null -eq $review) { '' } else { [string](Get-OptionalProperty $review 'status' '') }
    $validateReview = $requirePass -or ($DryRun -and $planStatus -eq 'blocked' -and $reviewStatus -eq 'pass')
    if (-not $validateReview) { return $null }
    if ($requirePass -and $null -eq $review) { throw 'a ready plan_status requires independent_review_record.status=pass with a bound out-of-repository review record' }
    if ($reviewStatus -ne 'pass') { throw 'independent_review_record.status must be pass for a ready plan_status; independent_review_ready=true alone is a static readiness marker and never an execution gate' }
    $reviewerRole = [string](Get-OptionalProperty $review 'reviewer_role' '')
    if ([string]::IsNullOrWhiteSpace($reviewerRole) -or $reviewerRole -ne [string]$Freeze.independent_reviewer_role) { throw 'independent_review_record.reviewer_role does not match the freeze independent_reviewer_role' }
    $recordPathInput = [string](Get-OptionalProperty $review 'record_path' '')
    if ([string]::IsNullOrWhiteSpace($recordPathInput)) { throw 'independent_review_record.record_path missing' }
    $recordPath = Get-NormalizedPath $recordPathInput
    if (-not (Test-Path -LiteralPath $recordPath -PathType Leaf)) { throw 'independent review record file missing' }
    if ($null -ne $script:RepoRoot -and ($recordPath -eq $script:RepoRoot -or (Test-IsUnderPath $recordPath $script:RepoRoot))) {
        throw 'independent review record must be outside the git repository'
    }
    Assert-NoReparseAncestor $recordPath
    $companionPath = $recordPath + '.sha256'
    Assert-NoReparseAncestor $companionPath
    if (-not (Test-Path -LiteralPath $companionPath -PathType Leaf)) { throw 'independent review record .sha256 companion missing; a lone record is never consumable' }
    if ([string](Get-Content -LiteralPath $companionPath -Raw).Trim() -ne (Get-FileSha256 $recordPath)) { throw 'independent review record .sha256 companion does not match the record bytes' }
    Assert-FileHash 'independent review record' $recordPath ([string](Get-OptionalProperty $review 'record_sha256' ''))
    $record = Get-Content -LiteralPath $recordPath -Raw | ConvertFrom-Json -Depth 40
    # ADJ-20260810-0001 (C6): exact-schema gate - any unknown top-level field (e.g. a target/serial/
    # secret canary) makes the review record un-consumable.
    $reviewAllowedFields = @('schema_version', 'record_kind', 'is_evidence', 'exception', 'campaign_id', 'evidence_id', 'code_sha', 'runner_sha256', 'confirmation_contract_sha256', 'machine_confirmation_sha256', 'reviewer_role', 'operator_role', 'verdict', 'blockers', 'majors', 'started_at', 'ended_at')
    foreach ($recordProperty in $record.PSObject.Properties) {
        if ($recordProperty.Name -notin $reviewAllowedFields) {
            throw "independent review record has an unknown top-level field: $($recordProperty.Name)"
        }
    }
    function Get-ReviewField {
        param($Object, [Parameter(Mandatory)][string]$Name)
        $value = [string](Get-OptionalProperty $Object $Name '')
        if ([string]::IsNullOrWhiteSpace($value)) { throw "independent review record missing or empty field: $Name" }
        return $value
    }
    $reviewSchema = Get-OptionalProperty $record 'schema_version' $null
    if (-not (Test-JsonInteger $reviewSchema) -or [long]$reviewSchema -ne 1) { throw 'independent review record schema_version must be 1' }
    if ((Get-ReviewField $record 'record_kind') -ne 'e3-ready-freeze-review') { throw 'independent review record record_kind mismatch' }
    if ((Get-ReviewField $record 'exception') -ne 'E3-PHYS-PREFLIGHT') { throw 'independent review record exception mismatch' }
    $reviewIsEvidence = Get-OptionalProperty $record 'is_evidence' $null
    if ($null -eq $reviewIsEvidence -or $reviewIsEvidence.GetType() -ne [bool] -or $reviewIsEvidence) { throw 'independent review record must be is_evidence=false' }
    if ((Get-ReviewField $record 'verdict') -ne 'pass') { throw 'independent review record verdict must be pass' }
    $blockers = Get-OptionalProperty $record 'blockers' $null
    $majors = Get-OptionalProperty $record 'majors' $null
    if (-not (Test-JsonInteger $blockers) -or [long]$blockers -ne 0 -or -not (Test-JsonInteger $majors) -or [long]$majors -ne 0) {
        throw 'independent review record requires blockers=0 and majors=0 for a pass verdict'
    }
    if ((Get-ReviewField $record 'reviewer_role') -ne $reviewerRole) { throw 'independent review record reviewer_role mismatch' }
    if ($reviewerRole -eq [string]$Freeze.operator_role) { throw 'independent review record reviewer_role must differ from the operator role' }
    if ((Get-ReviewField $record 'campaign_id') -ne $script:CandidateCampaignId -or (Get-ReviewField $record 'evidence_id') -ne $script:CandidateEvidenceId) {
        throw 'independent review record candidate IDs do not match the fixed AUTH candidate pair'
    }
    if ((Get-ReviewField $record 'code_sha') -ne [string]$Freeze.code_sha) { throw 'independent review record code_sha does not match the freeze' }
    if ((Get-ReviewField $record 'runner_sha256') -ne [string]$Freeze.runner_sha256) { throw 'independent review record runner_sha256 does not match the freeze' }
    if ((Get-ReviewField $record 'confirmation_contract_sha256') -ne (Get-ConfirmationContractSha256 $Freeze)) { throw 'independent review record confirmation_contract_sha256 does not match the current confirmation contract' }
    $machineConfirmation = Get-OptionalProperty $Freeze 'machine_fresh_confirmation' $null
    if ($null -eq $machineConfirmation) { throw 'independent review record requires a bound machine_fresh_confirmation on the freeze' }
    # ADJ-20260810-0001 (C6): a declared-pass review binds the machine confirmation hash, so the
    # machine side must itself be a fully validated pass; a pending/absent machine confirmation
    # can never anchor a declared-pass review.
    if ([string](Get-OptionalProperty $machineConfirmation 'status' '') -ne 'pass') { throw 'independent review record requires machine_fresh_confirmation.status=pass; a pending/absent machine confirmation cannot anchor a declared-pass review' }
    $machineSha = [string](Get-OptionalProperty $machineConfirmation 'record_sha256' '')
    if ([string]::IsNullOrWhiteSpace($machineSha)) { throw 'independent review record requires machine_fresh_confirmation.record_sha256 on the freeze' }
    if ((Get-ReviewField $record 'machine_confirmation_sha256') -ne $machineSha) { throw 'independent review record machine_confirmation_sha256 does not match the machine confirmation record' }
    $reviewStarted = Convert-ToDateTimeOffset (Get-ReviewField $record 'started_at')
    $reviewEnded = Convert-ToDateTimeOffset (Get-ReviewField $record 'ended_at')
    $frozenAt = Convert-ToDateTimeOffset (Get-OptionalProperty $Freeze 'preflight_inputs_frozen_at' $null)
    if ($null -eq $reviewStarted -or $null -eq $reviewEnded -or $null -eq $frozenAt) {
        throw 'independent review record started_at/ended_at or freeze preflight_inputs_frozen_at invalid'
    }
    if ($reviewStarted -gt $reviewEnded) { throw 'independent review record started_at must not be after ended_at' }
    if ($reviewEnded -gt $frozenAt) { throw 'independent review record ended_at must be no later than freeze preflight_inputs_frozen_at' }
    # ADJ-20260810-0001 (C6): the review happens AFTER the machine confirmation: the review start is
    # anchored to the bound machine confirmation record's ended_at, and the whole chain must fit
    # before the FINAL ready freeze's preflight_inputs_frozen_at (machine confirmation ended_at
    # <= review started_at <= review ended_at <= final freeze frozen_at). The blocked confirmation
    # freeze / ready draft keep a provisional frozen_at that is excluded from this gate.
    $machineRecordPath = Get-NormalizedPath ([string](Get-OptionalProperty $machineConfirmation 'record_path' ''))
    if (-not (Test-Path -LiteralPath $machineRecordPath -PathType Leaf)) { throw 'independent review record requires the bound machine confirmation record file' }
    $machineRecord = Get-Content -LiteralPath $machineRecordPath -Raw | ConvertFrom-Json -Depth 40
    $machineEndedAt = Convert-ToDateTimeOffset (Get-OptionalProperty $machineRecord 'ended_at' $null)
    if ($null -eq $machineEndedAt) { throw 'machine confirmation record ended_at invalid' }
    if ($machineEndedAt -gt $reviewStarted) { throw 'independent review must start no earlier than the machine confirmation ended_at' }
    $script:IndependentReviewRecord = [ordered]@{
        status = 'pass'
        reviewer_role = $reviewerRole
        record_sha256 = [string](Get-OptionalProperty $review 'record_sha256' '')
        record_path_sha256 = Get-TextSha256 $recordPath
    }
    return [pscustomobject]@{ RecordPath = $recordPath; RecordSha256 = [string](Get-OptionalProperty $review 'record_sha256' '') }
}

function Get-TargetBindingConfirmPlan {
    # ADJ-20260810-0001: the complete host-governed confirm plan is exactly the three frozen
    # target-binding probes already allowlisted by Get-HdcInvocation (Version / TupleModel /
    # TupleBuild). No install, staging, capture, cleanup, bundle, or process query may ever appear.
    return @(
        @{ operation = 'Version'; parameters = @{} },
        @{ operation = 'TupleModel'; parameters = @{} },
        @{ operation = 'TupleBuild'; parameters = @{} }
    )
}

function New-TargetBindingConfirmationRecord {
    param(
        [Parameter(Mandatory)]$Freeze,
        [Parameter(Mandatory)][string]$FreezeSha256,
        [Parameter(Mandatory)][string]$ConfirmationContractSha256,
        [Parameter(Mandatory)]$RepositoryBefore,
        [Parameter(Mandatory)][DateTimeOffset]$StartedAt,
        [Parameter(Mandatory)][DateTimeOffset]$EndedAt,
        [AllowNull()][string]$Verdict,
        [AllowNull()][string]$Reason,
        [AllowNull()][string]$ObservedVersion,
        [AllowNull()][string]$ObservedModel,
        [AllowNull()][string]$ObservedBuild,
        [Parameter(Mandatory)][int]$CommandAttempted,
        [Parameter(Mandatory)][int]$CommandCompleted
    )
    return [ordered]@{
        schema_version = 1
        record_kind = 'target-binding-confirmation'
        is_evidence = $false
        authorization_id = $script:AuthId
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = [string]$Freeze.campaign_id
        evidence_id = [string]$Freeze.evidence_id
        attempt = [string]$Freeze.attempt
        retry = [ordered]@{
            basis = [string]$Freeze.retry.basis
            infrastructure_reason = [string]$Freeze.retry.infrastructure_reason
        }
        plan_status = [string]$Freeze.plan_status
        device_alias = 'PHYS-1'
        target_redacted = $true
        code_sha = [string]$Freeze.code_sha
        runner_sha256 = [string]$Freeze.runner_sha256
        freeze_manifest_sha256 = $FreezeSha256
        # ADJ-20260810-0001 (C6): the record binds the STABLE confirmation contract (the
        # two-phase-invariant projection), not the full freeze contract: preflight_inputs_frozen_at
        # and other governance/time fields legitimately advance between the blocked confirmation
        # freeze and the final ready freeze, so the full-contract hash would never match at
        # consumption time. The consumer and the ready review record verify this same value.
        confirmation_contract_sha256 = $ConfirmationContractSha256
        hdc_sha256 = [string]$Freeze.hdc.sha256
        hdc_version = $ObservedVersion
        expected_model = [string]$Freeze.target_tuple.device_model
        expected_build = [string]$Freeze.target_tuple.full_system_build
        observed_model = $ObservedModel
        observed_build = $ObservedBuild
        started_at = $StartedAt.ToString('o')
        ended_at = $EndedAt.ToString('o')
        # ADJ-20260810-0001 (C6): honest attempted/completed counts; command_count is a compatibility
        # alias of command_completed. A pass verdict requires attempted=completed=3 (consumer-enforced).
        command_attempted = $CommandAttempted
        command_completed = $CommandCompleted
        command_count = $CommandCompleted
        repository_fingerprint = $RepositoryBefore.Fingerprint
        verdict = $Verdict
        reason = $(if ([string]::IsNullOrEmpty([string]$Reason)) { 'N/A' } else { [string]$Reason })
    }
}

function Write-TargetBindingConfirmationRecordPair {
    param([Parameter(Mandatory)][string]$RecordPath, [Parameter(Mandatory)]$Record)
    # ADJ-20260810-0001 (C6): double-file completion marker. The JSON record and its .sha256
    # companion are the atomic completion pair: a consumer only accepts the record when BOTH files
    # exist and the companion matches the record bytes. The JSON tmp and the companion tmp are
    # written first (no-clobber CreateNew) and the hash is recomputed over the tmp JSON; the JSON is
    # atomic-moved into place, then the companion is atomic-moved LAST as the completion marker. If
    # the companion move fails, the JSON is left as an orphan that is never consumed or overwritten.
    $companionPath = $RecordPath + '.sha256'
    foreach ($candidate in @($RecordPath, $companionPath)) {
        if (Test-Path -LiteralPath $candidate) { throw "confirmation output already exists and is immutable: $candidate" }
    }
    $tmpJson = $RecordPath + '.tmp-' + [guid]::NewGuid().ToString('N')
    $tmpSha = $companionPath + '.tmp-' + [guid]::NewGuid().ToString('N')
    if (Test-Path -LiteralPath $tmpJson) { throw 'confirmation JSON temp candidate already exists' }
    if (Test-Path -LiteralPath $tmpSha) { throw 'confirmation companion temp candidate already exists' }
    $jsonMoved = $false
    try {
        $jsonBytes = [Text.Encoding]::UTF8.GetBytes(($Record | ConvertTo-Json -Depth 40) + [Environment]::NewLine)
        $tmpStream = [IO.FileStream]::new($tmpJson, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $tmpStream.Write($jsonBytes, 0, $jsonBytes.Length); $tmpStream.Flush($true) } finally { $tmpStream.Dispose() }
        $sha = Get-FileSha256 $tmpJson
        $shaBytes = [Text.Encoding]::UTF8.GetBytes($sha + [Environment]::NewLine)
        $shaStream = [IO.FileStream]::new($tmpSha, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $shaStream.Write($shaBytes, 0, $shaBytes.Length); $shaStream.Flush($true) } finally { $shaStream.Dispose() }
        if ((Get-FileSha256 $tmpJson) -ne $sha) { throw 'confirmation record hash recompute mismatch' }
        [IO.File]::Move($tmpJson, $RecordPath)
        $jsonMoved = $true
        [IO.File]::Move($tmpSha, $companionPath)
    } finally {
        # A moved-but-uncompanioned JSON is an orphan: never delete it (it is the only trace of the
        # failure and the consumer requires the companion, so it can never be consumed or overwritten).
        if (-not $jsonMoved -and (Test-Path -LiteralPath $tmpJson)) { Remove-Item -LiteralPath $tmpJson -Force -ErrorAction SilentlyContinue }
        if (Test-Path -LiteralPath $tmpSha) { Remove-Item -LiteralPath $tmpSha -Force -ErrorAction SilentlyContinue }
    }
    return $sha
}

function Invoke-TargetBindingConfirm {
    param(
        [Parameter(Mandatory)]$Freeze,
        [Parameter(Mandatory)][string]$FreezeSha256,
        [Parameter(Mandatory)][string]$ConfirmationContractSha256,
        [Parameter(Mandatory)]$RepositoryBefore
    )
    # ADJ-20260810-0001 host-governed machine fresh confirmation. The confirmation record is a
    # single-use immutable out-of-repository double-file object (JSON + .sha256 companion); the real
    # target never enters it. Pre-record gate failures (record path issues, a pre-existing
    # record/companion) throw and exit 1 with NO record written. Probe/preflight failures (bad
    # target token, environment, tuple drift) and record-write failures produce a best-effort
    # blocked record + companion and exit 2; no campaign roots are ever created. A pass verdict
    # requires attempted=completed=3 and a complete double-file pair.
    $recordPath = Get-NormalizedPath $ConfirmationRecord
    if (Test-IsUnderPath $recordPath $script:RepoRoot) { throw 'ConfirmationRecord must be outside the git repository' }
    Assert-NoReparseAncestor $recordPath
    $companionPath = $recordPath + '.sha256'
    Assert-NoReparseAncestor $companionPath
    # Single-use no-clobber gates: the record AND its companion must both be absent up front. A
    # leftover orphan JSON from a previous companion failure is never overwritten and, because the
    # consumer requires both files, is never consumable.
    if (Test-Path -LiteralPath $recordPath) { throw 'ConfirmationRecord already exists; target-binding confirmation is single-use' }
    if (Test-Path -LiteralPath $companionPath) { throw 'ConfirmationRecord .sha256 companion already exists; target-binding confirmation is single-use' }
    $startedAt = Get-Now
    $verdict = 'blocked'
    $reason = $null
    $observedVersion = $null
    $observedModel = $null
    $observedBuild = $null
    $commandAttempted = 0
    $commandCompleted = 0
    try {
        Assert-TargetEnvironment
        foreach ($step in @(Get-TargetBindingConfirmPlan)) {
            $commandAttempted++
            $result = Invoke-HdcOperation $step.operation $step.parameters
            $commandCompleted++
            switch ($step.operation) {
                'Version' { $observedVersion = $result.Stdout.Trim() }
                'TupleModel' { $observedModel = $result.Stdout.Trim() }
                'TupleBuild' { $observedBuild = $result.Stdout.Trim() }
            }
        }
        if ($observedVersion -ne [string]$Freeze.hdc.version) { throw 'preflight: frozen HDC version mismatch' }
        if ($observedModel -ne [string]$Freeze.target_tuple.device_model) { throw 'preflight: frozen device model mismatch' }
        if ($observedBuild -ne [string]$Freeze.target_tuple.full_system_build) { throw 'preflight: frozen full system build mismatch' }
        # ADJ-20260810-0001 (C6): mechanical pass exit - a pass verdict requires exactly three HDC
        # processes started and attempted=completed=3, asserted BEFORE the record is written so a
        # pass double-file pair is never generated and then downgraded; any mismatch stays blocked.
        # A blocked record may carry any attempted/completed <= 3 (partial probe progress).
        if ($commandAttempted -ne 3 -or $commandCompleted -ne 3 -or $script:HdcProcessStartCount -ne 3) {
            throw "machine confirmation pass requires exactly 3 HDC processes started and attempted/completed=3 (hdc_processes_started=$($script:HdcProcessStartCount), attempted=$commandAttempted, completed=$commandCompleted)"
        }
        $verdict = 'pass'
    } catch {
        # Probe/tuple failure: still write a best-effort blocked record + companion (exit 2).
        $reason = Protect-SensitiveText $_.Exception.Message
    }
    $endedAt = Get-Now
    # Every device-observed value is protected before it can enter the record; the real target
    # never appears in any field (Protect-SensitiveText also covers observed version/model/build).
    $safeObservedVersion = Protect-SensitiveText $observedVersion
    $safeObservedModel = Protect-SensitiveText $observedModel
    $safeObservedBuild = Protect-SensitiveText $observedBuild
    $record = New-TargetBindingConfirmationRecord $Freeze $FreezeSha256 $ConfirmationContractSha256 $RepositoryBefore $startedAt $endedAt $verdict $reason $safeObservedVersion $safeObservedModel $safeObservedBuild $commandAttempted $commandCompleted
    $recordSha = $null
    try {
        $recordSha = Write-TargetBindingConfirmationRecordPair $recordPath $record
        # Return and disk stay the same source: recompute over the final moved file.
        $recordSha = Get-FileSha256 $recordPath
    } catch {
        # A companion failure may leave an orphan JSON. It is never deleted, never overwritten, and
        # never consumable (the consumer requires both files); the run returns blocked with exit 2.
        $writeFailure = Protect-SensitiveText $_.Exception.Message
        $verdict = 'blocked'
        $recordSha = $null
        if ([string]::IsNullOrEmpty([string]$reason)) { $reason = "record-write-failed: $writeFailure" } else { $reason = "$reason; record-write-failed: $writeFailure" }
    }
    return [pscustomobject]@{
        Verdict = $verdict
        Reason = $reason
        RecordPath = $recordPath
        RecordSha256 = $recordSha
        CommandAttempted = $commandAttempted
        CommandCompleted = $commandCompleted
        StartedAt = $startedAt
        EndedAt = $endedAt
    }
}

function Test-Sha256Hex {
    param([AllowNull()][string]$Value)
    return (-not [string]::IsNullOrWhiteSpace($Value)) -and $Value -match '^[0-9a-f]{64}$'
}

function Get-PriorBlockedBinding {
    param([Parameter(Mandatory)]$Freeze)
    # Optional governance projection only: N/A / missing, or pure hash object. No path/raw copy or re-verification.
    $prior = Get-OptionalProperty $Freeze 'prior_blocked_binding' $null
    if ($null -eq $prior) { return $null }
    if ($prior -is [string]) {
        if ([string]$prior -eq 'N/A' -or [string]::IsNullOrWhiteSpace([string]$prior)) { return $null }
        throw "prior_blocked_binding must be N/A or an object with source='consumed-blocked', evidence_id, and three SHA-256 hashes"
    }
    $source = [string](Get-OptionalProperty $prior 'source' '')
    $evidenceId = [string](Get-OptionalProperty $prior 'evidence_id' '')
    $scenarioResultsSha = [string](Get-OptionalProperty $prior 'scenario_results_sha256' '')
    $hashManifestSha = [string](Get-OptionalProperty $prior 'hash_manifest_sha256' '')
    $campaignSealSha = [string](Get-OptionalProperty $prior 'campaign_seal_sha256' '')
    $provided = @(@($source, $evidenceId, $scenarioResultsSha, $hashManifestSha, $campaignSealSha) | Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) -and [string]$_ -ne 'N/A' })
    if ($provided.Count -eq 0) { return $null }
    if ($source -ne 'consumed-blocked') {
        throw "prior_blocked_binding.source must be 'consumed-blocked'"
    }
    if ([string]::IsNullOrWhiteSpace($evidenceId) -or $evidenceId -eq 'N/A') {
        throw 'prior_blocked_binding.evidence_id must be non-empty'
    }
    foreach ($pair in @(
        @{ Key = 'scenario_results_sha256'; Value = $scenarioResultsSha },
        @{ Key = 'hash_manifest_sha256'; Value = $hashManifestSha },
        @{ Key = 'campaign_seal_sha256'; Value = $campaignSealSha }
    )) {
        if (-not (Test-Sha256Hex ([string]$pair.Value))) {
            throw "prior_blocked_binding.$($pair.Key) must be a final SHA-256"
        }
    }
    return [ordered]@{
        source = 'consumed-blocked'
        evidence_id = $evidenceId
        scenario_results_sha256 = $scenarioResultsSha.ToLowerInvariant()
        hash_manifest_sha256 = $hashManifestSha.ToLowerInvariant()
        campaign_seal_sha256 = $campaignSealSha.ToLowerInvariant()
    }
}

function Initialize-PriorBlockedBinding {
    param([Parameter(Mandatory)]$Freeze)
    $script:PriorBlockedBinding = Get-PriorBlockedBinding $Freeze
    if ($null -eq $script:PriorBlockedBinding) { return }
    Add-TranscriptRecord 'prior-blocked-binding' ([ordered]@{
        source = [string]$script:PriorBlockedBinding.source
        evidence_id = [string]$script:PriorBlockedBinding.evidence_id
        scenario_results_sha256 = [string]$script:PriorBlockedBinding.scenario_results_sha256
        hash_manifest_sha256 = [string]$script:PriorBlockedBinding.hash_manifest_sha256
        campaign_seal_sha256 = [string]$script:PriorBlockedBinding.campaign_seal_sha256
        binding_source = 'freeze-manifest'
    })
}

function Test-PhysicalTargetToken {
    param([AllowNull()][string]$Target)
    if ([string]::IsNullOrWhiteSpace($Target)) { return $false }
    return $Target -eq $Target.Trim() -and $Target -notmatch '[\s,;]' -and $Target -notmatch '^(?i:PHYS-1|<.+>)$'
}

function Assert-TargetEnvironment {
    $target = [Environment]::GetEnvironmentVariable('PHYS_1_TARGET', 'Process')
    if (-not (Test-PhysicalTargetToken $target)) { throw 'PHYS_1_TARGET must contain exactly one real target token' }
    $script:ActualTarget = $target
}

function Assert-ExactCommandParameters {
    param([Parameter(Mandatory)][string]$Operation, [Parameter(Mandatory)][hashtable]$Parameters)
    $table = @{
        Version = @(); TupleModel = @(); TupleBuild = @(); MkdirStaging = @(); RemoveStaging = @(); StagingProbe = @(); SendA = @(); SendB = @()
        InstallA = @(); InstallB = @(); FaultA = @(); FaultB = @(); HilogStream = @()
        BundleDump = @('Bundle'); PidOf = @('Bundle'); Uninstall = @('Bundle'); StartEntry = @('Bundle')
        ScreenCap = @('Name'); DumpLayout = @('Name'); ReceiveScreen = @('Name'); ReceiveLayout = @('Name')
        ForceStop = @('Bundle', 'Reason')
    }
    if (-not $table.ContainsKey($Operation)) { throw "command rejected: operation '$Operation' is not allowlisted" }
    $required = @($table[$Operation])
    foreach ($name in $required) {
        if (-not $Parameters.ContainsKey($name) -or [string]::IsNullOrWhiteSpace([string]$Parameters[$name])) {
            throw "command rejected: operation '$Operation' requires parameter '$name'"
        }
    }
    foreach ($name in $Parameters.Keys) {
        if ($name -notin $required) { throw "command rejected: operation '$Operation' does not accept parameter '$name'" }
    }
}

function Get-HdcInvocation {
    param([Parameter(Mandatory)][string]$Operation, [hashtable]$Parameters = @{})
    Assert-ExactCommandParameters $Operation $Parameters
    $bundle = if ($Parameters.ContainsKey('Bundle')) { [string]$Parameters.Bundle } else { '' }
    if ($bundle -and $bundle -notin @($script:BundleA, $script:BundleB)) { throw 'command rejected: bundle outside A/B allowlist' }
    if ($Parameters.ContainsKey('Name') -and [string]$Parameters.Name -notmatch '^scenario-[1-7](?:-[a-z]+)*$') {
        throw 'command rejected: capture name outside fixed scenario paths'
    }
    $common = @('-t', '<PHYS_1_TARGET>')
    switch ($Operation) {
        'Version' { return @('version') }
        'TupleModel' { return $common + @('shell', 'param', 'get', 'const.product.model') }
        'TupleBuild' { return $common + @('shell', 'param', 'get', 'const.product.software.version') }
        'BundleDump' { return $common + @('shell', 'bm', 'dump', '-n', $bundle) }
        # ADJ-20260808-0001: pidof targets the <bundle>:vpn Extension ability process, never the
        # bundle UI process. Under a normal Stop with the app UI still visible, bundle-level pidof is
        # physically unsatisfiable (the Entry UI process keeps running), so the strict process-boundary
        # fallback probes the Extension process that actually terminates on stop. BundleDump still
        # proves the bundle/main App remains installed. Directed exact-name pidof only; no broad
        # process list. The freeze field process_probe_target ('<bundle>:vpn') freezes this semantics.
        'PidOf' { return $common + @('shell', 'pidof', "${bundle}:vpn") }
        'MkdirStaging' { return $common + @('shell', 'mkdir', '-p', "$($script:Staging)/a", "$($script:Staging)/b", "$($script:Staging)/capture") }
        'RemoveStaging' { return $common + @('shell', 'rm', '-rf', $script:Staging) }
        'StagingProbe' { return $common + @('shell', 'ls', '-ld', $script:Staging) }
        'SendA' { return $common + @('file', 'send', '<HAP_A>', "$($script:Staging)/a/a.hap") }
        'SendB' { return $common + @('file', 'send', '<HAP_B>', "$($script:Staging)/b/b.hap") }
        'InstallA' { return $common + @('shell', 'bm', 'install', '-p', "$($script:Staging)/a") }
        'InstallB' { return $common + @('shell', 'bm', 'install', '-p', "$($script:Staging)/b") }
        'Uninstall' { return $common + @('shell', 'bm', 'uninstall', '-n', $bundle) }
        'StartEntry' { return $common + @('shell', 'aa', 'start', '-a', $script:Ability, '-b', $bundle, '-m', $script:Module) }
        'ForceStop' {
            if ([string]$Parameters.Reason -notin @('exception-cleanup', 'final-cleanup')) { throw 'command rejected: force-stop is cleanup-only' }
            return $common + @('shell', 'aa', 'force-stop', $bundle)
        }
        'HilogStream' { return $common + @('shell', 'hilog', '-T', 'E3PhysVpn', '-v', 'year', '-v', 'zone') }
        'FaultA' { return $common + @('shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', "*$($script:BundleA)*", '-print') }
        'FaultB' { return $common + @('shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', "*$($script:BundleB)*", '-print') }
        'ScreenCap' { return $common + @('shell', 'uitest', 'screenCap', '-p', "$($script:Staging)/capture/$([string]$Parameters.Name).png") }
        'DumpLayout' { return $common + @('shell', 'uitest', 'dumpLayout', '-p', "$($script:Staging)/capture/$([string]$Parameters.Name).json", '-i') }
        'ReceiveScreen' { return $common + @('file', 'recv', "$($script:Staging)/capture/$([string]$Parameters.Name).png", '<RAW_CAPTURE>') }
        'ReceiveLayout' { return $common + @('file', 'recv', "$($script:Staging)/capture/$([string]$Parameters.Name).json", '<RAW_CAPTURE>') }
    }
}

function Get-LiveHdcArguments {
    param([Parameter(Mandatory)][string[]]$AuditArguments, [Parameter(Mandatory)][string]$Operation, [Parameter(Mandatory)][hashtable]$Parameters)
    $liveArguments = @($AuditArguments | ForEach-Object {
        switch ($_ ) {
            '<PHYS_1_TARGET>' { $script:ActualTarget }
            '<HAP_A>' { Get-NormalizedPath $HapA }
            '<HAP_B>' { Get-NormalizedPath $HapB }
            '<RAW_CAPTURE>' {
                $extension = if ($Operation -eq 'ReceiveScreen') { '.png' } else { '.json' }
                Join-Path $script:RawPath ('capture-' + [string]$Parameters.Name + $extension)
            }
            default { $_ }
        }
    })
    return [string[]]$liveArguments
}

function New-SimulatedUiNode {
    param([hashtable]$Attributes, [object[]]$Children = @())
    # ADJ-20260808-0003 (C6): real layout facts are a top-level array; every node is
    # { attributes: { bundleName, type, id, key, text, ... }, children: [...] }. Build one such
    # node for the simulated (and future direct-field tolerant) layout fixture.
    return [ordered]@{ attributes = $Attributes; children = @($Children) }
}

function Get-SimulatedLayoutDocument {
    param([Parameter(Mandatory)][string]$Name)
    $profiles = Get-OptionalProperty $script:Simulation 'layout_profiles' $null
    $override = if ($null -ne $profiles) { Get-OptionalProperty $profiles $Name $null } else { $null }
    if ($null -ne $override -and $override -isnot [string]) { return $override }
    $profile = if ($null -ne $override) { [string]$override } elseif ($Name -match 'authorization') { 'authorization' }
        elseif ($Name -match 'settings-vpn-page') { 'settings-vpn' }
        elseif ($Name -match 'app-info') { 'settings-app-info-a' }
        elseif ($Name -match '(?:entry-a|after-allow|reactivation|scenario-3-|scenario-7-)') { 'entry-a' }
        elseif ($Name -match '(?:entry-b|after-deny|scenario-4-|scenario-6-conflict)') { 'entry-b' }
        else { 'generic' }
    # ADJ-20260808-0002 (C6): layout-ready-delay simulation knob. A capture name listed in
    # `layout_ready_delays` stays "not ready" (a generic layout that fails every profile) until
    # the given number of (virtual) seconds have elapsed since its FIRST capture attempt. This
    # models a fast operator whose platform popup/dialog renders a few seconds later, and lets
    # the bounded same-name layout resample verify it converges to the real layout in time.
    $readyDelays = Get-OptionalProperty $script:Simulation 'layout_ready_delays' $null
    if ($null -ne $readyDelays -and $null -ne (Get-OptionalProperty $readyDelays $Name $null)) {
        $delay = [double](Get-OptionalProperty $readyDelays $Name 0.0)
        if (-not $script:SimulationLayoutFirstAttempt.ContainsKey($Name)) {
            $script:SimulationLayoutFirstAttempt[$Name] = Get-Now
        }
        $elapsed = ((Get-Now) - [DateTimeOffset]$script:SimulationLayoutFirstAttempt[$Name]).TotalSeconds
        if ($elapsed -lt $delay) {
            return @(New-SimulatedUiNode @{ bundleName = 'generic'; type = 'root'; id = ''; key = ''; text = '' })
        }
    }
    switch ($profile) {
        # ADJ-20260808-0003 (C6): simulated layouts are sanitized STRUCTURE fixtures that mirror
        # the real attributes/children array shape (top-level array, every node carries an
        # `attributes` object plus `children`). They are NOT byte copies of any sealed raw; the
        # authorization/settings facts below are the minimal documented surface for each profile.
        'authorization' {
            return @(
                New-SimulatedUiNode @{ bundleName = 'com.huawei.hmos.vpndialog'; type = 'Dialog'; id = ''; key = ''; text = 'E3 Physical VPN Preflight' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = '是否允许使用 VPN？' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'permission_cancel_button'; key = 'permission_cancel_button'; text = '取消' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'permission_allow_button'; key = 'permission_allow_button'; text = '允许' })
                )
            )
        }
        'entry-a' {
            return @(
                New-SimulatedUiNode @{ bundleName = $script:BundleA; type = 'root'; id = ''; key = ''; text = '' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'start-vpn'; key = 'start-vpn'; text = 'Start VPN' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'stop-vpn'; key = 'stop-vpn'; text = 'Stop VPN' })
                )
            )
        }
        'entry-b' {
            return @(
                New-SimulatedUiNode @{ bundleName = $script:BundleB; type = 'root'; id = ''; key = ''; text = '' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'start-vpn'; key = 'start-vpn'; text = 'Start VPN' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'stop-vpn'; key = 'stop-vpn'; text = 'Stop VPN' })
                )
            )
        }
        'settings-vpn' {
            return @(
                New-SimulatedUiNode @{ bundleName = 'com.huawei.hmos.settings'; type = 'root'; id = ''; key = ''; text = '' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = 'Setting.MobileNetwork.vpn_group_group.vpn_settings.title'; key = 'Setting.MobileNetwork.vpn_group_group.vpn_settings.title'; text = 'VPN' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = 'Setting.MobileNetwork.vpn_group_group.vpn_settings'; key = 'Setting.MobileNetwork.vpn_group_group.vpn_settings'; text = '没有 VPN' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = ''; key = ''; text = '添加 VPN 网络' })
                )
            )
        }
        'settings-vpn-fake-app' {
            # ADJ-20260808-0003: an ordinary app page that merely contains the word VPN must not
            # match the settings profiles (owner bundle + stable resource id are required).
            return @(
                New-SimulatedUiNode @{ bundleName = 'cn.alfadb.netbird.e3physvpna'; type = 'root'; id = ''; key = ''; text = '' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = 'VPN settings' }),
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = 'VPN connection' })
                )
            )
        }
        'settings-app-info-a' {
            return @(
                New-SimulatedUiNode @{ bundleName = 'com.huawei.hmos.settings'; type = 'root'; id = ''; key = ''; text = ''; visible = 'true' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'NavDestination'; id = 'Setting.AppDetail'; key = 'Setting.AppDetail'; text = ''; visible = 'true' } @(
                        (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = 'Setting.AppDetail.title_id'; key = 'Setting.AppDetail.title_id'; text = 'E3 Preflight A'; visible = 'true' }),
                        (New-SimulatedUiNode @{ bundleName = ''; type = 'Button'; id = 'force_stop_button'; key = 'force_stop_button'; text = '强行停止'; visible = 'true' })
                    ))
                )
            )
        }
        'wrong-page' {
            return @(
                New-SimulatedUiNode @{ bundleName = 'com.example.unrelated'; type = 'root'; id = ''; key = ''; text = '' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = 'Unrelated page' })
                )
            )
        }
        'authorization-missing-controls' {
            return @(
                New-SimulatedUiNode @{ bundleName = 'com.huawei.hmos.vpndialog'; type = 'Dialog'; id = ''; key = ''; text = 'E3 Physical VPN Preflight' } @(
                    (New-SimulatedUiNode @{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = '是否允许使用 VPN？' })
                )
            )
        }
        default {
            return @(New-SimulatedUiNode @{ bundleName = 'generic'; type = 'root'; id = ''; key = ''; text = '' })
        }
    }
}

function Get-SimulationHdcResult {
    param([Parameter(Mandatory)][string]$Operation, [Parameter(Mandatory)][hashtable]$Parameters)
    if (-not $script:HdcOperationCounts.ContainsKey($Operation)) { $script:HdcOperationCounts[$Operation] = 0 }
    $script:HdcOperationCounts[$Operation]++
    $occurrence = [int]$script:HdcOperationCounts[$Operation]
    foreach ($failure in @(Get-OptionalProperty $script:Simulation 'hdc_failures' @())) {
        if ([string](Get-OptionalProperty $failure 'operation' '') -eq $Operation -and [int](Get-OptionalProperty $failure 'occurrence' 1) -eq $occurrence) {
            return [pscustomobject]@{
                ExitCode = [int](Get-OptionalProperty $failure 'exit_code' 1)
                Stdout = [string](Get-OptionalProperty $failure 'stdout' '')
                Stderr = [string](Get-OptionalProperty $failure 'stderr' 'simulated command failure')
                Simulated = $true
            }
        }
    }
    $captureFailures = @(Get-OptionalProperty $script:Simulation 'capture_failures' @())
    if ($Parameters.ContainsKey('Name') -and [string]$Parameters.Name -in $captureFailures) {
        return [pscustomobject]@{ ExitCode = 9; Stdout = ''; Stderr = 'simulated unknown capture command'; Simulated = $true }
    }
    $stdout = switch ($Operation) {
        'Version' { [string](Get-OptionalProperty $script:Simulation 'hdc_version' 'SELFTEST-HDC-1.0') }
        'TupleModel' { 'PLA-AL10' }
        'TupleBuild' { 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' }
        'BundleDump' {
            $bundle = [string]$Parameters.Bundle
            $installed = ($bundle -eq $script:BundleA -and $script:SimulationInstalledA) -or ($bundle -eq $script:BundleB -and $script:SimulationInstalledB)
            if ($installed) { '{ "app": { "bundleName": "' + $bundle + '" } }' } else { 'error: failed to get information and the parameters may be wrong.' }
        }
        'PidOf' { if ($script:SimulationActiveBundles.Contains([string]$Parameters.Bundle)) { '4242' } else { '' } }
        'InstallA' { 'install bundle successfully.' }
        'InstallB' { 'install bundle successfully.' }
        'StagingProbe' {
            if ($script:SimulationStagingPresent) { "drwxrwxrwx 3 shell shell 4096 2026-01-01 00:00 $($script:Staging)" } else { "ls: $($script:Staging): No such file or directory" }
        }
        default { 'SIMULATED_OK' }
    }
    $exitCode = 0
    if ($Operation -eq 'StagingProbe' -and -not $script:SimulationStagingPresent) { $exitCode = 1 }
    if ($Operation -eq 'MkdirStaging' -or $Operation -eq 'SendA' -or $Operation -eq 'SendB') { $script:SimulationStagingPresent = $true }
    if ($Operation -eq 'RemoveStaging') { $script:SimulationStagingPresent = $false }
    if ($Operation -eq 'InstallA') { $script:SimulationInstalledA = $true }
    if ($Operation -eq 'InstallB') { $script:SimulationInstalledB = $true }
    if ($Operation -eq 'ForceStop') { [void]$script:SimulationActiveBundles.Remove([string]$Parameters.Bundle) }
    if ($Operation -eq 'Uninstall') {
        [void]$script:SimulationActiveBundles.Remove([string]$Parameters.Bundle)
        if ([string]$Parameters.Bundle -eq $script:BundleA) { $script:SimulationInstalledA = $false }
        if ([string]$Parameters.Bundle -eq $script:BundleB) { $script:SimulationInstalledB = $false }
    }
    if ($Operation -in @('ReceiveScreen', 'ReceiveLayout')) {
        $extension = if ($Operation -eq 'ReceiveScreen') { '.png' } else { '.json' }
        $destination = Join-Path $script:RawPath ('capture-' + [string]$Parameters.Name + $extension)
        if ($Operation -eq 'ReceiveScreen') {
            [IO.File]::WriteAllBytes($destination, [byte[]](1, 2, 3, 4))
        } else {
            Write-JsonFile $destination (Get-SimulatedLayoutDocument ([string]$Parameters.Name))
        }
    }
    return [pscustomobject]@{ ExitCode = $exitCode; Stdout = $stdout; Stderr = ''; Simulated = $true }
}

function Invoke-HdcOperation {
    param(
        [Parameter(Mandatory)][string]$Operation,
        [hashtable]$Parameters = @{},
        [switch]$AllowFailure,
        [int]$TimeoutSeconds = $HdcTimeoutSeconds
    )
    $auditArguments = Get-HdcInvocation -Operation $Operation -Parameters $Parameters
    $script:HdcLogicalCallCount++
    Add-TranscriptRecord 'hdc-command' ([ordered]@{ operation = $Operation; executable = '<HDC_PATH>'; arguments = $auditArguments; timeout_seconds = $TimeoutSeconds })
    if ($DryRun) {
        $result = [pscustomobject]@{ ExitCode = 0; Stdout = 'DRY_RUN_NOT_EXECUTED'; Stderr = ''; Simulated = $true }
    } elseif ($LiveSimulation) {
        $result = Get-SimulationHdcResult $Operation $Parameters
    } else {
        $liveArguments = Get-LiveHdcArguments $auditArguments $Operation $Parameters
        $startInfo = [Diagnostics.ProcessStartInfo]::new()
        $startInfo.FileName = $HdcPath
        $startInfo.UseShellExecute = $false
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true
        foreach ($argument in $liveArguments) { [void]$startInfo.ArgumentList.Add($argument) }
        $nativeProcess = [Diagnostics.Process]::new()
        $nativeProcess.StartInfo = $startInfo
        try {
            if (-not $nativeProcess.Start()) { throw 'Process.Start returned false' }
            $script:HdcProcessStartCount++
            $stdoutTask = $nativeProcess.StandardOutput.ReadToEndAsync()
            $stderrTask = $nativeProcess.StandardError.ReadToEndAsync()
            $deadline = [DateTimeOffset]::Now.AddSeconds($TimeoutSeconds)
            $exited = $false
            while (-not $exited -and [DateTimeOffset]::Now -lt $deadline) {
                $remainingMilliseconds = [int][Math]::Max(1, [Math]::Min(250, ($deadline - [DateTimeOffset]::Now).TotalMilliseconds))
                $exited = $nativeProcess.WaitForExit($remainingMilliseconds)
                if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
            }
            if (-not $exited) {
                try { $nativeProcess.Kill($true) } catch {}
                [void]$nativeProcess.WaitForExit(5000)
                $result = [pscustomobject]@{ ExitCode = 124; Stdout = $stdoutTask.GetAwaiter().GetResult(); Stderr = "HDC timeout after $TimeoutSeconds seconds"; Simulated = $false }
            } else {
                $nativeProcess.WaitForExit()
                $result = [pscustomobject]@{ ExitCode = $nativeProcess.ExitCode; Stdout = $stdoutTask.GetAwaiter().GetResult(); Stderr = $stderrTask.GetAwaiter().GetResult(); Simulated = $false }
            }
        } catch {
            $result = [pscustomobject]@{ ExitCode = 125; Stdout = ''; Stderr = "HDC Process.Start exception: $($_.Exception.Message)"; Simulated = $false }
        } finally {
            $nativeProcess.Dispose()
        }
    }
    Add-TranscriptRecord 'hdc-result' ([ordered]@{
        operation = $Operation
        exit_code = $result.ExitCode
        stdout = [string]$result.Stdout
        stderr = [string]$result.Stderr
        simulated = [bool]$result.Simulated
    })
    if ($result.ExitCode -in @(124, 125) -or [string]$result.Stderr -match '(?i)\btimeout\b') {
        $script:InfrastructureReasonObserved = 'hdc-usb-interruption'
    }
    if ($result.ExitCode -ne 0 -and -not $AllowFailure) {
        $safeError = Protect-SensitiveText ([string]$result.Stderr)
        throw "HDC operation failed: $Operation exit=$($result.ExitCode) stderr=$safeError"
    }
    return $result
}

function Get-HdcCombinedText {
    param([Parameter(Mandatory)]$Result)
    return (([string]$Result.Stdout) + "`n" + ([string]$Result.Stderr)).Trim()
}

function Get-HdcInstallAssessment {
    param([Parameter(Mandatory)]$Result)
    # Three-state install outcome. Exit 0 alone is never sufficient.
    # functional_fail only for explicit rejection evidence; avoid broad \berror\b|\bfail patterns.
    $exitCode = [int]$Result.ExitCode
    $text = Get-HdcCombinedText $Result
    if ($exitCode -in @(124, 125)) {
        return [pscustomobject]@{ Status = 'infrastructure'; Reason = "install-exit-$exitCode" }
    }
    $reject = $text -match '(?i)(?:error:\s*failed\s+to\s+(?:execute\s+your\s+command|install)|install\s+bundle\s+fail|install\s+fail(?:ed|ure)?\b|signature\s+(?:reject(?:ed)?|invalid|fail|mismatch)|profile\s+(?:reject(?:ed)?|mismatch|not\s+(?:match|found))|device\s+not\s+(?:in\s+)?profile|(?:error[_ ]?code|err(?:or)?code)\s*[=:]?\s*\d+)'
    if ($reject) {
        return [pscustomobject]@{ Status = 'functional_fail'; Reason = 'install-rejected' }
    }
    if ($exitCode -eq 0 -and $text -match '(?i)install bundle successfully') {
        return [pscustomobject]@{ Status = 'pass'; Reason = 'install-success-string' }
    }
    return [pscustomobject]@{ Status = 'blocked'; Reason = 'install-outcome-uncertain' }
}

function Get-BundleDumpAssessment {
    param([Parameter(Mandatory)]$Result, [Parameter(Mandatory)][string]$Bundle)
    # Dump confirms presence only. Unavailable / permission / absence / uncertain => blocked, never functional_fail.
    $exitCode = [int]$Result.ExitCode
    $text = Get-HdcCombinedText $Result
    if ($exitCode -in @(124, 125)) {
        return [pscustomobject]@{ Status = 'infrastructure'; Reason = "dump-exit-$exitCode" }
    }
    if ($exitCode -ne 0) {
        return [pscustomobject]@{ Status = 'blocked'; Reason = 'bundle-dump-unavailable' }
    }
    if ([string]::IsNullOrWhiteSpace($text)) {
        return [pscustomobject]@{ Status = 'blocked'; Reason = 'bundle-dump-empty' }
    }
    if ($text -match '(?i)permission denied|access denied|not permitted|cannot access') {
        return [pscustomobject]@{ Status = 'blocked'; Reason = 'bundle-dump-permission' }
    }
    if ($text -match '(?i)failed to get information|not exist|not found|no such file') {
        return [pscustomobject]@{ Status = 'blocked'; Reason = 'bundle-dump-absent' }
    }
    if ($text -notmatch [regex]::Escape($Bundle)) {
        return [pscustomobject]@{ Status = 'blocked'; Reason = 'bundle-dump-bundle-not-listed' }
    }
    return [pscustomobject]@{ Status = 'pass'; Reason = 'bundle-dump-present' }
}

function Test-StagingAbsent {
    param([Parameter(Mandatory)]$Result)
    # StagingProbe is fixed-path ls -ld. Only clear path-absence evidence counts as absent.
    # cannot access / permission denied are uncertain residual, not absence.
    $text = Get-HdcCombinedText $Result
    if ($text -match '(?i)permission denied|access denied|cannot access|not permitted') { return $false }
    if ($text -match '(?i)no such file|not found|path does not exist') { return $true }
    if ([int]$Result.ExitCode -eq 0 -and $text -match [regex]::Escape($script:Staging)) { return $false }
    return $false
}

function Confirm-BundleInstalled {
    param(
        [Parameter(Mandatory)][ValidateSet('InstallA', 'InstallB')][string]$Operation,
        [Parameter(Mandatory)][string]$Bundle,
        [Parameter(Mandatory)][string]$Label
    )
    $installResult = Invoke-HdcOperation $Operation -AllowFailure
    if ($installResult.ExitCode -in @(124, 125)) { throw "HDC infrastructure interruption during $Operation exit=$($installResult.ExitCode)" }
    $installAssessment = Get-HdcInstallAssessment $installResult
    if ($installAssessment.Status -eq 'functional_fail') {
        throw "FUNCTIONAL_FAIL scenario-1 FINAL HAP $Label install rejected"
    }
    if ($installAssessment.Status -ne 'pass') {
        throw "scenario-1 FINAL HAP $Label install outcome blocked: $($installAssessment.Reason)"
    }
    $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $Bundle } -AllowFailure
    if ($dumpResult.ExitCode -in @(124, 125)) { throw "HDC infrastructure interruption during BundleDump exit=$($dumpResult.ExitCode)" }
    $dumpAssessment = Get-BundleDumpAssessment $dumpResult $Bundle
    if ($dumpAssessment.Status -eq 'infrastructure') {
        throw "HDC infrastructure interruption during BundleDump: $($dumpAssessment.Reason)"
    }
    if ($dumpAssessment.Status -ne 'pass') {
        throw "scenario-1 FINAL HAP $Label install confirmation blocked: $($dumpAssessment.Reason)"
    }
}

function Invoke-RemoveStagingVerified {
    param([Parameter(Mandatory)][string]$ActionLabel)
    $removeResult = Invoke-HdcOperation 'RemoveStaging' -AllowFailure
    $probeResult = Invoke-HdcOperation 'StagingProbe' -AllowFailure
    $absent = Test-StagingAbsent $probeResult
    $script:CleanupActions.Add([ordered]@{
        operation = $ActionLabel
        remove_exit_code = $removeResult.ExitCode
        probe_exit_code = $probeResult.ExitCode
        staging_absent = [bool]$absent
        staging_sent_flag = [bool]$script:StagingSent
        staging_may_exist_flag = [bool]$script:StagingMayExist
    })
    if ($absent) {
        $script:StagingSent = $false
        $script:StagingMayExist = $false
        return $true
    }
    return $false
}

function Read-OperatorEnter {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][int]$StepIndex,
        [Parameter(Mandatory)][string]$StepId,
        [Parameter(Mandatory)][string]$ExpectedAction,
        [Parameter(Mandatory)]$MachinePrecondition,
        [AllowNull()]$CaptureBefore = $null
    )
    Write-OperatorWaitState 'waiting' -Scenario $Scenario -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -MachinePrecondition $MachinePrecondition
    Write-Host "现在只做：$ExpectedAction。完成后按回车。"
    $completedAt = $null
    $timedOut = $false
    if ($LiveSimulation) {
        $operatorFixture = Get-OptionalProperty $script:Simulation 'operator'
        $delay = [double](Get-OptionalProperty $operatorFixture 'action_delay_seconds' 1.0)
        $stepDelays = Get-OptionalProperty $operatorFixture 'step_delay_seconds' $null
        if ($null -ne $stepDelays) {
            $stepDelay = Get-OptionalProperty $stepDelays "$Scenario.$StepIndex" $null
            if ($null -ne $stepDelay) { $delay = [double]$stepDelay }
        }
        if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
        $script:VirtualSeconds += $delay
        $completedAt = Get-Now
    } else {
        $readTask = [Console]::In.ReadLineAsync()
        $deadline = (Get-Now).AddSeconds($OperatorTimeoutSeconds)
        while (-not $readTask.IsCompleted -and (Get-Now) -lt $deadline) {
            $remainingMilliseconds = [int][Math]::Max(1, [Math]::Min(250, ($deadline - (Get-Now)).TotalMilliseconds))
            [void]$readTask.Wait($remainingMilliseconds)
            if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
        }
        if (-not $readTask.IsCompleted) {
            $timedOut = $true
            $completedAt = Get-Now
        } else {
            [void]$readTask.GetAwaiter().GetResult()
            $completedAt = Get-Now
        }
    }
    $actionRecord = [ordered]@{
        scenario = $Scenario
        step_index = $StepIndex
        step_id = $StepId
        expected_action = $ExpectedAction
        completed_at = $completedAt.ToString('o')
        input = 'enter'
        timed_out = [bool]$timedOut
    }
    $script:OperatorActions.Add($actionRecord)
    Add-TranscriptRecord 'operator-mechanical-action' $actionRecord
    Write-OperatorWaitState 'operator-complete' -Scenario $Scenario -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -MachinePrecondition $MachinePrecondition -MachinePostcondition ([ordered]@{ status = 'pending-verification' })
    if ($timedOut) {
        Throw-ScenarioInvalid -Scenario $Scenario -Reason "step-$StepIndex operator-timeout" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -MachinePrecondition $MachinePrecondition
    }
    return $completedAt
}

function Parse-HilogDeviceTime {
    param([Parameter(Mandatory)][string]$Line)
    # Real HiLog -v year -v zone: "CST 2026-07-17 16:54:59.204 ..."
    # Also accepts already-offset stamps produced by simulation/host tools.
    if ($Line -match '^(?<zone>[A-Za-z]{2,5})\s+(?<stamp>\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?)\b') {
        $zoneToken = [string]$Matches.zone
        $offset = $script:FrozenDeviceZoneMap[$zoneToken]
        if ([string]::IsNullOrEmpty([string]$offset)) {
            return [pscustomobject]@{ Ok = $false; DeviceObservedAt = $null; DeviceTimeZone = $zoneToken; Reason = "unknown-device-time-zone:$zoneToken" }
        }
        $stampWithOffset = "$($Matches.stamp)$offset"
        $parsed = [DateTimeOffset]::MinValue
        if (-not [DateTimeOffset]::TryParse($stampWithOffset, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AllowWhiteSpaces, [ref]$parsed)) {
            return [pscustomobject]@{ Ok = $false; DeviceObservedAt = $null; DeviceTimeZone = $zoneToken; Reason = 'device-time-parse-failed' }
        }
        return [pscustomobject]@{ Ok = $true; DeviceObservedAt = $parsed.ToString('o'); DeviceTimeZone = $zoneToken; Reason = $null }
    }
    if ($Line -match '(?<stamp>\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2}))') {
        $parsed = [DateTimeOffset]::MinValue
        if ([DateTimeOffset]::TryParse($Matches.stamp, [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AllowWhiteSpaces, [ref]$parsed)) {
            $zoneToken = if ($Matches.stamp -match '(Z|[+-]\d{2}:?\d{2})$') { $Matches[1] } else { 'offset' }
            return [pscustomobject]@{ Ok = $true; DeviceObservedAt = $parsed.ToString('o'); DeviceTimeZone = $zoneToken; Reason = $null }
        }
        return [pscustomobject]@{ Ok = $false; DeviceObservedAt = $null; DeviceTimeZone = $null; Reason = 'device-time-parse-failed' }
    }
    return [pscustomobject]@{ Ok = $false; DeviceObservedAt = $null; DeviceTimeZone = $null; Reason = 'device-time-missing' }
}

function Add-CaptureDegradation {
    param(
        [AllowNull()]$Capture,
        [Parameter(Mandatory)][string]$Component,
        [Parameter(Mandatory)][string]$Reason,
        [int]$Scenario = -1,
        [ValidateSet('infrastructure', 'non-infrastructure')][string]$Category = 'non-infrastructure',
        [AllowNull()][string]$InfrastructureReason = $null,
        [bool]$MarkContinuousDegraded = $true
    )
    $scenarioNumber = if ($Scenario -ge 0) { $Scenario } elseif ($null -ne $Capture) { [int]$Capture.ActiveScenario } else { 0 }
    $safeReason = Protect-SensitiveText $Reason
    if ($Category -eq 'infrastructure' -and [string]::IsNullOrEmpty($InfrastructureReason)) {
        $InfrastructureReason = 'hdc-usb-interruption'
    }
    if ($Category -eq 'infrastructure' -and -not [string]::IsNullOrEmpty($InfrastructureReason)) {
        $script:InfrastructureReasonObserved = $InfrastructureReason
    }
    if ($MarkContinuousDegraded -and $null -ne $Capture) {
        $Capture.Degraded = $true
    }
    $duplicate = @($script:CaptureDegraded | Where-Object {
        [int]$_.scenario -eq $scenarioNumber -and [string]$_.component -eq $Component -and [string]$_.reason -eq $safeReason
    }).Count -gt 0
    if ($duplicate) { return }
    $degraded = [ordered]@{
        scenario = $scenarioNumber
        component = $Component
        reason = $safeReason
        category = $Category
        infrastructure_reason = $InfrastructureReason
    }
    $script:CaptureDegraded.Add($degraded)
    Add-TranscriptRecord 'capture-degraded' $degraded
}

function New-CampaignCaptureState {
    param([Parameter(Mandatory)][string]$StdoutPath, [Parameter(Mandatory)][string]$StderrPath, [AllowNull()]$Process)
    return [pscustomobject]@{
        StartedAt = Get-Now
        Process = $Process
        StdoutPath = $StdoutPath
        StderrPath = $StderrPath
        ReadOffset = [long]0
        PendingBytes = [byte[]]@()
        PendingStartByte = [long]0
        CompleteByteOffset = [long]0
        LineCount = 0
        Events = [Collections.Generic.List[object]]::new()
        Degraded = $false
        Stopped = $false
        LastHealthyAt = $null
        LastStderrBytes = [long]0
        ActiveScenario = 0
        InitialAnchor = $null
        SimulatedDead = $false
    }
}

function Test-CampaignCaptureHealth {
    param([Parameter(Mandatory)]$Capture)
    if ($Capture.Stopped) { return $false }
    if ($Capture.SimulatedDead) {
        Add-CaptureDegradation $Capture 'raw-hilog-process' 'simulated capture process exited during the scenario window' -Category infrastructure -InfrastructureReason 'hdc-usb-interruption'
    }
    if ($null -ne $Capture.Process) {
        try {
            if ($Capture.Process.HasExited) {
                Add-CaptureDegradation $Capture 'raw-hilog-process' "capture process exited unexpectedly with code $($Capture.Process.ExitCode)" -Category infrastructure -InfrastructureReason 'hdc-usb-interruption'
            }
        } catch {
            Add-CaptureDegradation $Capture 'raw-hilog-health' $_.Exception.Message -Category infrastructure -InfrastructureReason 'hdc-usb-interruption'
        }
    }
    if (Test-Path -LiteralPath $Capture.StderrPath -PathType Leaf) {
        try {
            $stderrBytes = [long](Get-Item -LiteralPath $Capture.StderrPath).Length
            if ($stderrBytes -gt $Capture.LastStderrBytes) {
                Add-CaptureDegradation $Capture 'raw-hilog-stderr' "capture stderr grew by $($stderrBytes - $Capture.LastStderrBytes) bytes" -Category infrastructure -InfrastructureReason 'hdc-usb-interruption'
            }
            $Capture.LastStderrBytes = $stderrBytes
        } catch {
            # Local stderr-size read is a host I/O concern, not USB interruption.
            Add-CaptureDegradation $Capture 'raw-hilog-stderr-read' $_.Exception.Message -Category 'non-infrastructure' -MarkContinuousDegraded $true
        }
    }
    if (-not $Capture.Degraded) {
        $Capture.LastHealthyAt = Get-Now
        return $true
    }
    return $false
}

function Update-CampaignCapture {
    param([Parameter(Mandatory)]$Capture)
    [void](Test-CampaignCaptureHealth $Capture)
    if (-not (Test-Path -LiteralPath $Capture.StdoutPath -PathType Leaf)) {
        Add-CaptureDegradation $Capture 'raw-hilog-read' 'capture stdout file is missing' -Category 'non-infrastructure'
        return
    }
    try {
        $stream = [IO.FileStream]::new($Capture.StdoutPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
        try {
            $length = [long]$stream.Length
            if ($length -lt [long]$Capture.ReadOffset) {
                Add-CaptureDegradation $Capture 'raw-hilog-dropped-line' 'capture stdout was truncated behind the incremental byte cursor' -Category 'non-infrastructure'
                return
            }
            $available = $length - [long]$Capture.ReadOffset
            if ($available -gt [int]::MaxValue) { throw 'capture increment exceeds the supported in-memory read size' }
            [byte[]]$newBytes = [byte[]]::new([int]$available)
            [void]$stream.Seek([long]$Capture.ReadOffset, [IO.SeekOrigin]::Begin)
            $readTotal = 0
            while ($readTotal -lt $newBytes.Length) {
                $readNow = $stream.Read($newBytes, $readTotal, $newBytes.Length - $readTotal)
                if ($readNow -le 0) { break }
                $readTotal += $readNow
            }
            if ($readTotal -ne $newBytes.Length) {
                Add-CaptureDegradation $Capture 'raw-hilog-dropped-line' "incremental read returned $readTotal of $($newBytes.Length) bytes" -Category 'non-infrastructure'
                if ($readTotal -eq 0) { return }
                $newBytes = [byte[]]$newBytes[0..($readTotal - 1)]
            }
            $newReadOffset = [long]$Capture.ReadOffset + $readTotal
            $baseOffset = if ($Capture.PendingBytes.Length -gt 0) { [long]$Capture.PendingStartByte } else { [long]$Capture.ReadOffset }
            [byte[]]$combined = [byte[]]($Capture.PendingBytes + $newBytes)
            $segmentStart = 0
            $strictUtf8 = [Text.UTF8Encoding]::new($false, $true)
            for ($index = 0; $index -lt $combined.Length; $index++) {
                if ($combined[$index] -ne 10) { continue }
                $segmentLength = $index - $segmentStart
                if ($segmentLength -gt 0 -and $combined[$index - 1] -eq 13) { $segmentLength-- }
                [byte[]]$lineBytes = if ($segmentLength -gt 0) { [byte[]]$combined[$segmentStart..($segmentStart + $segmentLength - 1)] } else { [byte[]]@() }
                try { $line = $strictUtf8.GetString($lineBytes) } catch {
                    Add-CaptureDegradation $Capture 'raw-hilog-dropped-line' "invalid UTF-8 at byte $($baseOffset + $segmentStart)" -Category 'non-infrastructure'
                    $line = ''
                }
                $observedAt = Get-Now
                $parsedTime = Parse-HilogDeviceTime $line
                if (-not [bool]$parsedTime.Ok) {
                    Add-CaptureDegradation $Capture 'raw-hilog-time-parse' "line $($Capture.LineCount + 1) $($parsedTime.Reason)" -Category 'non-infrastructure'
                }
                $Capture.LineCount++
                $Capture.CompleteByteOffset = $baseOffset + $index + 1
                $Capture.Events.Add([pscustomobject]@{
                    line_index = [int]$Capture.LineCount
                    raw_byte_start = [long]($baseOffset + $segmentStart)
                    raw_byte_end = [long]$Capture.CompleteByteOffset
                    text = $line
                    device_observed_at = $parsedTime.DeviceObservedAt
                    device_time_zone = $parsedTime.DeviceTimeZone
                    host_observed_at = $observedAt.ToString('o')
                })
                $segmentStart = $index + 1
            }
            if ($segmentStart -lt $combined.Length) {
                $Capture.PendingBytes = [byte[]]$combined[$segmentStart..($combined.Length - 1)]
                $Capture.PendingStartByte = $baseOffset + $segmentStart
            } else {
                $Capture.PendingBytes = [byte[]]@()
                $Capture.PendingStartByte = $newReadOffset
            }
            $Capture.ReadOffset = $newReadOffset
        } finally {
            $stream.Dispose()
        }
    } catch {
        Add-CaptureDegradation $Capture 'raw-hilog-read' $_.Exception.Message -Category 'non-infrastructure'
    }
    [void](Test-CampaignCaptureHealth $Capture)
}

function Start-CampaignHilogCapture {
    $stdoutPath = Join-Path $script:RawPath 'raw-hilog-campaign.log'
    $stderrPath = Join-Path $script:RawPath 'raw-hilog-campaign.stderr.log'
    $auditArguments = Get-HdcInvocation 'HilogStream'
    $script:HdcLogicalCallCount++
    Add-TranscriptRecord 'hdc-capture-start' ([ordered]@{ scope = 'continuous-campaign'; operation = 'HilogStream'; arguments = $auditArguments; independent_raw_reference = 'RAW-HILOG-CAMPAIGN' })
    $captureProcess = $null
    if ($LiveSimulation) {
        $initialLines = @(Get-OptionalProperty $script:Simulation 'capture_initial_lines' @())
        $initialText = if ($initialLines.Count -gt 0) { (@($initialLines | ForEach-Object { [string]$_ }) -join [Environment]::NewLine) + [Environment]::NewLine } else { '' }
        [IO.File]::WriteAllText($stdoutPath, $initialText, [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText($stderrPath, '', [Text.UTF8Encoding]::new($false))
    } else {
        $liveArguments = Get-LiveHdcArguments $auditArguments 'HilogStream' @{}
        try {
            $captureProcess = Start-Process -FilePath $HdcPath -ArgumentList $liveArguments -NoNewWindow -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
            if ($null -eq $captureProcess) { throw 'Start-Process returned no process' }
            $script:HdcProcessStartCount++
        } catch {
            [IO.File]::WriteAllText($stdoutPath, '', [Text.UTF8Encoding]::new($false))
            [IO.File]::WriteAllText($stderrPath, '', [Text.UTF8Encoding]::new($false))
        }
    }
    $capture = New-CampaignCaptureState $stdoutPath $stderrPath $captureProcess
    if (-not $LiveSimulation -and $null -eq $captureProcess) {
        Add-CaptureDegradation $capture 'raw-hilog-start' 'unable to start continuous campaign capture' -Category infrastructure -InfrastructureReason 'hdc-usb-interruption'
    }
    $script:CampaignCapture = $capture
    return $capture
}

function Initialize-CampaignCaptureAnchor {
    param([Parameter(Mandatory)]$Capture)
    if ($LiveSimulation) {
        Update-CampaignCapture $Capture
    } else {
        $stablePolls = 0
        $lastOffset = -1L
        while ($stablePolls -lt 2) {
            Start-Sleep -Milliseconds 250
            Update-CampaignCapture $Capture
            if ($Capture.ReadOffset -eq $lastOffset) { $stablePolls++ } else { $stablePolls = 0; $lastOffset = $Capture.ReadOffset }
        }
    }
    $Capture.InitialAnchor = [pscustomobject]@{
        recorded_at = (Get-Now).ToString('o')
        complete_line_count = [int]$Capture.LineCount
        complete_byte_offset = [long]$Capture.CompleteByteOffset
        stream_byte_offset = [long]$Capture.ReadOffset
        partial_remainder_bytes = [int]$Capture.PendingBytes.Length
        event_count = [int]$Capture.Events.Count
    }
    Add-TranscriptRecord 'hilog-initial-anchor' $Capture.InitialAnchor
}

function Get-SimulationEventStepIndex {
    param([Parameter(Mandatory)][int]$Scenario, [Parameter(Mandatory)]$Item)
    $explicit = Get-OptionalProperty $Item 'step_index' $null
    if ($null -ne $explicit) { return [int]$explicit }
    $text = [string](Get-OptionalProperty $Item 'text' '')
    switch ($Scenario) {
        2 { if ($text -match 'UI_START\|') { return 1 }; return 2 }
        4 { if ($text -match 'UI_START\|') { return 1 }; return 2 }
        # ADJ-20260808-0003: S5 no longer asks the operator to open Settings>VPN; the only
        # mechanical steps are app-info (3) and force-stop (4). Destroy-side events belong to 4.
        5 { if ($text -match 'VPN_DESTROY_|VPN_ONDESTROY') { return 4 }; return 1 }
        # ADJ-20260808-0003: S6 mechanical steps are A Start (1), optional Allow reauth (2), B
        # Start (3). B-side events belong to the B Start step (3); A-side events to the A Start
        # step (1). Fixtures may still pin an explicit step_index per event.
        6 { if ($text -match "bundle=$([regex]::Escape($script:BundleB))|requestId=b") { return 3 }; return 1 }
        default { return 1 }
    }
}

function Test-SimulationStepHasEffect {
    param([Parameter(Mandatory)][int]$Scenario, [Parameter(Mandatory)][int]$StepIndex)
    if (-not $LiveSimulation) { return $true }
    $noEffect = @((Get-OptionalProperty (Get-OptionalProperty $script:Simulation 'operator') 'no_effect_steps' @()) | ForEach-Object { [string]$_ })
    return "$Scenario.$StepIndex" -notin $noEffect
}

function Add-SimulationScenarioStepOutput {
    param(
        [Parameter(Mandatory)]$Capture,
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][int]$StepIndex,
        [Parameter(Mandatory)][DateTimeOffset]$ActionPromptAt,
        [Parameter(Mandatory)][DateTimeOffset]$ActionCompletedAt
    )
    if (-not $LiveSimulation) { return }
    $key = "$Scenario.$StepIndex"
    if ($script:SimulationScenarioStepsWritten.ContainsKey($key)) { return }
    $script:SimulationScenarioStepsWritten[$key] = $true
    $scenarioEvents = Get-OptionalProperty $script:Simulation 'scenario_events'
    $items = @(Get-OptionalProperty $scenarioEvents ([string]$Scenario) @() | Where-Object { (Get-SimulationEventStepIndex $Scenario $_) -eq $StepIndex }) | Sort-Object { [double](Get-OptionalProperty $_ 'offset_seconds' 0.0) }
    $minimumOffset = if (@($items).Count -gt 0) { [double](Get-OptionalProperty @($items)[0] 'offset_seconds' 0.0) } else { 0.0 }
    foreach ($item in $items) {
        $offset = [double](Get-OptionalProperty $item 'offset_seconds' 0.0)
        # ADJ-20260808-0003: pre-enter events (relative_to_prompt=true) are stamped against the
        # prompt time, so a slow operator (action_delay_seconds >= 8) cannot push events past the
        # enter; the device timestamp lands shortly after the prompt and well before the enter.
        # All other events keep their completed-at base (post-enter) semantics.
        $relativeToPrompt = Get-OptionalJsonBoolean $item 'relative_to_prompt' $false
        $base = if ($relativeToPrompt) { $ActionPromptAt } else { $ActionCompletedAt }
        $relativeOffset = if ($relativeToPrompt) { 0.2 + [Math]::Max(0.0, $offset) } else { 0.2 + [Math]::Max(0.0, $offset - $minimumOffset) }
        $deviceStamp = $base.AddSeconds($relativeOffset).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        $text = ([string](Get-OptionalProperty $item 'text' '')).Replace('<DEVICE_OBSERVED_AT>', $deviceStamp, [StringComparison]::Ordinal)
        $withoutNewline = Get-OptionalJsonBoolean $item 'append_without_newline' $false
        [IO.File]::AppendAllText($Capture.StdoutPath, $text + $(if ($withoutNewline) { '' } else { [Environment]::NewLine }), [Text.UTF8Encoding]::new($false))
    }
    $dieScenario = [int](Get-OptionalProperty $script:Simulation 'capture_die_scenario' 0)
    if ($dieScenario -eq $Scenario -and $StepIndex -eq 1) { $Capture.SimulatedDead = $true }
}

function Get-ScenarioWindowEvents {
    param(
        [Parameter(Mandatory)]$Capture,
        [Parameter(Mandatory)][long]$AnchorByte,
        [Parameter(Mandatory)][DateTimeOffset]$ActionPromptAt,
        [Parameter(Mandatory)][DateTimeOffset]$ObservedThrough
    )
    $earliestDeviceTime = $ActionPromptAt.AddSeconds(-1.0 * [double]$script:DeviceClockSkewToleranceSeconds)
    return @($Capture.Events | Where-Object {
        # Byte-anchor excludes historical buffer; device-time bounds the action window with frozen skew tolerance only.
        if ([long]$_.raw_byte_start -lt $AnchorByte -or [string]::IsNullOrWhiteSpace([string]$_.device_observed_at)) { return $false }
        $deviceTime = [DateTimeOffset]::MinValue
        if (-not [DateTimeOffset]::TryParse([string]$_.device_observed_at, [ref]$deviceTime)) { return $false }
        return $deviceTime -ge $earliestDeviceTime -and $deviceTime -le $ObservedThrough
    } | ForEach-Object {
        [pscustomobject]@{
            line_index = $_.line_index
            raw_byte_start = $_.raw_byte_start
            raw_byte_end = $_.raw_byte_end
            offset_seconds = ([DateTimeOffset]::Parse([string]$_.device_observed_at) - $ActionPromptAt).TotalSeconds
            text = $_.text
            device_observed_at = $_.device_observed_at
            device_time_zone = $_.device_time_zone
            host_observed_at = $_.host_observed_at
        }
    })
}

function Stop-CampaignHilogCapture {
    param([Parameter(Mandatory)]$Capture)
    if ($Capture.Stopped) { return }
    for ($poll = 0; $poll -lt 3; $poll++) {
        if (-not $LiveSimulation) { Start-Sleep -Milliseconds 200 }
        Update-CampaignCapture $Capture
    }
    $Capture.Stopped = $true
    if ($null -ne $Capture.Process) {
        try {
            if (-not $Capture.Process.HasExited) { $Capture.Process.Kill($true) }
            if (-not $Capture.Process.WaitForExit(5000)) { throw 'hilog capture process did not exit within 5 seconds' }
        } catch {
            Add-CaptureDegradation $Capture 'raw-hilog-stop' $_.Exception.Message -Category 'non-infrastructure'
        }
    }
    Update-CampaignCapture $Capture
    if ($Capture.PendingBytes.Length -gt 0) {
        Add-CaptureDegradation $Capture 'raw-hilog-incomplete-final-line' "capture ended with $($Capture.PendingBytes.Length) uncompleted bytes"
    }
    if ($null -ne $Capture.Process) { $Capture.Process.Dispose() }
    foreach ($path in @($Capture.StdoutPath, $Capture.StderrPath)) {
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $reference = if ($path -eq $Capture.StdoutPath) { 'RAW-HILOG-CAMPAIGN' } else { 'RAW-HILOG-CAMPAIGN-STDERR' }
            $script:RawHilogArtifacts.Add([ordered]@{ scenario = 0; reference = $reference; path = $path; sha256 = Get-FileSha256 $path; bytes = (Get-Item -LiteralPath $path).Length })
        }
    }
}

function New-ScenarioContext {
    param([Parameter(Mandatory)][int]$Scenario)
    $capture = $script:CampaignCapture
    if ($null -eq $capture) { throw "scenario-$Scenario continuous capture is not initialized" }
    Update-CampaignCapture $capture
    # Cross-scenario operator action guard: any UI action event that arrived after the previous
    # scenario's window (and was not consumed by any mechanical step) invalidates the scenario
    # before any prompt of this one. Auto StartEntry ENTRY events are never UI actions.
    Assert-NoStrayOperatorActions ([pscustomobject]@{ Scenario = $Scenario; Capture = $capture; StartedAt = (Get-Now) })
    $capture.ActiveScenario = $Scenario
    $script:CampaignPhase = "scenario-$Scenario"
    [void](Test-CampaignCaptureHealth $capture)
    return [pscustomobject]@{
        Scenario = $Scenario
        Capture = $capture
        AnchorByte = [long]$capture.ReadOffset
        StartedAt = Get-Now
        FirstActionAt = $null
        LastActionCompletedAt = $null
        Actions = [Collections.Generic.List[object]]::new()
    }
}

function Get-ScenarioContextEvents {
    param([Parameter(Mandatory)]$Context, [AllowNull()]$ObservedThrough = $null, [AllowNull()]$AnchorByte = $null, [AllowNull()]$ActionAt = $null)
    Update-CampaignCapture $Context.Capture
    if ($null -eq $ObservedThrough) { $ObservedThrough = Get-Now }
    if ($null -eq $AnchorByte) { $AnchorByte = [long]$Context.AnchorByte }
    if ($null -eq $ActionAt) { $ActionAt = [DateTimeOffset]$Context.StartedAt }
    return @(Get-ScenarioWindowEvents $Context.Capture $AnchorByte $ActionAt $ObservedThrough)
}

function Get-CaptureDegradedInfra {
    param([Parameter(Mandatory)][int]$Scenario)
    # ADJ-20260808-0003 (C6): classify a continuous raw-hilog degradation affecting this scenario
    # (or the global scenario 0) as infrastructure (capture process exit / stderr growth / HDC
    # transport) vs non-infrastructure (host read/parse/storage). Returns $null when no
    # continuous raw-hilog degradation applies, $true for infrastructure, $false otherwise.
    $entries = @($script:CaptureDegraded | Where-Object {
        [int]$_.scenario -in @($Scenario, 0) -and [string]$_.component -match 'raw-hilog'
    })
    if ($entries.Count -eq 0) { return $null }
    $infra = @($entries | Where-Object {
        [string]$_.category -eq 'infrastructure' -or [string]$_.infrastructure_reason -eq 'hdc-usb-interruption'
    })
    return ($infra.Count -gt 0)
}

function Wait-MachineCondition {
    param(
        [Parameter(Mandatory)]$Context,
        [Parameter(Mandatory)][long]$AnchorByte,
        [Parameter(Mandatory)][DateTimeOffset]$ActionAt,
        [Parameter(Mandatory)][scriptblock]$Condition,
        [double]$TimeoutSeconds = 12.0
    )
    $deadline = (Get-Now).AddSeconds($TimeoutSeconds)
    $last = [pscustomobject]@{ status = 'pending'; reason = 'event-postcondition-pending' }
    $iterations = 0
    while ((Get-Now) -lt $deadline) {
        $iterations++
        if ($iterations -gt 5000) { break }
        $events = @(Get-ScenarioContextEvents $Context (Get-Now) $AnchorByte $ActionAt)
        $last = & $Condition $events
        if ($null -ne $last -and [string]$last.status -ne 'pending') { return $last }
        if ((Get-Now) -ge $deadline -or $Context.Capture.Degraded) {
            # ADJ-20260808-0002 (C6): continuous capture degradation is never a status invalid.
            # The unified classifier throws the classified blocked (infra authorizes the USB
            # retry; non-infra stays a plain blocked), so the caller can never turn it into
            # ScenarioInvalid.
            if ($Context.Capture.Degraded) {
                Assert-CampaignCaptureHealthy $Context.Capture ([int]$Context.Scenario) -Origin 'Wait-MachineCondition'
            }
            break
        }
        if ($LiveSimulation) {
            $now = Get-Now
            $nextAt = $deadline
            foreach ($event in @($Context.Capture.Events)) {
                if ([long]$event.raw_byte_start -lt $AnchorByte -or [string]::IsNullOrWhiteSpace([string]$event.device_observed_at)) { continue }
                $eventAt = [DateTimeOffset]::MinValue
                if ([DateTimeOffset]::TryParse([string]$event.device_observed_at, [ref]$eventAt) -and $eventAt -gt $now -and $eventAt -lt $nextAt) { $nextAt = $eventAt }
            }
            if ($nextAt -le $now) { $nextAt = $now.AddMilliseconds(1) }
            Wait-Until $nextAt
        } else {
            Start-Sleep -Milliseconds 250
        }
    }
    $pendingReason = [string](Get-OptionalProperty $last 'reason' 'event-postcondition-missing')
    # ADJ-20260808-0002 (C6): a pending-postcondition timeout is classified by whether the
    # mechanical UI action itself appeared. If the expected UI_START / UI_STOP never arrived,
    # the operator action did not register: invalid `mechanical-action-missing`. If the UI action
    # appeared but the platform's subsequent marker (create/destroy terminal) did not arrive, the
    # outcome is a platform/runner uncertainty: blocked, never invalid.
    if ($pendingReason -in @('UI_START-missing', 'UI_STOP-missing')) {
        return [pscustomobject]@{ status = 'invalid'; reason = 'mechanical-action-missing:' + $pendingReason }
    }
    return [pscustomobject]@{ status = 'blocked'; reason = 'platform-marker-missing:' + $pendingReason }
}

function Get-E3EventInfo {
    param([Parameter(Mandatory)]$Event)
    $text = [string]$Event.text
    $marker = $null
    if ($text -match '(?:^|\s)([A-Z][A-Z0-9_]+)\|') { $marker = [string]$Matches[1] }
    $bundle = $null
    if ($text -match '(?:^|\|)bundle=([^|\s]+)') { $bundle = [string]$Matches[1] }
    $requestId = $null
    if ($text -match '(?:^|\|)requestId=([^|\s]+)') { $requestId = [string]$Matches[1] }
    return [pscustomobject]@{ Marker = $marker; Bundle = $bundle; RequestId = $requestId; Text = $text; Event = $Event }
}

function Get-RejectionErrorCode {
    param([Parameter(Mandatory)][string]$Text)
    # ADJ-20260808-0002 (C6): extract a numeric BusinessError code from a rejection event text,
    # supporting the real Extension safeError comma-field shape (`summary=code=2203002,name=...,
    # message=...`) and the historical top-level `|code=123|` field. Boundary-rigorous: a code
    # must be a standalone `code=` name=value pair whose numeric value ends at a field boundary
    # (`|`, `,`, `;`, whitespace, or EOL) so prose quoted inside `message=...` is never mis-parsed,
    # and a standalone `code=` key must be preceded by start / `|` / `,` / `;` / `=`. Returns $null
    # when no such code is present.
    # 1) Historical top-level field: |code=123| or code=123 at EOL/whitespace.
    if ($Text -match '(?:^|\|)code=(\d+)(?=\||\s|$)') { return [int]$Matches[1] }
    # 2) safeError comma-field payload: summary=code=2203002,name=...,message=... and any
    #    comma/semicolon-separated name=value list (including the first field right after `=`).
    if ($Text -match '(?:^|[|,;=])\s*code=(\d+)\s*(?=,|;|$|\s)') { return [int]$Matches[1] }
    return $null
}

function Test-UniqueStartCondition {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events, [Parameter(Mandatory)][string]$Bundle)
    $infos = @($Events | ForEach-Object { Get-E3EventInfo $_ })
    $forbidden = @($infos | Where-Object { $_.Marker -in @('UI_STOP', 'UI_STOP_SKIPPED') })
    if ($forbidden.Count -gt 0) { return [pscustomobject]@{ status = 'invalid'; reason = "unexpected-$($forbidden[0].Marker)-during-start-step" } }
    $skipped = @($infos | Where-Object { $_.Marker -eq 'UI_START_SKIPPED' })
    if ($skipped.Count -gt 0) { return [pscustomobject]@{ status = 'invalid'; reason = 'UI_START_SKIPPED' } }
    $starts = @($infos | Where-Object { $_.Marker -eq 'UI_START' })
    if ($starts.Count -eq 0) { return [pscustomobject]@{ status = 'pending'; reason = 'UI_START-missing' } }
    if ($starts.Count -ne 1) { return [pscustomobject]@{ status = 'invalid'; reason = "expected-one-UI_START-observed-$($starts.Count)" } }
    if ([string]$starts[0].Bundle -ne $Bundle) { return [pscustomobject]@{ status = 'invalid'; reason = "UI_START-wrong-bundle:$($starts[0].Bundle)" } }
    if ([string]::IsNullOrWhiteSpace([string]$starts[0].RequestId) -or [string]$starts[0].RequestId -eq 'missing') { return [pscustomobject]@{ status = 'invalid'; reason = 'UI_START-requestId-missing' } }
    return [pscustomobject]@{ status = 'pass'; reason = 'unique-UI_START'; request_id = [string]$starts[0].RequestId; bundle = $Bundle }
}

function Test-UniqueStopCondition {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events, [Parameter(Mandatory)][string]$Bundle, [Parameter(Mandatory)][string]$RequestId)
    $infos = @($Events | ForEach-Object { Get-E3EventInfo $_ })
    $starts = @($infos | Where-Object { $_.Marker -in @('UI_START', 'UI_START_SKIPPED') })
    if ($starts.Count -gt 0) { return [pscustomobject]@{ status = 'invalid'; reason = "unexpected-$($starts[0].Marker)-during-stop-step" } }
    $skipped = @($infos | Where-Object { $_.Marker -eq 'UI_STOP_SKIPPED' })
    if ($skipped.Count -gt 0) { return [pscustomobject]@{ status = 'invalid'; reason = 'UI_STOP_SKIPPED' } }
    $stops = @($infos | Where-Object { $_.Marker -eq 'UI_STOP' })
    if ($stops.Count -eq 0) { return [pscustomobject]@{ status = 'pending'; reason = 'UI_STOP-missing' } }
    if ($stops.Count -ne 1) { return [pscustomobject]@{ status = 'invalid'; reason = "expected-one-UI_STOP-observed-$($stops.Count)" } }
    if ([string]$stops[0].Bundle -ne $Bundle) { return [pscustomobject]@{ status = 'invalid'; reason = "UI_STOP-wrong-bundle:$($stops[0].Bundle)" } }
    if ([string]$stops[0].RequestId -ne $RequestId) { return [pscustomobject]@{ status = 'invalid'; reason = "UI_STOP-wrong-requestId:$($stops[0].RequestId)" } }
    return [pscustomobject]@{ status = 'pass'; reason = 'unique-UI_STOP'; request_id = $RequestId; bundle = $Bundle }
}

function Test-NoOperatorAction {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events)
    # ADJ-20260808-0003: Allow/Deny/Settings navigation steps allow zero extra UI actions:
    # any UI_START / UI_STOP / UI_STOP_SKIPPED in the step window is invalid. Auto StartEntry
    # ENTRY events are not UI actions and stay allowed.
    $infos = @($Events | ForEach-Object { Get-E3EventInfo $_ })
    $actions = @($infos | Where-Object { $_.Marker -in @('UI_START', 'UI_STOP', 'UI_STOP_SKIPPED') })
    if ($actions.Count -gt 0) {
        return [pscustomobject]@{ status = 'invalid'; reason = "unexpected-$($actions[0].Marker):bundle=$($actions[0].Bundle):requestId=$($actions[0].RequestId)" }
    }
    return [pscustomobject]@{ status = 'pass'; reason = 'no-extra-ui-action' }
}

function Flush-SimulationGapActions {
    param([Parameter(Mandatory)][int]$Scenario, [Parameter(Mandatory)][int]$StepIndex)
    # ADJ-20260808-0003: simulation-only injection of a stray operator UI action that arrives in
    # the gap between verified checkpoints (after the previous step/scenario, before the next
    # prompt). The line is appended to the capture now (host time = now), so the operator action
    # guard scanning by host_observed_at sees it as an unowned UI action and invalidates.
    if (-not $LiveSimulation) { return }
    $gapActions = @(Get-OptionalProperty $script:Simulation 'gap_actions' @())
    $newList = [Collections.Generic.List[object]]::new()
    $flushed = $false
    foreach ($gap in $gapActions) {
        if ([int](Get-OptionalProperty $gap 'scenario' -1) -ne $Scenario) { $newList.Add($gap); continue }
        $afterStep = [int](Get-OptionalProperty $gap 'after_step_index' 0)
        if ($StepIndex -le $afterStep) { $newList.Add($gap); continue }
        $text = [string](Get-OptionalProperty $gap 'text' '')
        $delay = [double](Get-OptionalProperty $gap 'delay_seconds' 0.2)
        # ADJ-20260808-0003: advance the virtual clock so the appended line is observed strictly
        # after the previous guard checkpoint (host time now > guard-from), otherwise the guard
        # would treat the injected gap action as already-owned and skip it.
        $script:VirtualSeconds += $delay
        $stamp = (Get-Now).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        $line = $text.Replace('<DEVICE_OBSERVED_AT>', $stamp, [StringComparison]::Ordinal)
        [IO.File]::AppendAllText($script:CampaignCapture.StdoutPath, $line + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $flushed = $true
    }
    if ($flushed) { $script:Simulation.gap_actions = @($newList) }
    if ($flushed -and $null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
}

function Assert-NoStrayOperatorActions {
    param(
        [Parameter(Mandatory)]$Context,
        [AllowNull()][int]$StepIndex = $null,
        [AllowNull()][string]$StepId = $null,
        [AllowNull()][string]$ExpectedAction = $null
    )
    # ADJ-20260808-0003 global operator action guard: any UI_START / UI_STOP / UI_STOP_SKIPPED
    # (wrong bundle/request/order included) observed after the last verified checkpoint but not
    # owned by the current mechanical step invalidates the scenario immediately, before the next
    # prompt / scenario. Ownership is decided by host observation time: events consumed by a
    # step's verification loop were already read before the guard advanced, so only events that
    # arrived during a gap are scanned. Auto StartEntry ENTRY events are never UI actions.
    if ($null -ne $Context -and $null -ne $Context.PSObject.Properties['Capture']) {
        Flush-SimulationGapActions ([int]$Context.Scenario) $([int]$StepIndex)
    }
    Update-CampaignCapture $Context.Capture
    $from = if ($null -ne $script:OperatorActionGuardFrom) { [DateTimeOffset]$script:OperatorActionGuardFrom } else { [DateTimeOffset]$Context.StartedAt }
    foreach ($event in @($Context.Capture.Events)) {
        $hostAt = [DateTimeOffset]::MinValue
        if (-not [DateTimeOffset]::TryParse([string]$event.host_observed_at, [ref]$hostAt)) { continue }
        if ($hostAt -le $from) { continue }
        $info = Get-E3EventInfo $event
        if ($info.Marker -in @('UI_START', 'UI_STOP', 'UI_STOP_SKIPPED')) {
            Throw-ScenarioInvalid -Scenario ([int]$Context.Scenario) -Reason "stray-operator-action:$($info.Marker):bundle=$($info.Bundle):requestId=$($info.RequestId)" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction
        }
    }
    $script:OperatorActionGuardFrom = Get-Now
}

function Register-VerifiedRequest {
    param([Parameter(Mandatory)][string]$RequestId, [Parameter(Mandatory)][string]$Bundle, [Parameter(Mandatory)][int]$Scenario)
    # ADJ-20260808-0003: VerifiedRequests is a live global uniqueness / ownership register for
    # UI_START request ids. A repeated requestId (same or different bundle) across scenarios
    # makes attribution ambiguous and is invalid immediately.
    if ([string]::IsNullOrWhiteSpace($RequestId) -or $RequestId -eq 'missing') {
        Throw-ScenarioInvalid -Scenario $Scenario -Reason 'requestId-missing-cannot-register'
    }
    if ($script:VerifiedRequests.ContainsKey($RequestId)) {
        $previousBundle = [string]$script:VerifiedRequests[$RequestId]
        $reason = if ($previousBundle -eq $Bundle) { "requestId-reused:$RequestId" } else { "requestId-reused-with-different-bundle:$RequestId" }
        Throw-ScenarioInvalid -Scenario $Scenario -Reason $reason
    }
    $script:VerifiedRequests[$RequestId] = $Bundle
}

function Invoke-MechanicalStep {
    param(
        [Parameter(Mandatory)]$Context,
        [Parameter(Mandatory)][int]$StepIndex,
        [Parameter(Mandatory)][string]$ExpectedAction,
        [Parameter(Mandatory)]$MachinePrecondition,
        [Parameter(Mandatory)][scriptblock]$Postcondition,
        [AllowNull()]$CaptureBefore = $null,
        [AllowNull()][string]$CaptureAfterName = $null,
        [AllowNull()][string]$CaptureAfterProfile = $null,
        [AllowNull()][string]$CaptureAfterExpectedBundle = $null,
        [switch]$CaptureAfterReviewOnly,
        [switch]$CaptureAfterObservationOnly,
        [double]$VerifyTimeoutSeconds = 12.0
    )
    $scenario = [int]$Context.Scenario
    $stepId = [guid]::NewGuid().ToString('N').Substring(0, 12)
    # Global operator action guard runs before the prompt: any UI action observed in the gap
    # since the last verified checkpoint invalidates the scenario before the next step is asked.
    Assert-NoStrayOperatorActions $Context -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction
    # ADJ-20260808-0003: only an explicit machine-precondition status=invalid is operator/protocol
    # invalid. status=blocked (process mismatch, HDC infra, unknown probe) is a plain runner blocked
    # so process residue / transport loss is never misclassified as operator invalid.
    $preStatus = [string](Get-OptionalProperty $MachinePrecondition 'status' '')
    if ($preStatus -ne 'pass') {
        $preReason = [string](Get-OptionalProperty $MachinePrecondition 'reason' 'unknown')
        if ($preStatus -eq 'invalid') {
            Throw-ScenarioInvalid -Scenario $scenario -Reason "step-$StepIndex machine-precondition-not-pass:$preReason" -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -MachinePrecondition $MachinePrecondition -CaptureBefore $CaptureBefore
        }
        throw "scenario-$scenario machine-precondition-blocked step=$StepIndex reason=$preReason"
    }
    Update-CampaignCapture $Context.Capture
    $stepAnchor = [long]$Context.Capture.ReadOffset
    $promptAt = Get-Now
    if ($null -eq $Context.FirstActionAt) { $Context.FirstActionAt = $promptAt }
    $completedAt = Read-OperatorEnter $scenario $StepIndex $stepId $ExpectedAction $MachinePrecondition $CaptureBefore
    $Context.LastActionCompletedAt = $completedAt
    # ADJ-20260808-0003: fixture events marked relative_to_prompt are stamped against the prompt
    # time, so a slow operator cannot shift device timestamps past the enter; the verification
    # window lower bound is the prompt (minus frozen skew), never the operator completedAt.
    Add-SimulationScenarioStepOutput $Context.Capture $scenario $StepIndex $promptAt $completedAt
    Update-CampaignCapture $Context.Capture
    $captureAfter = [ordered]@{ status = 'not-required' }
    if (-not [string]::IsNullOrWhiteSpace($CaptureAfterName)) {
        if ($CaptureAfterReviewOnly) {
            # A truly review-only capture is always ObservationOnly: it never enters the global
            # CaptureDegraded list and can never block or pass the scenario result.
            $captureStatus = Invoke-Capture $CaptureAfterName $scenario -ObservationOnly
            $captureAfter = [ordered]@{ status = $captureStatus; name = $CaptureAfterName; review_only = $true; note = 'review-only capture; never a semantic operator verdict' }
            Add-TranscriptRecord 'review-only-layout-artifact' ([ordered]@{ scenario = $scenario; checkpoint = $captureAfter })
        } elseif (-not [string]::IsNullOrWhiteSpace($CaptureAfterProfile)) {
            $captureAfter = Invoke-LayoutCheckpoint $scenario $CaptureAfterName $CaptureAfterProfile $CaptureAfterExpectedBundle -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -ObservationOnly:$CaptureAfterObservationOnly
        } else {
            $captureStatus = Invoke-Capture $CaptureAfterName $scenario -ObservationOnly:$CaptureAfterObservationOnly
            if ($captureStatus -ne 'collected') {
                # ADJ-20260808-0003: infrastructure capture failure (exit 124/125 / timeout / HDC
                # transport) must propagate as infrastructure blocked with retry, never as a
                # scenario invalid; only non-infrastructure capture loss stays invalid here.
                if ($script:LastCaptureInfrastructure) {
                    throw "HDC infrastructure interruption capture-after=$CaptureAfterName scenario=$scenario"
                }
                Throw-ScenarioInvalid -Scenario $scenario -Reason "step-$StepIndex capture-after-not-collected:$CaptureAfterName" -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -MachinePrecondition $MachinePrecondition -CaptureBefore $CaptureBefore -CaptureAfter ([ordered]@{ status = $captureStatus; name = $CaptureAfterName })
            }
            $captureAfter = [ordered]@{ status = $captureStatus; name = $CaptureAfterName }
        }
    }
    Write-OperatorWaitState 'verifying' -Scenario $scenario -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -CaptureAfter $captureAfter -MachinePrecondition $MachinePrecondition -MachinePostcondition ([ordered]@{ status = 'verifying' })
    # ADJ-20260808-0003: the event time lower bound for verification is the prompt (with frozen
    # device clock skew tolerance), never the operator completedAt. completedAt is recorded only
    # for mechanical attestation; events that land between prompt and enter (slow operator) must
    # still be captured by the machine condition.
    $outcome = Wait-MachineCondition $Context $stepAnchor $promptAt $Postcondition $VerifyTimeoutSeconds
    if ([string]$outcome.status -eq 'blocked') {
        $reason = [string](Get-OptionalProperty $outcome 'reason' 'machine-verification-blocked')
        throw "scenario-$scenario machine-verification-blocked step=$StepIndex reason=$reason"
    }
    if ([string]$outcome.status -ne 'pass') {
        $reason = [string](Get-OptionalProperty $outcome 'reason' 'event-postcondition-missing')
        Throw-ScenarioInvalid -Scenario $scenario -Reason "step-$StepIndex $reason" -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -MachinePrecondition $MachinePrecondition -MachinePostcondition $outcome -CaptureBefore $CaptureBefore -CaptureAfter $captureAfter
    }
    Write-OperatorWaitState 'captured' -Scenario $scenario -StepIndex $StepIndex -StepId $stepId -ExpectedAction $ExpectedAction -CaptureBefore $CaptureBefore -CaptureAfter $captureAfter -MachinePrecondition $MachinePrecondition -MachinePostcondition $outcome
    Write-Host "机器采集/判定完成：scenario=$scenario step=$StepIndex。"
    $step = [pscustomobject]@{ StepIndex = $StepIndex; StepId = $stepId; ExpectedAction = $ExpectedAction; PromptAt = $promptAt; CompletedAt = $completedAt; AnchorByte = $stepAnchor; Outcome = $outcome; CaptureBefore = $CaptureBefore; CaptureAfter = $captureAfter }
    $Context.Actions.Add($step)
    # Advance the operator action guard past this step's verified window so the next step only
    # scans events that arrived during the gap, never events already consumed by this step.
    $script:OperatorActionGuardFrom = Get-Now
    return $step
}

function Complete-ScenarioContext {
    param([Parameter(Mandatory)]$Context, [scriptblock]$DuringWait)
    $capture = $Context.Capture
    $actionCompletedAt = if ($null -ne $Context.LastActionCompletedAt) { [DateTimeOffset]$Context.LastActionCompletedAt } else { [DateTimeOffset]$Context.StartedAt }
    $requiredEnd = $actionCompletedAt.AddSeconds($script:WindowSeconds)
    $script:CurrentWindowEnd = $requiredEnd
    $windowIterations = 0
    while ((Get-Now) -lt $requiredEnd -and -not $capture.Degraded) {
        $windowIterations++
        if ($windowIterations -gt 5000) { break }
        $events = @(Get-ScenarioContextEvents $Context)
        if ($null -ne $DuringWait) { & $DuringWait $events }
        if ((Get-Now) -ge $requiredEnd -or $capture.Degraded) { break }
        if ($LiveSimulation) {
            $now = Get-Now
            $nextAt = $requiredEnd
            foreach ($event in @($capture.Events)) {
                if ([long]$event.raw_byte_start -lt [long]$Context.AnchorByte -or [string]::IsNullOrWhiteSpace([string]$event.device_observed_at)) { continue }
                $eventAt = [DateTimeOffset]::MinValue
                if ([DateTimeOffset]::TryParse([string]$event.device_observed_at, [ref]$eventAt) -and $eventAt -gt $now -and $eventAt -lt $nextAt) { $nextAt = $eventAt }
            }
            if ($nextAt -le $now) { $nextAt = $now.AddMilliseconds(1) }
            Wait-Until $nextAt
        } else {
            Start-Sleep -Milliseconds 250
        }
    }
    Update-CampaignCapture $capture
    $observedThrough = Get-Now
    $events = @(Get-ScenarioContextEvents $Context $observedThrough)
    if ($null -ne $DuringWait) { & $DuringWait $events }
    $script:CurrentWindowEnd = $null
    # Advance the operator action guard to the end of the window so the next scenario only scans
    # events that arrive after this scenario's window (cross-scenario gap), never events that
    # Assert-ScenarioEventContract already consumed.
    $script:OperatorActionGuardFrom = Get-Now
    [void](Test-CampaignCaptureHealth $capture)
    $windowDegraded = [bool]$capture.Degraded
    $coverageAfterAction = if ($null -ne $capture.LastHealthyAt) { [Math]::Max(0.0, ($capture.LastHealthyAt - $actionCompletedAt).TotalSeconds) } else { 0.0 }
    $completeWindowObserved = -not $windowDegraded -and $coverageAfterAction -ge $script:WindowSeconds -and $observedThrough -ge $requiredEnd
    $firstActionAt = if ($null -ne $Context.FirstActionAt) { [DateTimeOffset]$Context.FirstActionAt } else { [DateTimeOffset]$Context.StartedAt }
    $observation = [ordered]@{
        scenario = [int]$Context.Scenario
        protocol = 'mechanical-action-only-machine-verified-v1'
        campaign_capture_started_at = $capture.StartedAt.ToString('o')
        initial_anchor = $capture.InitialAnchor
        scenario_anchor_byte = [long]$Context.AnchorByte
        window_started_at = ([DateTimeOffset]$Context.StartedAt).ToString('o')
        action_prompt_at = $firstActionAt.ToString('o')
        action_completed_at = $actionCompletedAt.ToString('o')
        required_observation_end_at = $requiredEnd.ToString('o')
        observation_ended_at = $observedThrough.ToString('o')
        action_interval_seconds = ($actionCompletedAt - $firstActionAt).TotalSeconds
        measured_coverage_before_action_prompt_seconds = ($firstActionAt - [DateTimeOffset]$Context.StartedAt).TotalSeconds
        measured_coverage_after_action_seconds = $coverageAfterAction
        complete_window_observed = [bool]$completeWindowObserved
        operator_steps = @($Context.Actions | ForEach-Object { [ordered]@{ step_index = $_.StepIndex; step_id = $_.StepId; expected_action = $_.ExpectedAction; completed_at = $_.CompletedAt.ToString('o'); machine_postcondition = $_.Outcome } })
        capture_degraded = [bool]$windowDegraded
        capture_health = [ordered]@{
            process_present = [bool]($null -ne $capture.Process)
            process_exited = $(if ($null -ne $capture.Process) { try { [bool]$capture.Process.HasExited } catch { $true } } else { [bool]$capture.SimulatedDead })
            stderr_bytes = [long]$capture.LastStderrBytes
            last_healthy_at = $(if ($null -ne $capture.LastHealthyAt) { $capture.LastHealthyAt.ToString('o') } else { $null })
            measured = $true
        }
        device_clock_skew_tolerance_seconds = [double]$script:DeviceClockSkewToleranceSeconds
        events = Protect-SensitiveData $events
    }
    Add-TranscriptRecord 'scenario-observation' $observation
    $capture.ActiveScenario = 0
    return [pscustomobject]@{ Observation = $observation; Events = $events; CaptureDegraded = $windowDegraded; CompleteWindowObserved = $completeWindowObserved }
}

function Assert-ScenarioEventContract {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [object[]]$ExpectedStarts = @(),
        [object[]]$ExpectedStops = @()
    )
    $infos = @($Events | ForEach-Object { Get-E3EventInfo $_ })
    $skipped = @($infos | Where-Object { $_.Marker -in @('UI_START_SKIPPED', 'UI_STOP_SKIPPED') })
    if ($skipped.Count -gt 0) { Throw-ScenarioInvalid $Scenario "unexpected-$($skipped[0].Marker)" }
    $starts = @($infos | Where-Object { $_.Marker -eq 'UI_START' })
    if ($starts.Count -ne $ExpectedStarts.Count) { Throw-ScenarioInvalid $Scenario "UI_START-count expected=$($ExpectedStarts.Count) actual=$($starts.Count)" }
    for ($index = 0; $index -lt $ExpectedStarts.Count; $index++) {
        $expected = $ExpectedStarts[$index]
        if ([string]$starts[$index].Bundle -ne [string]$expected.Bundle) { Throw-ScenarioInvalid $Scenario "UI_START-order-or-bundle expected=$([string]$expected.Bundle) actual=$([string]$starts[$index].Bundle)" }
        if ([string]$starts[$index].RequestId -ne [string]$expected.RequestId) { Throw-ScenarioInvalid $Scenario "UI_START-requestId expected=$([string]$expected.RequestId) actual=$([string]$starts[$index].RequestId)" }
    }
    $stops = @($infos | Where-Object { $_.Marker -eq 'UI_STOP' })
    if ($stops.Count -ne $ExpectedStops.Count) { Throw-ScenarioInvalid $Scenario "UI_STOP-count expected=$($ExpectedStops.Count) actual=$($stops.Count)" }
    for ($index = 0; $index -lt $ExpectedStops.Count; $index++) {
        $expected = $ExpectedStops[$index]
        if ([string]$stops[$index].Bundle -ne [string]$expected.Bundle) { Throw-ScenarioInvalid $Scenario "UI_STOP-bundle expected=$([string]$expected.Bundle) actual=$([string]$stops[$index].Bundle)" }
        if ([string]$stops[$index].RequestId -ne [string]$expected.RequestId) { Throw-ScenarioInvalid $Scenario "UI_STOP-requestId expected=$([string]$expected.RequestId) actual=$([string]$stops[$index].RequestId)" }
    }
    $allowed = @{}
    foreach ($expected in @($ExpectedStarts + $ExpectedStops)) { $allowed[[string]$expected.RequestId] = [string]$expected.Bundle }
    foreach ($info in $infos) {
        if ([string]::IsNullOrWhiteSpace([string]$info.Marker) -or $info.Marker -notmatch '^(UI|VPN|CREATE|START|STOP|DESTROY|FD|SESSION|PROMISE|ENTRY|LATE)_') { continue }
        if (-not [string]::IsNullOrWhiteSpace([string]$info.RequestId) -and [string]$info.RequestId -ne 'missing') {
            if (-not $allowed.ContainsKey([string]$info.RequestId)) { Throw-ScenarioInvalid $Scenario "unexpected-requestId:$([string]$info.RequestId) marker=$([string]$info.Marker)" }
            $expectedBundle = [string]$allowed[[string]$info.RequestId]
            if (-not [string]::IsNullOrWhiteSpace([string]$info.Bundle) -and [string]$info.Bundle -ne $expectedBundle) { Throw-ScenarioInvalid $Scenario "wrong-bundle-for-requestId:$([string]$info.RequestId)" }
        }
    }
    return $true
}

function Get-RequestIdFromEvents {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events, [Parameter(Mandatory)][string]$Bundle)
    foreach ($event in $Events) {
        if ([string]$event.text -match "UI_START\|bundle=$([regex]::Escape($Bundle))\|requestId=([^|\s]+)") {
            if ($Matches[1] -ne 'missing') { return $Matches[1] }
        }
    }
    return $null
}

function Test-CorrelatedMarker {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [Parameter(Mandatory)][string]$RequestId,
        [Parameter(Mandatory)][string]$Marker
    )
    foreach ($event in $Events) {
        $text = [string]$event.text
        if (-not $text.Contains($Marker) -or $text -notmatch "requestId=$([regex]::Escape($RequestId))(\||\s|$)") { continue }
        if ($text -match 'bundle=([^|\s]+)' -and $Matches[1] -ne $Bundle) { continue }
        return $true
    }
    return $false
}

function Get-StopRequestFromEvents {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [AllowNull()][string]$ExpectedBundle = $null
    )
    # Strict whole-marker tokens only: UI_STOP | STOP_PROMISE_RESOLVED | STOP_PROMISE_REJECTED.
    # Real token boundaries on both sides: timestamp prefix allowed, but UI_STOP_SKIPPED / LATE /
    # SESSION_RELEASED / destroy-only ignored, and requestId must end at a field boundary (|) or EOL,
    # so markers quoted inside prose/summary text are never treated as stop requests.
    # Collect all candidates; return only when exactly one unique requestId remains.
    $candidates = [ordered]@{}
    foreach ($event in @($Events)) {
        $text = [string]$event.text
        $requestId = $null
        $bundle = $null
        if ($text -match '(?:^|\s)UI_STOP\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)') {
            $bundle = [string]$Matches[1]
            $requestId = [string]$Matches[2]
        } elseif ($text -match '(?:^|\s)STOP_PROMISE_RESOLVED\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)') {
            $bundle = [string]$Matches[1]
            $requestId = [string]$Matches[2]
        } elseif ($text -match '(?:^|\s)STOP_PROMISE_REJECTED\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)') {
            $bundle = [string]$Matches[1]
            $requestId = [string]$Matches[2]
        } else {
            continue
        }
        if ($requestId -eq 'missing') { continue }
        if (-not [string]::IsNullOrWhiteSpace($ExpectedBundle)) {
            if ([string]::IsNullOrWhiteSpace([string]$bundle) -or [string]$bundle -ne [string]$ExpectedBundle) { continue }
        }
        if (-not $candidates.Contains($requestId)) { $candidates[$requestId] = $bundle }
    }
    if ($candidates.Count -ne 1) { return $null }
    $onlyKey = @($candidates.Keys)[0]
    return [pscustomobject]@{ RequestId = $onlyKey; Bundle = $candidates[$onlyKey] }
}

function Get-DestroyAssessment {
    param([Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events, [Parameter(Mandatory)][string]$Bundle, [AllowNull()][string]$RequestId)
    $effectiveRequestId = [string]$RequestId
    if ([string]::IsNullOrWhiteSpace($effectiveRequestId) -or $effectiveRequestId -eq 'missing') {
        $inferred = Get-StopRequestFromEvents -Events $Events -ExpectedBundle $Bundle
        if ($null -ne $inferred) {
            $effectiveRequestId = [string]$inferred.RequestId
            if (-not [string]::IsNullOrWhiteSpace([string]$inferred.Bundle)) { $Bundle = [string]$inferred.Bundle }
        }
    }
    if ([string]::IsNullOrWhiteSpace($effectiveRequestId) -or $effectiveRequestId -eq 'missing') {
        $hasStopOrDestroyEvidence = @($Events | Where-Object { [string]$_.text -match 'UI_STOP|STOP_PROMISE_|STOP_SESSION_RELEASED|VPN_ONDESTROY|VPN_DESTROY_|VPN_FD_SNAPSHOT' }).Count -gt 0
        $reason = if ($hasStopOrDestroyEvidence) { 'destroy-requestId-unresolved' } else { 'no-destroy-or-stop-marker-observed' }
        return [pscustomobject]@{ result = 'blocked'; reason = $reason }
    }
    # Hard fail before any terminal/snapshot early-return: an explicit post-destroy-phase
    # FD_STILL_OPEN marker on the same bundle/request is a leaked fd no matter whether the destroy
    # terminal or post-destroy snapshot are present. It must never be downgraded to blocked or
    # overridden by the strict-process-boundary fallback.
    if (Test-S5PostDestroyStillOpen -Events $Events -Bundle $Bundle -RequestId $effectiveRequestId) {
        return [pscustomobject]@{ result = 'fail'; reason = 'fd-still-open-after-destroy' }
    }
    $terminal = (Test-CorrelatedMarker $Events $Bundle $effectiveRequestId 'VPN_DESTROY_RESOLVED') -or (Test-CorrelatedMarker $Events $Bundle $effectiveRequestId 'VPN_DESTROY_REJECTED')
    $snapshot = (Test-CorrelatedMarker $Events $Bundle $effectiveRequestId 'VPN_FD_SNAPSHOT') -and @($Events | Where-Object {
        [string]$_.text -match "requestId=$([regex]::Escape($effectiveRequestId))(\||\s|$)" -and [string]$_.text -match 'phase=post-destroy-(resolved|rejected)'
    }).Count -gt 0
    if (-not $terminal -and -not $snapshot) { return [pscustomobject]@{ result = 'blocked'; reason = 'destroy-terminal-or-post-snapshot-missing' } }
    if (-not $terminal) { return [pscustomobject]@{ result = 'blocked'; reason = 'destroy-terminal-missing' } }
    if (-not $snapshot) { return [pscustomobject]@{ result = 'blocked'; reason = 'post-destroy-snapshot-missing' } }
    $correlated = @($Events | Where-Object { [string]$_.text -match "requestId=$([regex]::Escape($effectiveRequestId))(\||\s|$)" })
    $combined = ($correlated.text -join "`n")
    if ($combined.Contains('FD_STILL_OPEN')) { return [pscustomobject]@{ result = 'fail'; reason = 'FD_STILL_OPEN' } }
    if ($combined.Contains('FD_STATE_UNCONFIRMED')) { return [pscustomobject]@{ result = 'blocked'; reason = 'FD_STATE_UNCONFIRMED' } }
    if ($combined -notmatch 'FD_CLOSED_CONFIRMED|FD_NOT_OPEN_AFTER_DESTROY') { return [pscustomobject]@{ result = 'blocked'; reason = 'destroy-fd-decision-missing' } }
    # Callback terminal pass requires the same-request onDestroy marker; missing it cannot pass.
    if (-not (Test-CorrelatedMarker $Events $Bundle $effectiveRequestId 'VPN_ONDESTROY')) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'destroy-ondestroy-missing' }
    }
    return [pscustomobject]@{ result = 'pass'; reason = 'terminal-and-post-destroy-snapshot-confirmed' }
}

function Get-DenyAssessment {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [AllowNull()][string]$RequestId,
        [bool]$DenyScreenshot,
        [bool]$FullWindowObserved
    )
    $bRequestIds = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($event in @($Events)) {
        if ([string]$event.text -match "UI_START\|bundle=$([regex]::Escape($Bundle))\|requestId=([^|\s]+)") {
            if ($Matches[1] -ne 'missing') { [void]$bRequestIds.Add([string]$Matches[1]) }
        }
    }
    $createMarkers = @('VPN_ONCREATE', 'VPN_CREATE_BEGIN', 'VPN_CREATE_RESOLVED', 'CREATE_ACCEPTED')
    foreach ($event in @($Events)) {
        $text = [string]$event.text
        $hasCreate = $false
        foreach ($marker in $createMarkers) {
            if ($text.Contains($marker, [StringComparison]::Ordinal)) { $hasCreate = $true; break }
        }
        if (-not $hasCreate) { continue }
        $isBundle = $text -match "bundle=$([regex]::Escape($Bundle))(\||\s|$)"
        $eventRequestId = $null
        if ($text -match 'requestId=([^|\s]+)') { $eventRequestId = [string]$Matches[1] }
        if ($isBundle -or ($null -ne $eventRequestId -and $bRequestIds.Contains($eventRequestId))) {
            return [pscustomobject]@{ result = 'fail'; reason = 'deny-created-B-vpn' }
        }
    }
    if ([string]::IsNullOrWhiteSpace($RequestId)) { return [pscustomobject]@{ result = 'blocked'; reason = 'B-requestId-missing' } }
    $reject = (Test-CorrelatedMarker $Events $Bundle $RequestId 'START_PROMISE_REJECTED') -or
        (Test-CorrelatedMarker $Events $Bundle $RequestId 'VPN_CREATE_REJECTED')
    if ($reject) { return [pscustomobject]@{ result = 'pass'; reason = 'observable-B-request-rejection' } }
    if ($DenyScreenshot -and $FullWindowObserved) { return [pscustomobject]@{ result = 'pass'; reason = 'deny-layout-and-full-window-without-B-create' } }
    return [pscustomobject]@{ result = 'blocked'; reason = 'deny-proof-incomplete' }
}

function Get-ProcessProbeStatus {
    param([Parameter(Mandatory)]$PidResult, [Parameter(Mandatory)]$DumpResult, [Parameter(Mandatory)][string]$Bundle)
    # Single-probe classification:
    # - PidOf decides process state only: blank stdout + exit 0/1 + no stderr => absent candidate;
    #   non-blank PID stdout + exit 0 + no stderr => present; anything else => unknown/error.
    # - BundleDump decides bundle_present only via Get-BundleDumpAssessment: exit 0 AND Status=pass
    #   => bundle_present=true. Non-zero exit, stderr, permission, absent text, garbage, or any
    #   non-pass assessment => unknown/error and aborts the series (never accumulates as absent).
    $pidExit = [int]$PidResult.ExitCode
    if ($pidExit -in @(124, 125)) { return [pscustomobject]@{ status = 'error'; bundle_present = $false; detail = 'pid-exit-infrastructure' } }
    if (-not [string]::IsNullOrWhiteSpace([string]$PidResult.Stderr)) { return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = 'pid-stderr' } }
    $pidOut = [string]$PidResult.Stdout
    $pidBlank = [string]::IsNullOrWhiteSpace($pidOut)
    $processStatus = $null
    if (-not $pidBlank) {
        if ($pidExit -eq 0) { $processStatus = 'present' }
        else { return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = "pid-exit-$pidExit-with-output" } }
    } else {
        if ($pidExit -in @(0, 1)) { $processStatus = 'absent' }
        else { return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = "pid-exit-$pidExit" } }
    }
    $dumpExit = [int]$DumpResult.ExitCode
    if ($dumpExit -in @(124, 125)) { return [pscustomobject]@{ status = 'error'; bundle_present = $false; detail = 'dump-infrastructure' } }
    if (-not [string]::IsNullOrWhiteSpace([string]$DumpResult.Stderr)) {
        return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = 'dump-stderr' }
    }
    if ($dumpExit -ne 0) {
        return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = "dump-exit-$dumpExit" }
    }
    $dumpAssessment = Get-BundleDumpAssessment $DumpResult $Bundle
    if ($dumpAssessment.Status -eq 'infrastructure') {
        return [pscustomobject]@{ status = 'error'; bundle_present = $false; detail = [string]$dumpAssessment.Reason }
    }
    if ($dumpAssessment.Status -ne 'pass') {
        return [pscustomobject]@{ status = 'unknown'; bundle_present = $false; detail = [string]$dumpAssessment.Reason }
    }
    return [pscustomobject]@{ status = $processStatus; bundle_present = $true; detail = $null }
}

function New-ProcessProbeContext {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][string]$Bundle,
        [bool]$RequireBundlePresent = $false,
        [int]$RequiredCount = 2,
        [double]$SpacingSeconds = 3.0
    )
    return [pscustomobject]@{
        Scenario = $Scenario
        Bundle = $Bundle
        # ADJ-20260808-0001: probe target is the <bundle>:vpn Extension ability process.
        ProcessTarget = "${Bundle}:vpn"
        RequireBundlePresent = [bool]$RequireBundlePresent
        RequiredCount = [int]$RequiredCount
        SpacingSeconds = [double]$SpacingSeconds
        Started = $false
        Finished = $false
        Aborted = $false
        Terminal = $false
        ConsecutiveAbsent = 0
        BundlePresent = $false
        Probes = [Collections.Generic.List[object]]::new()
        LastProbeAt = $null
        OverrideProbeIndex = 0
    }
}

function Get-SimulationFailureMatch {
    param([Parameter(Mandatory)][string]$Operation, [Parameter(Mandatory)][int]$Occurrence)
    foreach ($failure in @(Get-OptionalProperty $script:Simulation 'hdc_failures' @())) {
        if ([string](Get-OptionalProperty $failure 'operation' '') -eq $Operation -and [int](Get-OptionalProperty $failure 'occurrence' 1) -eq $Occurrence) {
            return [pscustomobject]@{
                ExitCode = [int](Get-OptionalProperty $failure 'exit_code' 1)
                Stdout = [string](Get-OptionalProperty $failure 'stdout' '')
                Stderr = [string](Get-OptionalProperty $failure 'stderr' 'simulated command failure')
                Simulated = $true
                FromFailure = $true
            }
        }
    }
    return $null
}

function Get-ProcessProbeOverrideResult {
    param(
        [Parameter(Mandatory)][ValidateSet('PidOf', 'BundleDump')][string]$Operation,
        [Parameter(Mandatory)][string]$Bundle,
        [AllowNull()]$Entry
    )
    # Strict enum only. Unknown values deliberately produce an unknown classification, never a default absent/present pass.
    if ($Operation -eq 'PidOf') {
        $pidStatus = if ($null -eq $Entry) { 'absent' } else { [string](Get-OptionalProperty $Entry 'pid' '') }
        switch ($pidStatus) {
            'present' { return [pscustomobject]@{ ExitCode = 0; Stdout = '12345'; Stderr = ''; Simulated = $true } }
            'absent' { return [pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = ''; Simulated = $true } }
            'error' { return [pscustomobject]@{ ExitCode = 124; Stdout = ''; Stderr = 'simulated pidof error'; Simulated = $true } }
            'unknown' { return [pscustomobject]@{ ExitCode = 2; Stdout = ''; Stderr = ''; Simulated = $true } }
            default { return [pscustomobject]@{ ExitCode = 3; Stdout = 'garbage-override'; Stderr = 'invalid-pid-override'; Simulated = $true } }
        }
    }
    $dumpStatus = if ($null -eq $Entry) { 'present' } else { [string](Get-OptionalProperty $Entry 'dump' '') }
    switch ($dumpStatus) {
        'present' { return [pscustomobject]@{ ExitCode = 0; Stdout = '{ "app": { "bundleName": "' + $Bundle + '" } }'; Stderr = ''; Simulated = $true } }
        'absent' { return [pscustomobject]@{ ExitCode = 0; Stdout = 'error: failed to get information and the parameters may be wrong.'; Stderr = ''; Simulated = $true } }
        'error' { return [pscustomobject]@{ ExitCode = 124; Stdout = ''; Stderr = 'simulated dump error'; Simulated = $true } }
        'unknown' { return [pscustomobject]@{ ExitCode = 0; Stdout = 'Permission denied'; Stderr = ''; Simulated = $true } }
        default { return [pscustomobject]@{ ExitCode = 3; Stdout = 'garbage-override'; Stderr = 'invalid-dump-override'; Simulated = $true } }
    }
}

function Invoke-SimulationProbePair {
    param([Parameter(Mandatory)]$Context)
    $bundle = [string]$Context.Bundle
    $scenario = [int]$Context.Scenario
    # Each probe pair is two logical HDC operations (PidOf + BundleDump), matching two transcript commands.
    $script:HdcLogicalCallCount += 2
    $pidAudit = Get-HdcInvocation 'PidOf' @{ Bundle = $bundle }
    $dumpAudit = Get-HdcInvocation 'BundleDump' @{ Bundle = $bundle }
    Add-TranscriptRecord 'hdc-command' ([ordered]@{ operation = 'PidOf'; executable = '<HDC_PATH>'; arguments = $pidAudit; timeout_seconds = $HdcTimeoutSeconds; simulated = $true })
    Add-TranscriptRecord 'hdc-command' ([ordered]@{ operation = 'BundleDump'; executable = '<HDC_PATH>'; arguments = $dumpAudit; timeout_seconds = $HdcTimeoutSeconds; simulated = $true })
    $scenarioOverride = Get-OptionalProperty (Get-OptionalProperty $script:Simulation 'process_probe_override' @{}) ([string]$scenario) $null
    $entry = $null
    if ($null -ne $scenarioOverride) {
        $entries = @($scenarioOverride)
        $index = [int]$Context.OverrideProbeIndex
        $entry = if ($entries.Count -gt 0) { if ($index -lt $entries.Count) { $entries[$index] } else { $entries[$entries.Count - 1] } } else { $null }
        $Context.OverrideProbeIndex = $index + 1
    }
    # Occurrence counters advance for every probe so hdc_failures stay aligned with later live ops.
    if (-not $script:HdcOperationCounts.ContainsKey('PidOf')) { $script:HdcOperationCounts['PidOf'] = 0 }
    if (-not $script:HdcOperationCounts.ContainsKey('BundleDump')) { $script:HdcOperationCounts['BundleDump'] = 0 }
    $script:HdcOperationCounts['PidOf']++
    $script:HdcOperationCounts['BundleDump']++
    $pidOccurrence = [int]$script:HdcOperationCounts['PidOf']
    $dumpOccurrence = [int]$script:HdcOperationCounts['BundleDump']
    # hdc_failures always win over process_probe_override; override cannot bypass injected failures.
    $pidFailure = Get-SimulationFailureMatch 'PidOf' $pidOccurrence
    $dumpFailure = Get-SimulationFailureMatch 'BundleDump' $dumpOccurrence
    $pidResult = if ($null -ne $pidFailure) {
        $pidFailure
    } elseif ($null -ne $scenarioOverride) {
        Get-ProcessProbeOverrideResult 'PidOf' $bundle $entry
    } else {
        [pscustomobject]@{ ExitCode = 0; Stdout = ''; Stderr = ''; Simulated = $true }
    }
    $dumpResult = if ($null -ne $dumpFailure) {
        $dumpFailure
    } elseif ($null -ne $scenarioOverride) {
        Get-ProcessProbeOverrideResult 'BundleDump' $bundle $entry
    } else {
        $installed = ($bundle -eq $script:BundleA -and $script:SimulationInstalledA) -or ($bundle -eq $script:BundleB -and $script:SimulationInstalledB)
        if ($installed) {
            [pscustomobject]@{ ExitCode = 0; Stdout = '{ "app": { "bundleName": "' + $bundle + '" } }'; Stderr = ''; Simulated = $true }
        } else {
            [pscustomobject]@{ ExitCode = 0; Stdout = 'error: failed to get information and the parameters may be wrong.'; Stderr = ''; Simulated = $true }
        }
    }
    foreach ($pair in @(@{ op = 'PidOf'; result = $pidResult }, @{ op = 'BundleDump'; result = $dumpResult })) {
        Add-TranscriptRecord 'hdc-result' ([ordered]@{
            operation = $pair.op
            exit_code = [int]$pair.result.ExitCode
            stdout = [string]$pair.result.Stdout
            stderr = [string]$pair.result.Stderr
            simulated = $true
        })
    }
    # Keep infrastructure classification identical to live Invoke-HdcOperation: 124/125 (or a
    # timeout stderr) marks the campaign as hdc-usb-interruption, not a functional result.
    foreach ($probeResult in @($pidResult, $dumpResult)) {
        if ([int]$probeResult.ExitCode -in @(124, 125) -or [string]$probeResult.Stderr -match '(?i)\btimeout\b') {
            $script:InfrastructureReasonObserved = 'hdc-usb-interruption'
        }
    }
    return [pscustomobject]@{ PidResult = $pidResult; DumpResult = $dumpResult }
}

function Invoke-ProcessProbePair {
    param([Parameter(Mandatory)]$Context)
    $bundle = [string]$Context.Bundle
    if ($LiveSimulation) { return Invoke-SimulationProbePair $Context }
    $pidResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
    $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
    return [pscustomobject]@{ PidResult = $pidResult; DumpResult = $dumpResult }
}

function Invoke-ProcessFinalStateProbeSeries {
    param([Parameter(Mandatory)]$Context, [AllowNull()][DateTimeOffset]$Deadline = $null)
    if ([bool]$Context.Finished) { return $Context }
    # Probes are window-bound only. Never open a fresh post-window 60s series; late stop stays blocked.
    if ($null -eq $Deadline) {
        if ($null -ne $script:CurrentWindowEnd) { $Deadline = [DateTimeOffset]$script:CurrentWindowEnd }
        else { return $Context }
    }
    if ((Get-Now) -ge $Deadline) { return $Context }
    $Context.Started = $true
    $spacingSeconds = [double]$Context.SpacingSeconds
    if ($LiveSimulation) {
        $overrideSpacing = Get-OptionalProperty $script:Simulation 'probe_spacing_override_seconds' $null
        if ($null -ne $overrideSpacing) { $spacingSeconds = [double]$overrideSpacing }
    }
    while (-not [bool]$Context.Finished -and -not [bool]$Context.Aborted) {
        if ($null -ne $Context.LastProbeAt) {
            # Round-trip the exact DateTimeOffset (never [string], which drops sub-second precision)
            # and add a small scheduling margin so the recorded spacing actually reaches the frozen
            # 3.0s rule on the device clock; the rule threshold itself is never lowered and the
            # assessment still re-checks recorded timestamps (no post-hoc tolerance override).
            $nextProbeAt = ([DateTimeOffset]$Context.LastProbeAt).AddSeconds($spacingSeconds + 0.1)
            if ((Get-Now) -lt $nextProbeAt) { Wait-Until $nextProbeAt }
        }
        if ((Get-Now) -ge $Deadline) { break }
        $probeAt = Get-Now
        $pair = Invoke-ProcessProbePair $Context
        $classification = Get-ProcessProbeStatus $pair.PidResult $pair.DumpResult $Context.Bundle
        $status = [string]$classification.status
        if ($status -eq 'present') {
            $Context.ConsecutiveAbsent = 0
        } elseif ($status -eq 'absent') {
            $Context.ConsecutiveAbsent++
            $Context.BundlePresent = [bool]$classification.bundle_present
        } elseif ($status -eq 'error' -or $status -eq 'unknown') {
            $Context.Aborted = $true
            $Context.Finished = $true
        }
        $previous = if ($Context.Probes.Count -gt 0) { $Context.Probes[$Context.Probes.Count - 1] } else { $null }
        $spacingSincePrevious = if ($null -ne $previous) { ($probeAt - [DateTimeOffset]::Parse([string]$previous.time)).TotalSeconds } else { 0.0 }
        $probeRecord = [ordered]@{
            time = $probeAt.ToString('o')
            status = $status
            detail = $classification.detail
            process_target = [string]$Context.ProcessTarget
            bundle_present = [bool]$classification.bundle_present
            consecutive_absent = [int]$Context.ConsecutiveAbsent
            spacing_seconds_since_previous = [double]$spacingSincePrevious
        }
        $Context.Probes.Add($probeRecord)
        Add-TranscriptRecord 'process-final-state-probe' ([ordered]@{
            scenario = [int]$Context.Scenario
            bundle = [string]$Context.Bundle
            probe = $probeRecord
        })
        if ([int]$Context.ConsecutiveAbsent -ge [int]$Context.RequiredCount -and -not [bool]$Context.Aborted) {
            # Reaching the consecutive-absent count alone is not enough: the tail must actually be
            # spaced >= the frozen spacing. Otherwise keep probing inside the window instead of
            # finishing early; Test-ProcessAbsentEvidence re-checks recorded timestamps and would
            # stay blocked if we stopped here.
            $absentTail = @($Context.Probes | Where-Object { [string]$_.status -eq 'absent' } | Select-Object -Last ([int]$Context.RequiredCount))
            if ($absentTail.Count -ge [int]$Context.RequiredCount) {
                $firstAbsentAt = [DateTimeOffset]::Parse([string]$absentTail[0].time)
                $lastAbsentAt = [DateTimeOffset]::Parse([string]$absentTail[$absentTail.Count - 1].time)
                if (($lastAbsentAt - $firstAbsentAt).TotalSeconds -ge ([double]$Context.SpacingSeconds - 0.001)) {
                    $Context.Terminal = $true
                    $Context.Finished = $true
                }
            }
        }
        $Context.LastProbeAt = $probeAt
    }
    return $Context
}

function Test-StrictFallbackPrerequisites {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [AllowNull()][string]$RequestId = $null
    )
    # Strict-process-boundary fallback marker gate for S3/S7: a unique legal stop for the same
    # bundle in the current window plus onDestroy plus destroy-begin (or pre-destroy snapshot).
    # VPN_DESTROY_ISSUED never counts as begin/terminal and is ignored everywhere.
    $stop = $null
    if (-not [string]::IsNullOrWhiteSpace($RequestId)) {
        $candidate = Get-StopRequestFromEvents -Events $Events -ExpectedBundle $Bundle
        if ($null -eq $candidate -or [string]$candidate.RequestId -ne [string]$RequestId) {
            return [pscustomobject]@{ Met = $false; Stop = $null; RequestId = [string]$RequestId; Reason = 'strict-fallback-stop-unique-missing' }
        }
        if (-not ((Test-CorrelatedMarker $Events $Bundle $RequestId 'UI_STOP') -or (Test-CorrelatedMarker $Events $Bundle $RequestId 'STOP_PROMISE_RESOLVED'))) {
            return [pscustomobject]@{ Met = $false; Stop = $candidate; RequestId = [string]$RequestId; Reason = 'strict-fallback-stop-marker-missing' }
        }
        $stop = $candidate
    } else {
        $stop = Get-StopRequestFromEvents -Events $Events -ExpectedBundle $Bundle
        if ($null -eq $stop) { return [pscustomobject]@{ Met = $false; Stop = $null; RequestId = $null; Reason = 'strict-fallback-stop-unique-missing' } }
        $rid = [string]$stop.RequestId
        if (-not ((Test-CorrelatedMarker $Events $Bundle $rid 'UI_STOP') -or (Test-CorrelatedMarker $Events $Bundle $rid 'STOP_PROMISE_RESOLVED'))) {
            return [pscustomobject]@{ Met = $false; Stop = $stop; RequestId = $rid; Reason = 'strict-fallback-stop-marker-missing' }
        }
    }
    $rid = [string]$stop.RequestId
    if (-not (Test-CorrelatedMarker $Events $Bundle $rid 'VPN_ONDESTROY')) {
        return [pscustomobject]@{ Met = $false; Stop = $stop; RequestId = $rid; Reason = 'strict-fallback-ondestroy-missing' }
    }
    $preSnapshot = @($Events | Where-Object {
        [string]$_.text -match "requestId=$([regex]::Escape($rid))(\||\s|$)" -and
        [string]$_.text -match 'VPN_FD_SNAPSHOT' -and
        [string]$_.text -match 'phase=pre-destroy'
    }).Count -gt 0
    $begin = (Test-CorrelatedMarker $Events $Bundle $rid 'VPN_DESTROY_BEGIN') -or $preSnapshot
    if (-not $begin) {
        return [pscustomobject]@{ Met = $false; Stop = $stop; RequestId = $rid; Reason = 'strict-fallback-destroy-begin-missing' }
    }
    return [pscustomobject]@{ Met = $true; Stop = $stop; RequestId = $rid; Reason = $null }
}

function Get-VpnFinalState {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [AllowNull()][string]$RequestId = $null,
        [AllowNull()]$ProbeState = $null,
        [bool]$RequireBundlePresent = $false,
        [int]$RequiredCount = 2,
        [double]$SpacingSeconds = 3.0
    )
    # Priority 1: existing callback terminal + post-destroy fd snapshot. FD_STILL_OPEN is a hard
    # fail and can never fall back to the process-boundary route.
    $callback = Get-DestroyAssessment -Events $Events -Bundle $Bundle -RequestId $RequestId
    if ($callback.result -eq 'fail' -and $callback.reason -in @('FD_STILL_OPEN', 'fd-still-open-after-destroy')) {
        return [pscustomobject]@{ result = 'fail'; reason = $callback.reason; terminal_mode = 'callback-post-fd'; callback = $callback; strict = $null }
    }
    if ($callback.result -eq 'pass') {
        return [pscustomobject]@{ result = 'pass'; reason = 'terminal-and-post-destroy-snapshot-confirmed'; terminal_mode = 'callback-post-fd'; callback = $callback; strict = $null }
    }
    # Priority 2: strict-process-boundary fallback (S3/S7 only). Every missing or uncertain piece is blocked.
    $strict = Test-StrictFallbackPrerequisites -Events $Events -Bundle $Bundle -RequestId $RequestId
    if (-not $strict.Met) {
        return [pscustomobject]@{ result = 'blocked'; reason = $strict.Reason; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    if ($null -eq $ProbeState -or -not [bool]$ProbeState.Started) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-probes-not-started'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    if ([bool]$ProbeState.Aborted) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-probe-unknown-or-error'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    if (-not [bool]$ProbeState.Terminal) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-process-absent-insufficient'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    $absentProbes = @($ProbeState.Probes | Where-Object { [string]$_.status -eq 'absent' })
    if ($absentProbes.Count -lt $RequiredCount) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-process-absent-insufficient'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    $lastAbsent = $absentProbes[$absentProbes.Count - 1]
    $previousAbsent = $absentProbes[$absentProbes.Count - 2]
    $spacing = ([DateTimeOffset]::Parse([string]$lastAbsent.time) - [DateTimeOffset]::Parse([string]$previousAbsent.time)).TotalSeconds
    if ($spacing -lt ($SpacingSeconds - 0.001)) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-probe-spacing-insufficient'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    if ($RequireBundlePresent -and -not [bool]$ProbeState.BundlePresent) {
        return [pscustomobject]@{ result = 'blocked'; reason = 'strict-fallback-bundle-absent'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
    }
    return [pscustomobject]@{ result = 'pass'; reason = 'strict-process-boundary-terminal'; terminal_mode = 'strict-process-boundary'; callback = $callback; strict = $strict }
}

function Test-S5PostDestroyStillOpen {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [AllowNull()][string]$RequestId
    )
    # S5 hard-fail detection on the current request: an explicit FD_STILL_OPEN marker on a
    # post-destroy-phase snapshot or on a destroy terminal is a leaked fd and fails, and can never
    # be overridden by consecutive-absent process probes. Pre-destroy open snapshots never count.
    # Same-bundle/request marker only: an explicit bundle field must equal the target bundle.
    if ([string]::IsNullOrWhiteSpace($RequestId) -or $RequestId -eq 'missing') { return $false }
    foreach ($event in @($Events)) {
        $text = [string]$event.text
        if (-not $text.Contains('FD_STILL_OPEN')) { continue }
        if ($text -notmatch "requestId=$([regex]::Escape([string]$RequestId))(\||\s|$)") { continue }
        if ($text -match 'bundle=([^|\s]+)' -and $Matches[1] -ne $Bundle) { continue }
        if ($text -match 'VPN_DESTROY_RESOLVED|VPN_DESTROY_REJECTED|phase=post-destroy') { return $true }
    }
    return $false
}

function Test-ProcessAbsentEvidence {
    param(
        [Parameter(Mandatory)]$ProbeState,
        [int]$RequiredCount = 2,
        [double]$SpacingSeconds = 3.0
    )
    # Timestamp-based re-check of the recorded probe series. The runner never trusts execution-time
    # Wait/Terminal flags alone: the last RequiredCount probes must be consecutive absent and the
    # first-to-last spacing must be >= SpacingSeconds. A probe_spacing_override below the freeze
    # spacing therefore stays blocked.
    if ($null -eq $ProbeState -or -not [bool]$ProbeState.Started) {
        return [pscustomobject]@{ Met = $false; Reason = 'probes-not-started'; SpacingSeconds = $null }
    }
    if ([bool]$ProbeState.Aborted) {
        return [pscustomobject]@{ Met = $false; Reason = 'probe-unknown-or-error'; SpacingSeconds = $null }
    }
    $probes = @($ProbeState.Probes)
    if ($probes.Count -lt $RequiredCount) {
        return [pscustomobject]@{ Met = $false; Reason = 'process-absent-probes-insufficient'; SpacingSeconds = $null }
    }
    $tail = @($probes | Select-Object -Last $RequiredCount)
    foreach ($probe in $tail) {
        if ([string]$probe.status -ne 'absent') {
            return [pscustomobject]@{ Met = $false; Reason = 'process-absent-probes-insufficient'; SpacingSeconds = $null }
        }
    }
    $firstAt = [DateTimeOffset]::Parse([string]$tail[0].time)
    $lastAt = [DateTimeOffset]::Parse([string]$tail[$tail.Count - 1].time)
    $measured = ($lastAt - $firstAt).TotalSeconds
    if ($measured -lt ($SpacingSeconds - 0.001)) {
        return [pscustomobject]@{ Met = $false; Reason = 'probe-spacing-insufficient'; SpacingSeconds = $measured }
    }
    return [pscustomobject]@{ Met = $true; Reason = $null; SpacingSeconds = $measured }
}

function Test-PostCreateOpen {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Events,
        [Parameter(Mandatory)][string]$Bundle,
        [Parameter(Mandatory)][string]$RequestId
    )
    # Clean reactivation proof: the fresh request shows CREATE_ACCEPTED plus a post-create fd snapshot
    # with open=true. Exact field-boundary match only: `|open=true|` counts, `reopen=true` never does.
    # Same-bundle/request marker only: an explicit bundle field must equal the target bundle.
    foreach ($event in @($Events)) {
        $text = [string]$event.text
        if ($text -notmatch "requestId=$([regex]::Escape($RequestId))(\||\s|$)") { continue }
        if ($text -notmatch 'VPN_FD_SNAPSHOT') { continue }
        if ($text -notmatch 'phase=post-create') { continue }
        if ($text -notmatch '(?:^|\|)open=true(?:\||$)') { continue }
        if ($text -match 'bundle=([^|\s]+)' -and $Matches[1] -ne $Bundle) { continue }
        return $true
    }
    return $false
}

function Get-ScenarioAggregation {
    param([Parameter(Mandatory)][object[]]$Scenarios, [bool]$IntegrityViolation = $false)
    if ($IntegrityViolation) { return 'invalid' }
    $results = @($Scenarios | ForEach-Object { [string]$_.result })
    if ($results -contains 'invalid') { return 'invalid' }
    if ($results -contains 'fail') { return 'fail' }
    if ($results -contains 'blocked') { return 'blocked' }
    if ($results.Count -ne 7 -or @($results | Where-Object { $_ -ne 'pass' }).Count -gt 0) { return 'invalid' }
    # S3 strict-process-boundary fallback pass additionally requires a clean reactivation proof:
    # the same bundle must show a subsequent fresh request CREATE_ACCEPTED plus post-create open
    # (scenario 5). Without it the fallback process-absent observation could be an uninstall/death,
    # so overall must stay blocked even though every individual scenario passed.
    $s3 = @($Scenarios | Where-Object { [int]$_.scenario -eq 3 })[0]
    if ($null -ne $s3 -and [string]$s3.terminal_mode -eq 'strict-process-boundary' -and -not [bool]$s3.clean_reactivation_proof) {
        return 'blocked'
    }
    return 'pass'
}

function New-BlockedScenarios {
    param([Parameter(Mandatory)][string]$Reason)
    $items = foreach ($number in 1..7) {
        $entry = [ordered]@{ sequence_index = $number; scenario = $number; result = 'blocked'; reason = $Reason }
        if ($number -eq 2) {
            $entry['assertions'] = [ordered]@{ allow = 'blocked'; vpn_on_create = 'blocked'; vpn_connection_create_fd = 'blocked' }
        }
        $entry
    }
    return @($items)
}

function Assert-CampaignCaptureHealthy {
    param(
        [Parameter(Mandatory)]$Capture,
        [AllowNull()][int]$Scenario = $null,
        [AllowNull()][string]$Origin = $null
    )
    # ADJ-20260808-0002 (C6): a continuously degraded CampaignCapture (raw-hilog) is NEVER a
    # scenario-invalid input on any path (Invoke-Capture, Wait-MachineCondition, S5 direct
    # capture, post-observation). Both infrastructure and non-infrastructure continuous
    # degradation throw blocked with the recorded raw-hilog category; only the infrastructure
    # category authorizes the USB retry. No-op when the capture is healthy.
    if ($null -eq $Capture -or -not [bool]$Capture.Degraded) { return }
    $scenarioNumber = if ($null -eq $Scenario) { 0 } else { [int]$Scenario }
    $continuousInfra = Get-CaptureDegradedInfra $scenarioNumber
    if ($true -eq $continuousInfra) {
        $infraEntry = @($script:CaptureDegraded | Where-Object { [string]$_.category -eq 'infrastructure' } | Select-Object -First 1)
        $detail = Protect-SensitiveText ([string](Get-OptionalProperty $infraEntry 'reason' 'capture process degraded'))
        $script:InfrastructureReasonObserved = 'hdc-usb-interruption'
        throw "scenario-$scenarioNumber continuous capture infrastructure failure: $detail"
    }
    $entry = @($script:CaptureDegraded | Where-Object { [string]$_.component -match 'raw-hilog' } | Select-Object -First 1)
    $detail = Protect-SensitiveText ([string](Get-OptionalProperty $entry 'reason' 'continuous capture degraded'))
    throw "scenario-$scenarioNumber continuous capture non-infrastructure blocked: $detail"
}

function Assert-ScenarioCaptureCanContinue {
    param([Parameter(Mandatory)]$Results, [Parameter(Mandatory)]$Observation)
    $script:PartialScenarios = @($Results)
    if (-not $Observation.CaptureDegraded) { return }
    $scenarioNumber = [int]$Observation.Observation.scenario
    # ADJ-20260808-0002 (C6): a scenario whose observation window was shortened by continuous
    # raw-hilog degradation is a runner blocked (infra or non-infra), never a scenario invalid.
    # Delegates to the unified classifier used by Invoke-Capture / Wait-MachineCondition.
    Assert-CampaignCaptureHealthy $script:CampaignCapture $scenarioNumber -Origin 'Assert-ScenarioCaptureCanContinue'
}

function Invoke-Capture {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Scenario, [switch]$ObservationOnly, [switch]$Replace)
    # ADJ-20260808-0003: ScreenCap/DumpLayout/Receive exit 124/125 / timeout / HDC transport
    # failures are infrastructure, not scenario evidence loss. They must propagate the
    # infrastructure category/blocked path (global CaptureDegraded + infrastructure_reason +
    # authorized retry) and must never be turned into a ScenarioInvalid verdict.
    # ADJ-20260808-0002 (C6): `-Replace` drops any previous same-name CaptureArtifacts record so
    # a bounded same-name layout resample never leaves the collection manifest pointing at an
    # overwritten capture file.
    $script:LastCaptureInfrastructure = $false
    $operations = @('ScreenCap', 'DumpLayout', 'ReceiveScreen', 'ReceiveLayout')
    $failures = [Collections.Generic.List[string]]::new()
    $infrastructureFailures = [Collections.Generic.List[string]]::new()
    if ($null -ne $script:CampaignCapture -and $script:CampaignCapture.Degraded) {
        # ADJ-20260808-0002 (C6): a continuously degraded raw-hilog stream is NEVER a scenario
        # invalid input on any capture path. Throw the classified blocked (infra or non-infra)
        # immediately so Invoke-LayoutCheckpoint / Invoke-MechanicalStep / S5 direct capture
        # cannot convert it into ScenarioInvalid. Infrastructure continuous degradation authorizes
        # the USB retry; non-infrastructure stays a plain blocked.
        Assert-CampaignCaptureHealthy $script:CampaignCapture $Scenario -Origin 'Invoke-Capture'
    } else {
        foreach ($operation in $operations) {
            $result = Invoke-HdcOperation $operation @{ Name = $Name } -AllowFailure
            if ($result.ExitCode -ne 0) { $failures.Add("$operation-exit-$($result.ExitCode)") }
            if ($result.ExitCode -in @(124, 125) -or [string]$result.Stderr -match '(?i)\btimeout\b|\boffline\b|\bUSB\b|\bdisconnect(?:ed)?\b|transport (?:offline|error|fail)|HDC Process\.Start') {
                $infrastructureFailures.Add("$operation-exit-$($result.ExitCode)")
            }
        }
    }
    $screenPath = Join-Path $script:RawPath "capture-$Name.png"
    $layoutPath = Join-Path $script:RawPath "capture-$Name.json"
    if (-not $DryRun) {
        foreach ($path in @($screenPath, $layoutPath)) {
            if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or (Get-Item -LiteralPath $path).Length -eq 0) { $failures.Add("missing-or-empty:$([IO.Path]::GetFileName($path))") }
        }
    }
    $status = if ($failures.Count -eq 0) { 'collected' } else { 'degraded' }
    $artifact = [ordered]@{ scenario = $Scenario; name = $Name; status = $status; failures = @($failures); screen_path = $screenPath; layout_path = $layoutPath }
    if ($Replace) {
        $stale = @($script:CaptureArtifacts | Where-Object { [string]$_.name -eq $Name })
        foreach ($old in $stale) { [void]$script:CaptureArtifacts.Remove($old) }
    }
    $script:CaptureArtifacts.Add($artifact)
    if ($status -eq 'degraded') {
        $isInfrastructure = $infrastructureFailures.Count -gt 0
        $script:LastCaptureInfrastructure = $isInfrastructure
        if ($ObservationOnly) {
            # Observation-only captures never enter the global CaptureDegraded list: their loss
            # is an independent diagnostic and must not block the scenario or overall. An
            # infrastructure failure inside an observation-only capture still records its
            # category so the reviewer sees the transport reason without blocking anything.
            $script:ObservationOnlyDegraded.Add([ordered]@{ scenario = $Scenario; name = $Name; status = 'degraded'; category = $(if ($isInfrastructure) { 'infrastructure' } else { 'non-infrastructure' }); infrastructure_reason = $(if ($isInfrastructure) { 'hdc-usb-interruption' } else { $null }); failures = @($failures); screen_path = $screenPath; layout_path = $layoutPath })
        } else {
            if ($isInfrastructure) {
                $script:InfrastructureReasonObserved = 'hdc-usb-interruption'
                Add-CaptureDegradation $script:CampaignCapture 'screen-layout-capture' ($failures -join ',') -Scenario $Scenario -Category 'infrastructure' -InfrastructureReason 'hdc-usb-interruption' -MarkContinuousDegraded $false
            } else {
                # Non-infrastructure screen/layout evidence loss; do not mark continuous Capture.Degraded.
                Add-CaptureDegradation $script:CampaignCapture 'screen-layout-capture' ($failures -join ',') -Scenario $Scenario -Category 'non-infrastructure' -MarkContinuousDegraded $false
            }
        }
    }
    return $status
}

function Get-LayoutFacts {
    param([Parameter(Mandatory)]$Value)
    $facts = [Collections.Generic.List[string]]::new()
    function Visit-LayoutValue {
        param($Current, [string]$Path)
        if ($null -eq $Current) { return }
        if ($Current -is [string] -or $Current -is [ValueType]) {
            $facts.Add("$Path=$([string]$Current)")
            return
        }
        if ($Current -is [Collections.IDictionary]) {
            foreach ($key in $Current.Keys) { Visit-LayoutValue $Current[$key] "$Path.$([string]$key)" }
            return
        }
        if ($Current -is [pscustomobject]) {
            foreach ($property in $Current.PSObject.Properties) { Visit-LayoutValue $property.Value "$Path.$($property.Name)" }
            return
        }
        if ($Current -is [Collections.IEnumerable]) {
            $index = 0
            foreach ($item in $Current) { Visit-LayoutValue $item "$Path[$index]"; $index++ }
        }
    }
    Visit-LayoutValue $Value '$'
    return @($facts)
}

function Test-CapturedLayoutProfile {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet('entry', 'authorization', 'authorization-dismissed', 'settings-vpn', 'settings-app-info')][string]$Profile,
        [AllowNull()][string]$ExpectedBundle = $null
    )
    $artifact = @($script:CaptureArtifacts | Where-Object { [string]$_.name -eq $Name } | Select-Object -Last 1)
    if ($artifact.Count -ne 1 -or [string]$artifact[0].status -ne 'collected') {
        return [pscustomobject]@{ status = 'unverifiable'; reason = 'capture-not-collected'; profile = $Profile; matched = @() }
    }
    $layout = $null
    try { $layout = Get-Content -LiteralPath ([string]$artifact[0].layout_path) -Raw | ConvertFrom-Json -Depth 80 } catch {
        return [pscustomobject]@{ status = 'unverifiable'; reason = 'layout-json-invalid'; profile = $Profile; matched = @() }
    }
    $facts = @(Get-LayoutFacts $layout)
    $joined = ($facts -join "`n").ToLowerInvariant()
    $checks = [ordered]@{}
    # ADJ-20260808-0003 (C6): real layout facts are a top-level array with `attributes` +
    # `children` on every node (`$[n][.children[m]...].attributes.<field>=<value>`). A single-
    # element top-level array round-trips through JSON to a bare root (`$.attributes.<field>` /
    # `$.children[m].attributes.<field>`), which this generic matcher also tolerates. It never
    # depends on any self-made `window`/`resourceId` structure. `attributes.id`/`key` carry
    # stable control ids; `attributes.text` carries visible text; `attributes.bundleName`/`type`
    # carry the window owner/type on whichever node actually owns them.
    $attrField = '[^=\n]*?\.attributes\.'
    $textNode = $attrField + 'text='
    function Get-AttrValuePattern([string]$Field, [string]$Value) {
        return '(?:^|\n)' + $attrField + $Field + '=' + [regex]::Escape($Value) + '(?=\n|$)'
    }
    # ExpectedBundle is structurally required only by the entry profile. Authorization ownership
    # remains event-bound; settings-app-info uses ExpectedBundle only to select the expected A/B
    # display label inside Setting.AppDetail, while final A correctness remains process-effect-bound.
    if (-not [string]::IsNullOrWhiteSpace($ExpectedBundle) -and $Profile -eq 'entry') { $checks['expected-bundle'] = $joined -match (Get-AttrValuePattern 'bundlename' $ExpectedBundle.ToLowerInvariant()) }
    switch ($Profile) {
        # ADJ-20260808-0003 (C6): entry profile keeps ExpectedBundle + button id/key start-vpn /
        # stop-vpn. The real EMU entry layout carries id/key `start-vpn`/`stop-vpn` on Button
        # nodes; fullscreen text alone is not sufficient.
        'entry' {
            $checks['start-control'] = $joined -match ($attrField + '(?:id|key)=start-vpn(?=\n|$)')
            $checks['stop-control'] = $joined -match ($attrField + '(?:id|key)=stop-vpn(?=\n|$)')
        }
        # ADJ-20260808-0003 (C6): calibrated against the real attributes/children authorization
        # layout facts (read-only): any node owns bundleName=com.huawei.hmos.vpndialog and any
        # node owns type=Dialog (API26 puts the Dialog on a child), plus a dialog text that
        # contains both 允许 and VPN (whitespace/app-name tolerant, never the exact app label),
        # plus Allow/允许 and Cancel/Deny/取消/拒绝 controls. The request bundle is never required
        # here: request ownership is proven by the UI_START/event gate, not by the page.
        'authorization' {
            $checks['dialog-owner'] = $joined -match (Get-AttrValuePattern 'bundlename' 'com.huawei.hmos.vpndialog')
            $checks['dialog-type'] = $joined -match (Get-AttrValuePattern 'type' 'dialog')
            $checks['dialog-text'] = ($joined -match ($textNode + '[^\n]*允许[^\n]*vpn[^\n]*(?=\n|$)')) -or ($joined -match ($textNode + '[^\n]*vpn[^\n]*允许[^\n]*(?=\n|$)'))
            $checks['allow-control'] = ($joined -match ($textNode + '[^\n]*\ballow\b[^\n]*(?=\n|$)')) -or ($joined -match ($textNode + '[^\n]*允许[^\n]*(?=\n|$)'))
            $checks['cancel-control'] = ($joined -match ($textNode + '[^\n]*(?:cancel|deny)[^\n]*(?=\n|$)')) -or ($joined -match ($textNode + '[^\n]*取消[^\n]*(?=\n|$)')) -or ($joined -match ($textNode + '[^\n]*拒绝[^\n]*(?=\n|$)')) -or ($joined -match ($textNode + '[^\n]*不允许[^\n]*(?=\n|$)'))
        }
        'authorization-dismissed' {
            $checks['authorization-controls-absent'] = $joined -notmatch ($textNode + '[^\n]*(?:允许|取消|拒绝|不允许)[^\n]*(?=\n|$)')
            $checks['entry-start-control'] = $joined -match ($attrField + '(?:id|key)=start-vpn(?=\n|$)')
        }
        # ADJ-20260808-0003 (C6): settings owner matches any node's attributes.bundleName=
        # com.huawei.hmos.settings; the VPN group resource matches attributes.id/key =
        # Setting.MobileNetwork.vpn_group_group.vpn_settings. The page is no longer decisive
        # (Settings>VPN is observation-only), but plain text "VPN" alone must never match.
        'settings-vpn' {
            $checks['settings-owner'] = $joined -match (Get-AttrValuePattern 'bundlename' 'com.huawei.hmos.settings')
            $checks['vpn-group-resource'] = $joined -match ($attrField + '(?:id|key)=setting\.mobilenetwork\.vpn_group_group\.vpn_settings(?=\n|$)')
            $checks['vpn-page-text'] = $joined -match ($textNode + '[^\n]*vpn[^\n]*(?=\n|$)') -and $joined -match ($textNode + '[^\n]*没有 vpn[^\n]*(?=\n|$)')
            $checks['add-vpn-button'] = $joined -match ($textNode + '[^\n]*添加 vpn 网络[^\n]*(?=\n|$)')
        }
        # The production S5 sample contains app-list and sceneboard siblings in the same dump.
        # Match the label and force-stop control only inside one current visible Setting.AppDetail
        # NavDestination beneath the Settings owner; sibling/root facts cannot satisfy either field.
        'settings-app-info' {
            $factMap = @{}
            foreach ($fact in $facts) {
                $separator = ([string]$fact).IndexOf('=')
                if ($separator -lt 0) { continue }
                $path = ([string]$fact).Substring(0, $separator).ToLowerInvariant()
                $factMap[$path] = ([string]$fact).Substring($separator + 1)
            }
            $settingsBases = @()
            foreach ($path in @($factMap.Keys)) {
                if ($path.EndsWith('.attributes.bundlename', [StringComparison]::Ordinal) -and ([string]$factMap[$path]).ToLowerInvariant() -eq 'com.huawei.hmos.settings') {
                    $base = $path -replace '\.attributes\.bundlename$', ''
                    if ($settingsBases -notcontains $base) { $settingsBases += $base }
                }
            }
            $checks['settings-owner'] = $settingsBases.Count -gt 0
            $hiddenBases = @()
            foreach ($path in @($factMap.Keys)) {
                if ($path.EndsWith('.attributes.visible', [StringComparison]::Ordinal) -and ([string]$factMap[$path]).ToLowerInvariant() -eq 'false') {
                    $base = $path -replace '\.attributes\.visible$', ''
                    if ($hiddenBases -notcontains $base) { $hiddenBases += $base }
                }
            }
            $detailBases = @()
            foreach ($path in @($factMap.Keys)) {
                if ($path -notmatch '\.attributes\.(?:id|key)$' -or ([string]$factMap[$path]).ToLowerInvariant() -ne 'setting.appdetail') { continue }
                $base = $path -replace '\.attributes\.(?:id|key)$', ''
                if (@($settingsBases | Where-Object { $base.StartsWith($_ + '.', [StringComparison]::Ordinal) }).Count -eq 0) { continue }
                if (([string](Get-OptionalProperty $factMap ($base + '.attributes.visible') '')).ToLowerInvariant() -ne 'true') { continue }
                if (([string](Get-OptionalProperty $factMap ($base + '.attributes.type') '')).ToLowerInvariant() -ne 'navdestination') { continue }
                if (@($hiddenBases | Where-Object { $base -eq $_ -or $base.StartsWith($_ + '.', [StringComparison]::Ordinal) }).Count -gt 0) { continue }
                if ($detailBases -notcontains $base) { $detailBases += $base }
            }
            $acceptedLabels = if ([string]$ExpectedBundle -eq $script:BundleA) { @('e3 preflight a') }
                elseif ([string]$ExpectedBundle -eq $script:BundleB) { @('e3 preflight b') }
                else { @() }
            $labelFound = $false
            $forceStopFound = $false
            foreach ($detailBase in $detailBases) {
                $detailHasLabel = $false
                $detailHasForceStop = $false
                foreach ($path in @($factMap.Keys)) {
                    if (-not $path.StartsWith($detailBase + '.', [StringComparison]::Ordinal)) { continue }
                    $nodeBase = $path -replace '\.attributes\.[^.]+$', ''
                    if (@($hiddenBases | Where-Object { $nodeBase -eq $_ -or $nodeBase.StartsWith($_ + '.', [StringComparison]::Ordinal) }).Count -gt 0) { continue }
                    $normalized = [regex]::Replace(([string]$factMap[$path]).Trim().ToLowerInvariant(), '\s+', ' ')
                    if ($path.EndsWith('.attributes.text', [StringComparison]::Ordinal)) {
                        if ($acceptedLabels -contains $normalized) { $detailHasLabel = $true }
                        if ($normalized -in @('强行停止', '强制停止', 'force stop')) { $detailHasForceStop = $true }
                    } elseif ($path.EndsWith('.attributes.id', [StringComparison]::Ordinal) -or $path.EndsWith('.attributes.key', [StringComparison]::Ordinal)) {
                        if ($normalized -match 'force[_-]?stop') { $detailHasForceStop = $true }
                    }
                }
                $labelFound = $labelFound -or $detailHasLabel
                $forceStopFound = $forceStopFound -or ($detailHasLabel -and $detailHasForceStop)
            }
            $checks['app-detail-structure'] = $detailBases.Count -eq 1
            $checks['app-label'] = $labelFound
            $checks['force-stop-control'] = $forceStopFound
        }
    }
    $failed = @($checks.Keys | Where-Object { -not [bool]$checks[$_] })
    return [pscustomobject]@{
        status = $(if ($failed.Count -eq 0) { 'pass' } else { 'mismatch' })
        reason = $(if ($failed.Count -eq 0) { 'deterministic-layout-match' } else { 'layout-fields-missing:' + ($failed -join ',') })
        profile = $Profile
        matched = @($checks.Keys | Where-Object { [bool]$checks[$_] })
        required = @($checks.Keys)
    }
}

function Invoke-LayoutCheckpoint {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet('entry', 'authorization', 'authorization-dismissed', 'settings-vpn', 'settings-app-info')][string]$Profile,
        [AllowNull()][string]$ExpectedBundle = $null,
        [AllowNull()][int]$StepIndex = $null,
        [AllowNull()][string]$StepId = $null,
        [AllowNull()][string]$ExpectedAction = $null,
        [switch]$ObservationOnly
    )
    # Machine-only deterministic layout gate. The operator never judges the screen: wrong/unexpected
    # UI is protocol invalid; uncollected capture is also invalid at a decisive gate. Platform "no
    # authorization UI at all" remains a documented blocked path only when a scenario explicitly
    # classifies that outcome outside this checkpoint (this gate itself never asks the operator).
    $captureStatus = Invoke-Capture $Name $Scenario -ObservationOnly:$ObservationOnly
    if ($captureStatus -ne 'collected') {
        $checkpoint = [ordered]@{ status = 'unverifiable'; name = $Name; capture_status = $captureStatus; profile = $Profile; matching = $false; note = 'screenshot-or-layout-not-collected'; reason = 'capture-not-collected' }
        Add-TranscriptRecord 'machine-layout-checkpoint' ([ordered]@{ scenario = $Scenario; checkpoint = $checkpoint })
        # ADJ-20260808-0003: an infrastructure capture failure (exit 124/125 / timeout / HDC
        # transport) propagates as infrastructure blocked with retry authorization, never as a
        # scenario invalid; only non-infrastructure capture loss invalidates here.
        if ($script:LastCaptureInfrastructure) {
            throw "HDC infrastructure interruption layout-checkpoint=$Name scenario=$Scenario"
        }
        Throw-ScenarioInvalid -Scenario $Scenario -Reason "layout-checkpoint-$Name-capture-not-collected" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -MachinePostcondition $checkpoint -CaptureAfter ([ordered]@{ status = $captureStatus; name = $Name })
    }
    $assessment = Test-CapturedLayoutProfile $Name $Profile $ExpectedBundle
    # ADJ-20260808-0002 (C6): bounded same-name resample for a collected-but-mismatched layout.
    # A fast operator can act before the platform renders the expected UI (e.g. the authorization
    # dialog appears a few seconds later), so a mismatch on a collected capture is re-captured
    # under the SAME name at ~1s intervals for at most 8 seconds total and re-evaluated. Every
    # retry REPLACES the previous same-name CaptureArtifacts record so the collection manifest
    # never points at an overwritten file; transcript records each attempt and the missing
    # fields. Infrastructure / continuous capture degradation aborts immediately with blocked
    # (never ScenarioInvalid). A final mismatch / unverifiable stays ScenarioInvalid; the
    # operator is never re-asked to confirm the screen.
    $attempts = 0
    $resampleDeadline = (Get-Now).AddSeconds(8)
    while ([string]$assessment.status -eq 'mismatch' -and (Get-Now) -lt $resampleDeadline) {
        $attempts++
        Wait-Until (Get-Now).AddSeconds(1)
        $retryStatus = Invoke-Capture $Name $Scenario -ObservationOnly:$ObservationOnly -Replace
        if ($retryStatus -ne 'collected') {
            # Infrastructure / continuous degradation aborts the resample immediately with blocked.
            if ($script:LastCaptureInfrastructure) {
                throw "HDC infrastructure interruption layout-checkpoint=$Name scenario=$Scenario (resample attempt $attempts)"
            }
            # A non-infrastructure capture loss during resample keeps the last collected mismatch
            # assessment and falls through to the final mismatch handling below.
            break
        }
        $assessment = Test-CapturedLayoutProfile $Name $Profile $ExpectedBundle
        Add-TranscriptRecord 'machine-layout-resample' ([ordered]@{
            scenario = $Scenario
            name = $Name
            profile = $Profile
            attempt = $attempts
            matching = ([string]$assessment.status -eq 'pass')
            reason = [string]$assessment.reason
            missing = @($assessment.required | Where-Object { $_ -notin @($assessment.matched) })
        })
    }
    $matching = [string]$assessment.status -eq 'pass'
    $checkpoint = [ordered]@{
        status = [string]$assessment.status
        name = $Name
        capture_status = $captureStatus
        profile = $Profile
        expected_bundle = $ExpectedBundle
        matching = [bool]$matching
        reason = [string]$assessment.reason
        matched = @($assessment.matched)
        required = @($assessment.required)
        attempts = $attempts
        note = 'machine-deterministic-layout-v1'
    }
    Add-TranscriptRecord 'machine-layout-checkpoint' ([ordered]@{ scenario = $Scenario; checkpoint = $checkpoint })
    if (-not $matching) {
        $suffix = if ([string]$assessment.status -eq 'unverifiable') { 'layout-unverifiable' } else { 'layout-mismatch' }
        Throw-ScenarioInvalid -Scenario $Scenario -Reason "layout-checkpoint-$Name-$suffix" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -MachinePostcondition $checkpoint -CaptureAfter ([ordered]@{ status = $captureStatus; name = $Name })
    }
    return $checkpoint
}

function Invoke-LayoutChoiceCheckpoint {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][string]$Name,
        [AllowNull()][string]$ExpectedBundle = $null,
        [AllowNull()][int]$StepIndex = $null,
        [AllowNull()][string]$StepId = $null,
        [AllowNull()][string]$ExpectedAction = $null,
        [switch]$ObservationOnly
    )
    # ADJ-20260808-0003: dual-profile machine layout gate for optional reauthorization (S5/S6 A).
    # Capture once under $Name, then classify as entry OR authorization. A fast operator can act
    # before the platform renders either UI, so a dual-mismatch on a collected capture is re-
    # captured under the SAME name (-Replace) at ~1s intervals for at most 8 seconds total and
    # re-evaluated against both profiles. Any profile pass returns selected_profile + attempts.
    # Infrastructure / continuous capture degradation aborts immediately as blocked (never
    # ScenarioInvalid). A final dual mismatch stays ScenarioInvalid; the operator is never re-
    # asked to confirm the screen.
    $captureStatus = Invoke-Capture $Name $Scenario -ObservationOnly:$ObservationOnly
    if ($captureStatus -ne 'collected') {
        $checkpoint = [ordered]@{ status = 'unverifiable'; name = $Name; capture_status = $captureStatus; selected_profile = $null; matching = $false; note = 'screenshot-or-layout-not-collected'; reason = 'capture-not-collected'; attempts = 0 }
        Add-TranscriptRecord 'machine-layout-choice-checkpoint' ([ordered]@{ scenario = $Scenario; checkpoint = $checkpoint })
        if ($script:LastCaptureInfrastructure) {
            throw "HDC infrastructure interruption layout-choice-checkpoint=$Name scenario=$Scenario"
        }
        Throw-ScenarioInvalid -Scenario $Scenario -Reason "layout-choice-checkpoint-$Name-capture-not-collected" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -MachinePostcondition $checkpoint -CaptureAfter ([ordered]@{ status = $captureStatus; name = $Name })
    }
    $entryAssessment = Test-CapturedLayoutProfile $Name 'entry' $ExpectedBundle
    $authAssessment = Test-CapturedLayoutProfile $Name 'authorization' $ExpectedBundle
    $selectedProfile = $null
    if ([string]$authAssessment.status -eq 'pass') { $selectedProfile = 'authorization' }
    elseif ([string]$entryAssessment.status -eq 'pass') { $selectedProfile = 'entry' }
    $attempts = 0
    $resampleDeadline = (Get-Now).AddSeconds(8)
    while ($null -eq $selectedProfile -and (Get-Now) -lt $resampleDeadline) {
        $attempts++
        Wait-Until (Get-Now).AddSeconds(1)
        $retryStatus = Invoke-Capture $Name $Scenario -ObservationOnly:$ObservationOnly -Replace
        if ($retryStatus -ne 'collected') {
            if ($script:LastCaptureInfrastructure) {
                throw "HDC infrastructure interruption layout-choice-checkpoint=$Name scenario=$Scenario (resample attempt $attempts)"
            }
            break
        }
        $entryAssessment = Test-CapturedLayoutProfile $Name 'entry' $ExpectedBundle
        $authAssessment = Test-CapturedLayoutProfile $Name 'authorization' $ExpectedBundle
        if ([string]$authAssessment.status -eq 'pass') { $selectedProfile = 'authorization' }
        elseif ([string]$entryAssessment.status -eq 'pass') { $selectedProfile = 'entry' }
        Add-TranscriptRecord 'machine-layout-choice-resample' ([ordered]@{
            scenario = $Scenario
            name = $Name
            attempt = $attempts
            matching = ($null -ne $selectedProfile)
            selected_profile = $selectedProfile
            entry_reason = [string]$entryAssessment.reason
            authorization_reason = [string]$authAssessment.reason
        })
    }
    $matching = $null -ne $selectedProfile
    $checkpoint = [ordered]@{
        status = $(if ($matching) { 'pass' } else { 'mismatch' })
        name = $Name
        capture_status = $captureStatus
        selected_profile = $selectedProfile
        expected_bundle = $ExpectedBundle
        matching = [bool]$matching
        reason = $(if ($matching) { "layout-choice-$selectedProfile" } else { "entry:$([string]$entryAssessment.reason);authorization:$([string]$authAssessment.reason)" })
        attempts = $attempts
        note = 'machine-deterministic-layout-choice-v1'
    }
    Add-TranscriptRecord 'machine-layout-choice-checkpoint' ([ordered]@{ scenario = $Scenario; checkpoint = $checkpoint })
    if (-not $matching) {
        Throw-ScenarioInvalid -Scenario $Scenario -Reason "layout-choice-checkpoint-$Name-layout-mismatch" -StepIndex $StepIndex -StepId $StepId -ExpectedAction $ExpectedAction -MachinePostcondition $checkpoint -CaptureAfter ([ordered]@{ status = $captureStatus; name = $Name })
    }
    return $checkpoint
}

function Invoke-ReviewOnlyCapture {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Scenario)
    $captureStatus = Invoke-Capture $Name $Scenario -ObservationOnly
    $checkpoint = [ordered]@{ status = 'review-only'; name = $Name; capture_status = $captureStatus; matching = $null; note = 'final evidence only; not used as a semantic verdict input' }
    Add-TranscriptRecord 'review-only-layout-artifact' ([ordered]@{ scenario = $Scenario; checkpoint = $checkpoint })
    return $captureStatus
}

function Get-ExactProcessCheckpoint {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$ExpectedActiveBundles,
        [AllowEmptyCollection()][string[]]$ObservedBundles = @()
    )
    # ADJ-20260808-0003: every non-pass process checkpoint is status=blocked (never invalid).
    # HDC transport/timeout/offline on PidOf/BundleDump records hdc-usb-interruption and sets the
    # global InfrastructureReasonObserved. Present/absent mismatch or unknown probe state is also
    # blocked — process residue is platform/infra uncertainty, not an operator protocol error.
    $states = [Collections.Generic.List[object]]::new()
    $valid = $true
    $reason = $null
    $infraHit = $false
    foreach ($bundle in @($script:BundleA, $script:BundleB)) {
        $pidResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
        $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
        $dump = Get-BundleDumpAssessment $dumpResult $bundle
        $pidIsInfra = Test-FaultInfrastructureFailure $pidResult
        $dumpIsInfra = (Test-FaultInfrastructureFailure $dumpResult) -or ([string]$dump.Status -eq 'infrastructure')
        $pidKnown = [int]$pidResult.ExitCode -in @(0, 1) -and [string]::IsNullOrWhiteSpace([string]$pidResult.Stderr)
        $present = $pidKnown -and -not [string]::IsNullOrWhiteSpace([string]$pidResult.Stdout)
        $expectedPresent = $bundle -in $ExpectedActiveBundles
        # ADJ-20260808-0003: observed-only bundles (e.g. B after a rejected create in S6) are
        # recorded but never required absent/present; their state cannot gate the checkpoint.
        $observedOnly = $bundle -in $ObservedBundles
        if ($observedOnly) {
            $states.Add([ordered]@{ bundle = $bundle; bundle_present = ($dump.Status -eq 'pass'); process_target = "${bundle}:vpn"; process_present = [bool]$present; expected_present = $null; observed_only = $true })
            continue
        }
        if ($pidIsInfra -or $dumpIsInfra) {
            $valid = $false
            $infraHit = $true
            if ($null -eq $reason) { $reason = "hdc-usb-interruption:process-check:$bundle" }
        } elseif ($dump.Status -ne 'pass') {
            $valid = $false
            if ($null -eq $reason) { $reason = "bundle-check-unavailable:$bundle" }
        } elseif (-not $pidKnown) {
            $valid = $false
            if ($null -eq $reason) { $reason = "process-check-unavailable:$bundle" }
        } elseif ($present -ne $expectedPresent) {
            $valid = $false
            if ($null -eq $reason) { $reason = "process-state-mismatch:$bundle expected-active=$expectedPresent actual-active=$present" }
        }
        $states.Add([ordered]@{ bundle = $bundle; bundle_present = ($dump.Status -eq 'pass'); process_target = "${bundle}:vpn"; process_present = [bool]$present; expected_present = [bool]$expectedPresent; observed_only = $false })
    }
    if ($infraHit) { $script:InfrastructureReasonObserved = 'hdc-usb-interruption' }
    return [ordered]@{ status = $(if ($valid) { 'pass' } else { 'blocked' }); reason = $(if ($valid) { 'exact-process-checkpoint' } else { $reason }); states = @($states) }
}

function Get-ProcessTargetCheckpoint {
    param([Parameter(Mandatory)][string]$Bundle)
    # ADJ-20260808-0003: after a machine-verified CREATE_ACCEPTED, a precise `pidof <bundle>:vpn`
    # present checkpoint proves the naming tuple actually resolves to a live Extension process.
    # Absent / unknown / error means the naming is unverified: blocked with an explicit
    # process-target-unverified reason, never pass. S3/S5/S7 consume this verified checkpoint.
    $pidResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $Bundle } -AllowFailure
    $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $Bundle } -AllowFailure
    $assessment = Get-ProcessProbeStatus $pidResult $dumpResult $Bundle
    $verified = [string]$assessment.status -eq 'present'
    $reason = if ($verified) { 'process-target-present' } elseif ([string]$assessment.status -eq 'absent') { 'process-target-unverified:absent' } else { 'process-target-unverified:' + [string]$assessment.detail }
    return [ordered]@{
        status = $(if ($verified) { 'pass' } else { 'blocked' })
        reason = $reason
        process_target = "${Bundle}:vpn"
        probe = [ordered]@{ pid_status = [string]$assessment.status; detail = $assessment.detail; bundle_present = [bool]$assessment.bundle_present }
    }
}

function Test-FaultInfrastructureFailure {
    param([Parameter(Mandatory)]$Result)
    if ([int]$Result.ExitCode -in @(124, 125)) { return $true }
    $text = Get-HdcCombinedText $Result
    return [bool]($text -match '(?i)\boffline\b|\bUSB\b|\bdisconnect(?:ed)?\b|transport (?:offline|error)|HDC Process\.Start|\btimeout\b')
}

function Invoke-FaultArtifact {
    param([Parameter(Mandatory)][ValidateSet('FaultA', 'FaultB')][string]$Operation, [Parameter(Mandatory)][int]$Scenario)
    $suffix = if ($Operation -eq 'FaultA') { 'a' } else { 'b' }
    $path = Join-Path $script:RawPath "fault-scenario-$Scenario-$suffix.txt"
    $status = 'collected'
    $failures = [Collections.Generic.List[string]]::new()
    $result = Invoke-HdcOperation $Operation -AllowFailure
    $faultIsInfra = Test-FaultInfrastructureFailure $result
    if ($result.ExitCode -ne 0) { $status = 'degraded'; $failures.Add("$Operation-exit-$($result.ExitCode)") }
    try {
        $content = [string]$result.Stdout
        if (-not [string]::IsNullOrWhiteSpace([string]$result.Stderr)) { $content += [Environment]::NewLine + [string]$result.Stderr }
        [IO.File]::WriteAllText($path, $content, [Text.UTF8Encoding]::new($false))
        $script:FaultArtifacts.Add([ordered]@{
            scenario = $Scenario
            operation = $Operation
            reference = "RAW-FAULT-$($suffix.ToUpperInvariant())-SCENARIO-$Scenario"
            status = $status
            path = $path
            sha256 = Get-FileSha256 $path
            bytes = (Get-Item -LiteralPath $path).Length
            failures = @($failures)
        })
    } catch {
        $status = 'degraded'
        $failures.Add((Protect-SensitiveText $_.Exception.Message))
    }
    if ($status -ne 'collected') {
        # Fault artifact loss records CaptureDegraded for scenario-7 only. It must not mark continuous
        # Capture.Degraded or shorten the 60-second observation window.
        $category = if ($faultIsInfra) { 'infrastructure' } else { 'non-infrastructure' }
        $infraReason = if ($faultIsInfra) { 'hdc-usb-interruption' } else { $null }
        if ($faultIsInfra) { $script:InfrastructureReasonObserved = 'hdc-usb-interruption' }
        Add-CaptureDegradation $script:CampaignCapture $Operation ('targeted fault artifact unavailable: ' + ($failures -join ',')) -Scenario $Scenario -Category $category -InfrastructureReason $infraReason -MarkContinuousDegraded $false
    }
    return $status
}

function Invoke-DryRunCampaign {
    $planned = @(
        @{ operation = 'Version'; parameters = @{} }, @{ operation = 'TupleModel'; parameters = @{} }, @{ operation = 'TupleBuild'; parameters = @{} },
        @{ operation = 'BundleDump'; parameters = @{ Bundle = $script:BundleA } }, @{ operation = 'BundleDump'; parameters = @{ Bundle = $script:BundleB } },
        @{ operation = 'PidOf'; parameters = @{ Bundle = $script:BundleA } }, @{ operation = 'PidOf'; parameters = @{ Bundle = $script:BundleB } },
        @{ operation = 'MkdirStaging'; parameters = @{} }, @{ operation = 'SendA'; parameters = @{} }, @{ operation = 'SendB'; parameters = @{} },
        @{ operation = 'InstallA'; parameters = @{} }, @{ operation = 'InstallB'; parameters = @{} },
        @{ operation = 'StartEntry'; parameters = @{ Bundle = $script:BundleA } }, @{ operation = 'StartEntry'; parameters = @{ Bundle = $script:BundleB } },
        @{ operation = 'FaultA'; parameters = @{} }, @{ operation = 'FaultB'; parameters = @{} },
        @{ operation = 'Uninstall'; parameters = @{ Bundle = $script:BundleB } }, @{ operation = 'Uninstall'; parameters = @{ Bundle = $script:BundleA } },
        @{ operation = 'RemoveStaging'; parameters = @{} }, @{ operation = 'StagingProbe'; parameters = @{} }
    )
    foreach ($item in $planned) { [void](Invoke-HdcOperation $item.operation $item.parameters) }
    return New-BlockedScenarios 'dry-run-no-device-non-evidence'
}

function Invoke-StrongLiveCampaign {
    param([Parameter(Mandatory)]$Freeze)
    $results = [Collections.Generic.List[object]]::new()
    $script:CampaignPhase = 'preflight'
    $versionResult = Invoke-HdcOperation 'Version'
    if ($versionResult.Stdout.Trim() -ne [string]$Freeze.hdc.version) { throw 'preflight: frozen HDC version mismatch' }
    $modelResult = Invoke-HdcOperation 'TupleModel'
    $buildResult = Invoke-HdcOperation 'TupleBuild'
    if ($modelResult.Stdout.Trim() -ne $Freeze.target_tuple.device_model -or $buildResult.Stdout.Trim() -ne $Freeze.target_tuple.full_system_build) {
        throw 'preflight: model/build precheck drifted before continuous capture'
    }
    $campaignCapture = Start-CampaignHilogCapture
    Initialize-CampaignCaptureAnchor $campaignCapture
    if ($campaignCapture.Degraded) { throw 'collection preparation blocked: continuous capture unavailable before scenario-1 installation' }

    # S1 is fully machine-operated. The operator is not asked to attest an installation fact.
    $context1 = New-ScenarioContext 1
    $firstBaselineQueryAt = Get-Now
    foreach ($bundle in @($script:BundleA, $script:BundleB)) {
        $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
        if ((Get-HdcCombinedText $dumpResult) -notmatch 'failed to get information|not exist|not found') { throw "cleanup baseline failed: bundle already installed or query unavailable: $bundle" }
        $processResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
        if ($processResult.ExitCode -notin @(0, 1) -or -not [string]::IsNullOrWhiteSpace([string]$processResult.Stderr) -or -not [string]::IsNullOrWhiteSpace([string]$processResult.Stdout)) { throw "cleanup baseline failed: extension process state is not absent: $bundle" }
    }
    [void](Invoke-HdcOperation 'RemoveStaging' -AllowFailure)
    if (-not (Test-StagingAbsent (Invoke-HdcOperation 'StagingProbe' -AllowFailure))) { throw 'cleanup baseline failed: staging residual still present after RemoveStaging' }
    $script:StagingMayExist = $true
    [void](Invoke-HdcOperation 'MkdirStaging')
    $script:StagingSent = $true
    [void](Invoke-HdcOperation 'SendA')
    [void](Invoke-HdcOperation 'SendB')
    $script:CampaignStarted = $true
    Confirm-BundleInstalled 'InstallA' $script:BundleA 'A'
    $script:InstalledA = $true
    Confirm-BundleInstalled 'InstallB' $script:BundleB 'B'
    $script:InstalledB = $true
    $installCompletedAt = Get-Now
    $observation1 = Complete-ScenarioContext $context1
    [void](Assert-ScenarioEventContract 1 $observation1.Events)
    $capture1 = Invoke-Capture 'scenario-1-baseline' 1
    $installSeconds = ($installCompletedAt - [DateTimeOffset]$context1.StartedAt).TotalSeconds
    $scenario1Result = if ($observation1.CompleteWindowObserved -and -not $observation1.CaptureDegraded -and $capture1 -eq 'collected' -and $installSeconds -le 60 -and $script:InstalledA -and $script:InstalledB) { 'pass' } else { 'blocked' }
    $results.Add([ordered]@{ sequence_index = 1; scenario = 1; result = $scenario1Result; reason = 'machine-cleanup-baseline-and-install'; first_baseline_query_covered = ($firstBaselineQueryAt -ge [DateTimeOffset]$context1.StartedAt); install_elapsed_seconds = $installSeconds; install_completed_within_60_seconds = ($installSeconds -le 60); observation = $observation1.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation1

    # S2: runner opens A, verifies the Entry layout, then allows exactly Start and Allow.
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $entry2 = Invoke-LayoutCheckpoint 2 'scenario-2-entry-a' 'entry' $script:BundleA
    $pre2 = Get-ExactProcessCheckpoint @()
    $context2 = New-ScenarioContext 2
    $step2Start = Invoke-MechanicalStep $context2 1 '点击测试 App A 的 Start' $pre2 { param($events) Test-UniqueStartCondition $events $script:BundleA } -CaptureBefore $entry2
    $request2 = [string]$step2Start.Outcome.request_id
    Register-VerifiedRequest $request2 $script:BundleA 2
    $auth2 = Invoke-LayoutCheckpoint 2 'scenario-2-authorization' 'authorization' $script:BundleA -StepIndex 2 -StepId $step2Start.StepId -ExpectedAction '点击 Allow'
    $step2Allow = Invoke-MechanicalStep $context2 2 '点击 Allow' ([ordered]@{ status = 'pass'; reason = 'authorization-layout-verified'; request_id = $request2 }) {
        param($events)
        $extra = Test-UniqueStartCondition $events $script:BundleA
        if ([string]$extra.status -eq 'pass' -or [string]$extra.reason -notin @('UI_START-missing')) { return [pscustomobject]@{ status = 'invalid'; reason = 'unexpected-Start-or-Stop-after-Allow' } }
        $terminal = (Test-CorrelatedMarker $events $script:BundleA $request2 'CREATE_ACCEPTED') -or (Test-CorrelatedMarker $events $script:BundleA $request2 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $events $script:BundleA $request2 'VPN_CREATE_INVALID_FD') -or (Test-CorrelatedMarker $events $script:BundleA $request2 'START_PROMISE_REJECTED')
        if ($terminal) { return [pscustomobject]@{ status = 'pass'; reason = 'create-terminal-observed'; request_id = $request2 } }
        return [pscustomobject]@{ status = 'pending'; reason = 'create-terminal-missing-after-Allow' }
    } -CaptureBefore $auth2 -CaptureAfterName 'scenario-2-after-allow' -CaptureAfterProfile 'authorization-dismissed' -CaptureAfterExpectedBundle $script:BundleA
    $afterAllow = Test-CapturedLayoutProfile 'scenario-2-after-allow' 'authorization-dismissed' $script:BundleA
    if ([string]$afterAllow.status -ne 'pass') { Throw-ScenarioInvalid 2 "authorization-not-dismissed-after-Allow:$([string]$afterAllow.reason)" -StepIndex 2 -StepId $step2Allow.StepId -ExpectedAction $step2Allow.ExpectedAction -MachinePostcondition $afterAllow }
    $observation2 = Complete-ScenarioContext $context2
    [void](Assert-ScenarioEventContract 2 $observation2.Events @([ordered]@{ Bundle = $script:BundleA; RequestId = $request2 }))
    $onCreate2 = Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_ONCREATE'
    $accepted2 = (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_RESOLVED') -and (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'CREATE_ACCEPTED') -and (Test-PostCreateOpen $observation2.Events $script:BundleA $request2)
    # ADJ-20260808-0002 (C6): the S2 authorization button outcome is NOT a product negative fact
    # source. A START_PROMISE_REJECTED (or any authorization-layer rejection) cannot prove a
    # platform feature failure: it is blocked `authorization-outcome-unclassified`. Only an
    # Extension VPN_CREATE_REJECTED / INVALID_FD AFTER a VPN_ONCREATE was observed is a
    # functional fail (the platform demonstrably engaged the create). This also prevents a
    # mis-click on Allow from manufacturing a product fail.
    $extensionReject2 = ((Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_INVALID_FD')) -and $onCreate2
    $authUnclassified2 = ((Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_INVALID_FD') -or (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'START_PROMISE_REJECTED')) -and -not $extensionReject2
    $scenario2Result = if ($extensionReject2) { 'fail' } elseif ($authUnclassified2) { 'blocked' } elseif (-not $observation2.CompleteWindowObserved -or $observation2.CaptureDegraded) { 'blocked' } elseif ($onCreate2 -and $accepted2) { 'pass' } else { 'blocked' }
    # ADJ-20260808-0003: after CREATE_ACCEPTED, a precise `<bundle>:vpn` present checkpoint
    # proves the naming tuple resolves to a live Extension process. Absent / unverifiable is
    # blocked with an explicit process-target-unverified reason; it can never pass. S3/S5/S7
    # consume this verified checkpoint.
    $processTarget2 = $null
    if ($scenario2Result -eq 'pass') {
        # ADJ-20260808-0003: the Extension must already be registered as active before the
        # precise `:vpn` present checkpoint so the simulated (and live) pidof sees it.
        [void]$script:SimulationActiveBundles.Add($script:BundleA)
        $processTarget2 = Get-ProcessTargetCheckpoint $script:BundleA
        if ([string]$processTarget2.status -ne 'pass') {
            $scenario2Result = 'blocked'
        }
    }
    $results.Add([ordered]@{
        sequence_index = 2; scenario = 2; result = $scenario2Result; reason = $(if ($extensionReject2) { 'create-rejected-after-Allow' } elseif ($authUnclassified2) { 'authorization-outcome-unclassified' } elseif ($null -ne $processTarget2 -and [string]$processTarget2.status -ne 'pass') { [string]$processTarget2.reason } else { 'machine-verified-Allow-onCreate-create-fd' }); bundle = $script:BundleA; request_id = $request2
        process_target_verified = $(if ($null -ne $processTarget2) { [string]$processTarget2.status -eq 'pass' } else { $null })
        process_target_checkpoint = $processTarget2
        assertions = [ordered]@{ allow = $(if ($authUnclassified2) { 'blocked' } else { 'pass' }); vpn_on_create = $(if ($onCreate2) { 'pass' } else { 'blocked' }); vpn_connection_create_fd = $(if ($accepted2) { 'pass' } elseif ($extensionReject2) { 'fail' } else { 'blocked' }) }
        authorization_capture = [ordered]@{ name = 'scenario-2-authorization'; status = 'collected'; result = 'pass'; layout_checkpoint = $auth2 }
        observation = $observation2.Observation
    })
    Assert-ScenarioCaptureCanContinue $results $observation2

    # S3 consumes only the machine-verified S2 request and active bundle. No second Start is legal.
    if ($scenario2Result -ne 'pass') {
        $results.Add([ordered]@{ sequence_index = 3; scenario = 3; result = 'blocked'; reason = 'S2-machine-active-checkpoint-unavailable'; bundle = $script:BundleA; request_id = $request2; clean_reactivation_proof = $null; process_target_verified = $null })
    } else {
        [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
        $entry3 = Invoke-LayoutCheckpoint 3 'scenario-3-entry-a' 'entry' $script:BundleA
        $pre3 = Get-ExactProcessCheckpoint @($script:BundleA)
        $context3 = New-ScenarioContext 3
        $step3Stop = Invoke-MechanicalStep $context3 1 '点击测试 App A 的 Stop' $pre3 { param($events) Test-UniqueStopCondition $events $script:BundleA $request2 } -CaptureBefore $entry3 -CaptureAfterName 'scenario-3-after-stop' -CaptureAfterReviewOnly
        if (Test-SimulationStepHasEffect 3 1) { [void]$script:SimulationActiveBundles.Remove($script:BundleA) }
        $script:ProbeContexts[3] = New-ProcessProbeContext -Scenario 3 -Bundle $script:BundleA -RequireBundlePresent $true -RequiredCount ([int]$Freeze.process_absent_required_count) -SpacingSeconds ([double]$Freeze.process_absent_probe_spacing_seconds)
        $during3 = {
            param($events)
            if ((Test-StrictFallbackPrerequisites $events $script:BundleA $request2).Met) { [void](Invoke-ProcessFinalStateProbeSeries $script:ProbeContexts[3] $script:CurrentWindowEnd) }
        }
        $observation3 = Complete-ScenarioContext $context3 $during3
        [void](Assert-ScenarioEventContract 3 $observation3.Events @() @([ordered]@{ Bundle = $script:BundleA; RequestId = $request2 }))
        $hasDestroyBegin3 = (Test-CorrelatedMarker $observation3.Events $script:BundleA $request2 'VPN_DESTROY_BEGIN') -or @($observation3.Events | Where-Object { [string]$_.text -match 'VPN_FD_SNAPSHOT' -and [string]$_.text -match 'phase=pre-destroy' -and [string]$_.text -match "requestId=$([regex]::Escape($request2))(\||\s|$)" }).Count -gt 0
        if (-not (Test-CorrelatedMarker $observation3.Events $script:BundleA $request2 'VPN_ONDESTROY') -or -not $hasDestroyBegin3) { Throw-ScenarioInvalid 3 'Stop-postcondition-missing-onDestroy-or-destroy-begin' -StepIndex 1 -StepId $step3Stop.StepId -ExpectedAction $step3Stop.ExpectedAction }
        $final3 = Get-VpnFinalState $observation3.Events $script:BundleA $request2 $script:ProbeContexts[3] $true ([int]$Freeze.process_absent_required_count) ([double]$Freeze.process_absent_probe_spacing_seconds)
        $scenario3Result = if ($final3.result -eq 'fail') { 'fail' } elseif (-not $observation3.CompleteWindowObserved -or $observation3.CaptureDegraded -or $final3.result -ne 'pass') { 'blocked' } else { 'pass' }
        $results.Add([ordered]@{ sequence_index = 3; scenario = 3; result = $scenario3Result; reason = $final3.reason; bundle = $script:BundleA; request_id = $request2; terminal_mode = $final3.terminal_mode; process_target = [string]$script:ProbeContexts[3].ProcessTarget; process_target_verified = [bool]($scenario2Result -eq 'pass'); process_final_state_probes = @($script:ProbeContexts[3].Probes); bundle_present_during_probe = [bool]$script:ProbeContexts[3].BundlePresent; clean_reactivation_proof = $false; observation = $observation3.Observation })
        Assert-ScenarioCaptureCanContinue $results $observation3
    }

    # S4 mirrors S2, but the authorization layout is captured and verified before Deny.
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleB })
    $entry4 = Invoke-LayoutCheckpoint 4 'scenario-4-entry-b' 'entry' $script:BundleB
    $context4 = New-ScenarioContext 4
    $step4Start = Invoke-MechanicalStep $context4 1 '点击测试 App B 的 Start' ([ordered]@{ status = 'pass'; reason = 'B-entry-layout-verified' }) { param($events) Test-UniqueStartCondition $events $script:BundleB } -CaptureBefore $entry4
    $request4 = [string]$step4Start.Outcome.request_id
    Register-VerifiedRequest $request4 $script:BundleB 4
    $auth4 = Invoke-LayoutCheckpoint 4 'scenario-4-authorization' 'authorization' $script:BundleB -StepIndex 2 -StepId $step4Start.StepId -ExpectedAction '点击 Deny'
    $step4Deny = Invoke-MechanicalStep $context4 2 '点击 Deny' ([ordered]@{ status = 'pass'; reason = 'authorization-layout-verified'; request_id = $request4 }) {
        param($events)
        $layout = Test-CapturedLayoutProfile 'scenario-4-after-deny' 'authorization-dismissed' $script:BundleB
        if ([string]$layout.status -eq 'pass') { return [pscustomobject]@{ status = 'pass'; reason = 'authorization-dismissed-after-Deny' } }
        return [pscustomobject]@{ status = 'invalid'; reason = "Deny-layout-postcondition:$([string]$layout.reason)" }
    } -CaptureBefore $auth4 -CaptureAfterName 'scenario-4-after-deny' -CaptureAfterProfile 'authorization-dismissed' -CaptureAfterExpectedBundle $script:BundleB
    $observation4 = Complete-ScenarioContext $context4
    [void](Assert-ScenarioEventContract 4 $observation4.Events @([ordered]@{ Bundle = $script:BundleB; RequestId = $request4 }))
    $deny4 = Get-DenyAssessment $observation4.Events $script:BundleB $request4 $true ([bool]$observation4.CompleteWindowObserved)
    # ADJ-20260808-0002 (C6): if B's create/onCreate appears after the Deny click, the machine
    # cannot distinguish a mis-click on Allow from a system deny defect. Under the strong-reliable
    # trust model the manual click is NOT a product negative fact source, so this is scenario
    # invalid `deny-action-produced-create-untrusted` (never a product fail); a no-create full
    # window still passes below.
    if ([string]$deny4.result -eq 'fail' -and [string]$deny4.reason -eq 'deny-created-B-vpn') {
        Throw-ScenarioInvalid 4 'deny-action-produced-create-untrusted' -StepIndex 2 -StepId $step4Deny.StepId -ExpectedAction $step4Deny.ExpectedAction -MachinePostcondition $deny4
    }
    $scenario4Result = if ($deny4.result -eq 'fail') { 'fail' } elseif (-not $observation4.CompleteWindowObserved -or $observation4.CaptureDegraded) { 'blocked' } else { [string]$deny4.result }
    $results.Add([ordered]@{ sequence_index = 4; scenario = 4; result = $scenario4Result; reason = $deny4.reason; bundle = $script:BundleB; request_id = $request4; deny_screen = $true; deny_screen_capture = [ordered]@{ name = 'scenario-4-authorization'; status = 'collected'; visible = $true; result = 'pass'; layout_checkpoint = $auth4 }; full_window_after_action = [bool]$observation4.CompleteWindowObserved; observation = $observation4.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation4

    # S5: fresh A activation, then directly the A app-info machine gate and one force-stop action.
    # ADJ-20260808-0003: the Settings>VPN page is no longer a decisive step and is not asked of
    # the operator; it is only a not-required observation (never invalid/block/pass input).
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $entry5 = Invoke-LayoutCheckpoint 5 'scenario-5-entry-a' 'entry' $script:BundleA
    $context5 = New-ScenarioContext 5
    $step5Start = Invoke-MechanicalStep $context5 1 '点击测试 App A 的 Start' ([ordered]@{ status = 'pass'; reason = 'A-entry-layout-verified' }) { param($events) Test-UniqueStartCondition $events $script:BundleA } -CaptureBefore $entry5
    $request5 = [string]$step5Start.Outcome.request_id
    Register-VerifiedRequest $request5 $script:BundleA 5
    # ADJ-20260808-0003: dual-profile 8s same-name resample (entry OR authorization). selected_profile
    # decides direct activation vs reauthorization UI; infra/continuous capture is blocked, final
    # dual mismatch is scenario invalid.
    $reactivation = Invoke-LayoutChoiceCheckpoint 5 'scenario-5-reactivation' $script:BundleA -StepIndex 1 -StepId $step5Start.StepId -ExpectedAction $step5Start.ExpectedAction
    $actualReallowPath = if ([string]$reactivation.selected_profile -eq 'authorization') { 'system-reauthorization-UI' } else { 'direct-system-activation' }
    if ($actualReallowPath -eq 'system-reauthorization-UI') {
        [void](Invoke-MechanicalStep $context5 2 '点击 Allow' ([ordered]@{ status = 'pass'; reason = 'reauthorization-layout-verified' }) {
            param($events)
            $extra = Test-NoOperatorAction $events
            if ([string]$extra.status -ne 'pass') { return $extra }
            if ((Test-CorrelatedMarker $events $script:BundleA $request5 'CREATE_ACCEPTED') -or (Test-CorrelatedMarker $events $script:BundleA $request5 'VPN_CREATE_REJECTED')) { return [pscustomobject]@{ status = 'pass'; reason = 'reactivation-create-terminal'; request_id = $request5 } }
            return [pscustomobject]@{ status = 'pending'; reason = 'reactivation-create-terminal-missing' }
        } -CaptureBefore $reactivation -CaptureAfterName 'scenario-5-after-allow' -CaptureAfterProfile 'authorization-dismissed' -CaptureAfterExpectedBundle $script:BundleA)
    }
    $createTerminal5 = Wait-MachineCondition $context5 ([long]$step5Start.AnchorByte) ([DateTimeOffset]$step5Start.PromptAt) {
        param($events)
        # ADJ-20260808-0003 (C6): re-assert the verified Start window before judging the platform
        # create terminal. Extra operator UI actions (UI_STOP / UI_STOP_SKIPPED / duplicated Start /
        # wrong bundle / requestId) observed while waiting for the platform marker must invalidate
        # on the spot, never be masked as a platform marker-missing blocked.
        $uniqueStart = Test-UniqueStartCondition $events $script:BundleA
        if ([string]$uniqueStart.status -eq 'invalid') { return $uniqueStart }
        if ([string]$uniqueStart.status -eq 'pending') { return $uniqueStart }
        if ((Test-CorrelatedMarker $events $script:BundleA $request5 'CREATE_ACCEPTED') -or (Test-CorrelatedMarker $events $script:BundleA $request5 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $events $script:BundleA $request5 'START_PROMISE_REJECTED')) { return [pscustomobject]@{ status = 'pass'; reason = 'fresh-create-terminal'; request_id = $request5 } }
        return [pscustomobject]@{ status = 'pending'; reason = 'fresh-create-terminal-missing' }
    }
    # ADJ-20260808-0003 (C6): capture degradation (infra or non-infra) surfaces as status blocked
    # from Wait-MachineCondition; that is a plain runner blocked, never a scenario invalid.
    if ([string]$createTerminal5.status -eq 'blocked') {
        throw "scenario-5 machine-verification-blocked step=1 reason=$([string](Get-OptionalProperty $createTerminal5 'reason' 'machine-verification-blocked'))"
    }
    if ([string]$createTerminal5.status -ne 'pass') { Throw-ScenarioInvalid 5 ([string]$createTerminal5.reason) -StepIndex 1 -StepId $step5Start.StepId -ExpectedAction $step5Start.ExpectedAction }
    [void]$script:SimulationActiveBundles.Add($script:BundleA)
    $step5Info = Invoke-MechanicalStep $context5 3 '打开测试 App A 的应用信息页' ([ordered]@{ status = 'pass'; reason = 'fresh-A-request-bound'; request_id = $request5 }) {
        param($events)
        $extra = Test-NoOperatorAction $events
        if ([string]$extra.status -ne 'pass') { return $extra }
        $layout = Test-CapturedLayoutProfile 'scenario-5-app-info' 'settings-app-info' $script:BundleA
        if ([string]$layout.status -eq 'pass') { return [pscustomobject]@{ status = 'pass'; reason = 'A-app-info-layout-match' } }
        return [pscustomobject]@{ status = 'invalid'; reason = [string]$layout.reason }
    } -CaptureAfterName 'scenario-5-app-info' -CaptureAfterProfile 'settings-app-info' -CaptureAfterExpectedBundle $script:BundleA
    $preForce5 = Get-ExactProcessCheckpoint @($script:BundleA)
    # ADJ-20260808-0002 (C6): the S5 force-stop pre-checkpoint expects A active. A mismatch here
    # comes from platform residue / an indeterminable process state, not from an extra operator
    # event (operator mis-action is enforced by the global action guard as invalid). Classify
    # blocked, never a precondition scenario invalid.
    if ([string]$preForce5.status -ne 'pass') {
        throw "scenario-5 machine-verification-blocked step=4 reason=exact-process-precondition:$([string]$preForce5.reason)"
    }
    $script:ProbeContexts[5] = New-ProcessProbeContext -Scenario 5 -Bundle $script:BundleA -RequireBundlePresent $true -RequiredCount ([int]$Freeze.process_absent_required_count) -SpacingSeconds ([double]$Freeze.process_absent_probe_spacing_seconds)
    $forceEffectApplied = $false
    # The operator completes the visible Settings action, including its confirmation if one
    # appears. The post-action capture is evidence-only because Settings may leave AppDetail;
    # only the process-absent + bundle-present postcondition decides the force-stop effect.
    $step5Force = Invoke-MechanicalStep $context5 4 '点击强行停止，并完成随后出现的确认（如有）' $preForce5 {
        param($events)
        if (-not $forceEffectApplied) {
            if (Test-SimulationStepHasEffect 5 4) { [void]$script:SimulationActiveBundles.Remove($script:BundleA) }
            $forceEffectApplied = $true
        }
        $extra = Test-NoOperatorAction $events
        if ([string]$extra.status -ne 'pass') { return $extra }
        [void](Invoke-ProcessFinalStateProbeSeries $script:ProbeContexts[5] ((Get-Now).AddSeconds(20)))
        $absent = Test-ProcessAbsentEvidence $script:ProbeContexts[5] ([int]$Freeze.process_absent_required_count) ([double]$Freeze.process_absent_probe_spacing_seconds)
        if ($absent.Met -and $script:ProbeContexts[5].BundlePresent) { return [pscustomobject]@{ status = 'pass'; reason = 'fresh-request-extension-process-absent-bundle-present' } }
        if ($script:ProbeContexts[5].Aborted) { return [pscustomobject]@{ status = 'blocked'; reason = 'force-stop-process-check-unverifiable' } }
        return [pscustomobject]@{ status = 'invalid'; reason = [string]$absent.Reason }
    } -CaptureBefore ([ordered]@{ status = 'pass'; name = 'scenario-5-app-info'; profile = 'settings-app-info' }) -CaptureAfterName 'scenario-5-app-info-force-stop' -CaptureAfterReviewOnly -VerifyTimeoutSeconds 25
    $observation5 = Complete-ScenarioContext $context5
    [void](Assert-ScenarioEventContract 5 $observation5.Events @([ordered]@{ Bundle = $script:BundleA; RequestId = $request5 }))
    $onCreate5 = Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_ONCREATE'
    $create5 = (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'CREATE_ACCEPTED')
    $freshCreateProof = $create5 -and (Test-PostCreateOpen $observation5.Events $script:BundleA $request5)
    $absentEvidence5 = Test-ProcessAbsentEvidence $script:ProbeContexts[5] ([int]$Freeze.process_absent_required_count) ([double]$Freeze.process_absent_probe_spacing_seconds)
    $s5FdStillOpen = Test-S5PostDestroyStillOpen $observation5.Events $script:BundleA $request5
    $scenario5Result = if ($s5FdStillOpen) { 'fail' } elseif (-not $observation5.CompleteWindowObserved -or $observation5.CaptureDegraded -or -not $onCreate5 -or -not $freshCreateProof -or -not $absentEvidence5.Met -or -not $script:ProbeContexts[5].BundlePresent) { 'blocked' } else { 'pass' }
    $s5Reason = if ($s5FdStillOpen) { 'FD_STILL_OPEN' } elseif (-not $freshCreateProof) { 'fresh-create-proof-missing' } elseif (-not $absentEvidence5.Met) { [string]$absentEvidence5.Reason } else { 'settings-app-info-force-stop-terminal' }
    $results.Add([ordered]@{
        sequence_index = 5; scenario = 5; result = $scenario5Result; reason = $s5Reason; bundle = $script:BundleA; request_id = $request5
        settings_revoke_mechanism = [string]$Freeze.settings_revoke_mechanism; settings_vpn_page_policy = [string]$Freeze.settings_vpn_page_policy; settings_vpn_page_observation_only = $true
        # ADJ-20260808-0003: Settings>VPN page is not a decisive step and is not asked of the
        # operator; the capture is not-required and never invalid/block/pass input.
        settings_vpn_page_capture = [ordered]@{ name = 'scenario-5-settings-vpn-page'; status = 'not-required'; machine_verified = $false; note = 'observation-only optional; not asked of the operator' }
        app_info_force_stop_capture = [ordered]@{ name = 'scenario-5-app-info-force-stop'; status = [string]$step5Force.CaptureAfter.status; machine_verified = $false; observation_only = $true }
        terminal_mode = 'settings-app-info-force-stop'; fd_still_open = [bool]$s5FdStillOpen; process_target = [string]$script:ProbeContexts[5].ProcessTarget; process_target_verified = [bool]($scenario2Result -eq 'pass')
        process_final_state_probes = @($script:ProbeContexts[5].Probes); process_absent_evidence = [ordered]@{ met = [bool]$absentEvidence5.Met; reason = $absentEvidence5.Reason; required_count = [int]$Freeze.process_absent_required_count; required_spacing_seconds = [double]$Freeze.process_absent_probe_spacing_seconds; measured_spacing_seconds = $absentEvidence5.SpacingSeconds }; bundle_present_during_probe = [bool]$script:ProbeContexts[5].BundlePresent
        settings_reallow_path = [ordered]@{ expected = [string]$Freeze.settings_reallow_expected_path; actual = $actualReallowPath; match = ($actualReallowPath -eq [string]$Freeze.settings_reallow_expected_path); observation = 'machine-layout-and-event-classified'; policy = [string]$Freeze.settings_reallow_path_policy }
        assertions = [ordered]@{ vpn_on_create = $(if ($onCreate5) { 'pass' } else { 'blocked' }); vpn_connection_create_fd = $(if ($create5) { 'pass' } else { 'blocked' }); fresh_create_proof = $(if ($freshCreateProof) { 'pass' } else { 'blocked' }); force_stop = $(if ($absentEvidence5.Met) { 'pass' } else { 'blocked' }) }
        observation = $observation5.Observation
    })
    $s3Entry = @($results | Where-Object { [int]$_.scenario -eq 3 })[0]
    if ($null -ne $s3Entry -and $s3Entry -is [Collections.IDictionary] -and $s3Entry.Contains('clean_reactivation_proof') -and $null -ne $s3Entry.clean_reactivation_proof) { $s3Entry.clean_reactivation_proof = [bool]$freshCreateProof }
    Assert-ScenarioCaptureCanContinue $results $observation5

    # S6: exactly A Start then B Start. Only a frozen explicit B conflict code is a passing conflict result.
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $entry6A = Invoke-LayoutCheckpoint 6 'scenario-6-entry-a' 'entry' $script:BundleA
    $context6 = New-ScenarioContext 6
    $step6A = Invoke-MechanicalStep $context6 1 '点击测试 App A 的 Start' ([ordered]@{ status = 'pass'; reason = 'A-entry-layout-verified' }) { param($events) Test-UniqueStartCondition $events $script:BundleA } -CaptureBefore $entry6A
    $request6A = [string]$step6A.Outcome.request_id
    Register-VerifiedRequest $request6A $script:BundleA 6
    # ADJ-20260808-0003: S6 A optional reauthorization uses dual-profile 8s same-name resample
    # (entry OR authorization). When authorization is selected, the operator is asked one mechanical
    # Allow step (after capture, NO extra UI-action guard: A create terminal is platform markers
    # only). Infra/continuous capture is blocked; final dual mismatch is scenario invalid.
    $reauth6 = Invoke-LayoutChoiceCheckpoint 6 'scenario-6-reactivation-a' $script:BundleA -StepIndex 1 -StepId $step6A.StepId -ExpectedAction $step6A.ExpectedAction
    $s6AReauthPath = if ([string]$reauth6.selected_profile -eq 'authorization') { 'system-reauthorization-UI' } else { 'direct-system-activation' }
    if ($s6AReauthPath -eq 'system-reauthorization-UI') {
        [void](Invoke-MechanicalStep $context6 2 '点击 Allow' ([ordered]@{ status = 'pass'; reason = 'reauthorization-layout-verified'; request_id = $request6A }) {
            param($events)
            # ADJ-20260808-0003: no extra UI-action guard here (unlike S5's Test-NoOperatorAction).
            # The A create terminal is judged by the platform markers alone; an authorization-layer
            # or Extension outcome is classified (blocked/fail) after the window, never invalidated
            # by an extra operator action during the Allow step.
            if ((Test-CorrelatedMarker $events $script:BundleA $request6A 'CREATE_ACCEPTED') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'VPN_CREATE_INVALID_FD') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'START_PROMISE_REJECTED')) { return [pscustomobject]@{ status = 'pass'; reason = 'reauth-A-create-terminal'; request_id = $request6A } }
            return [pscustomobject]@{ status = 'pending'; reason = 'reauth-A-create-terminal-missing' }
        } -CaptureBefore $reauth6 -CaptureAfterName 'scenario-6-after-allow-a' -CaptureAfterProfile 'authorization-dismissed' -CaptureAfterExpectedBundle $script:BundleA)
    }
    $aTerminal6 = Wait-MachineCondition $context6 ([long]$step6A.AnchorByte) ([DateTimeOffset]$step6A.PromptAt) {
        param($events)
        # ADJ-20260808-0003 (C6): re-assert the verified A Start window before judging the platform
        # A create terminal. Extra operator UI actions observed while waiting for the platform
        # marker invalidate on the spot, never a platform marker-missing blocked.
        $uniqueStart = Test-UniqueStartCondition $events $script:BundleA
        if ([string]$uniqueStart.status -eq 'invalid') { return $uniqueStart }
        if ([string]$uniqueStart.status -eq 'pending') { return $uniqueStart }
        if ((Test-CorrelatedMarker $events $script:BundleA $request6A 'CREATE_ACCEPTED') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'VPN_CREATE_INVALID_FD') -or (Test-CorrelatedMarker $events $script:BundleA $request6A 'START_PROMISE_REJECTED')) { return [pscustomobject]@{ status = 'pass'; reason = 'A-create-terminal'; request_id = $request6A } }
        return [pscustomobject]@{ status = 'pending'; reason = 'A-create-terminal-missing' }
    }
    # ADJ-20260808-0003 (C6): capture degradation surfaces as blocked (infra or non-infra); a
    # plain runner blocked is never a scenario invalid.
    if ([string]$aTerminal6.status -eq 'blocked') {
        throw "scenario-6 machine-verification-blocked step=1 reason=$([string](Get-OptionalProperty $aTerminal6 'reason' 'machine-verification-blocked'))"
    }
    if ([string]$aTerminal6.status -ne 'pass') { Throw-ScenarioInvalid 6 ([string]$aTerminal6.reason) -StepIndex 1 -StepId $step6A.StepId -ExpectedAction $step6A.ExpectedAction }
    $aAccepted6 = Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'CREATE_ACCEPTED'
    if ($aAccepted6) { [void]$script:SimulationActiveBundles.Add($script:BundleA) }
    # ADJ-20260808-0003 (C6): S6 A outcomes follow S2's classification. A START_PROMISE_REJECTED (or
    # any reject/invalid-fd marker WITHOUT a preceding VPN_ONCREATE) is an authorization-layer outcome
    # with an unclassified product meaning: S6 blocked `authorization-outcome-unclassified` and S7
    # not-run-after-platform-blocked (never a fail, never a scenario invalid). Only a
    # VPN_CREATE_REJECTED / VPN_CREATE_INVALID_FD observed AFTER VPN_ONCREATE is a functional fail of
    # the conflict scenario (the platform demonstrably engaged the Extension create).
    $onCreate6A = Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'VPN_ONCREATE'
    $extensionReject6A = ((Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'VPN_CREATE_INVALID_FD')) -and $onCreate6A
    $authUnclassified6A = ((Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'VPN_CREATE_REJECTED') -or (Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'VPN_CREATE_INVALID_FD') -or (Test-CorrelatedMarker (Get-ScenarioContextEvents $context6) $script:BundleA $request6A 'START_PROMISE_REJECTED')) -and -not $extensionReject6A
    if (-not $aAccepted6) {
        # ADJ-20260808-0003: an Extension create rejected / invalid fd (VPN_ONCREATE observed) is a
        # functional fail of the S6 conflict scenario, never an operator invalid. A pure
        # authorization-layer outcome (START_PROMISE_REJECTED or a reject with no VPN_ONCREATE) is a
        # platform result with an unclassified product meaning: blocked, never a fail, never a
        # scenario invalid. B Start is not asked; S7 is not-run after the fail/blocked and finally
        # cleanup still runs.
        $observation6 = Complete-ScenarioContext $context6
        [void](Assert-ScenarioEventContract 6 $observation6.Events @([ordered]@{ Bundle = $script:BundleA; RequestId = $request6A }))
        $s6AResult = if ($extensionReject6A) { 'fail' } else { 'blocked' }
        $s6AReason = if ($extensionReject6A) { 'A-create-rejected-or-invalid-fd' } else { 'authorization-outcome-unclassified' }
        $s7AReason = if ($extensionReject6A) { 'not-run-after-functional-fail' } else { 'not-run-after-platform-blocked' }
        $results.Add([ordered]@{ sequence_index = 6; scenario = 6; result = $s6AResult; reason = $s6AReason; a_reauth_path = $s6AReauthPath; request_id_a = $request6A; request_id_b = $null; a_accepted = $false; a_on_create = [bool]$onCreate6A; a_extension_rejected = [bool]$extensionReject6A; a_auth_unclassified = [bool]$authUnclassified6A; b_rejected = $null; b_rejection_code = $null; b_accepted = $false; accepted_session_count_in_window = @($observation6.Events | Where-Object { [string]$_.text -match 'CREATE_ACCEPTED' }).Count; conflict_capture = [ordered]@{ name = 'scenario-6-conflict'; status = 'not-required'; review_only = $false }; observation = $observation6.Observation })
        $results.Add([ordered]@{ sequence_index = 7; scenario = 7; result = 'blocked'; reason = $s7AReason; active_bundle = $null; request_id = $null })
        $script:PartialScenarios = @($results)
        return @($results)
    }
    $pre6B = Get-ExactProcessCheckpoint @($script:BundleA)
    # ADJ-20260808-0002 (C6): the S6 B pre-checkpoint expects A active. A mismatch here comes
    # from platform residue / an indeterminable process state, not from an extra operator event
    # (operator mis-action is enforced by the global action guard as invalid). Classify blocked,
    # never a precondition scenario invalid.
    if ([string]$pre6B.status -ne 'pass') {
        throw "scenario-6 machine-verification-blocked step=3 reason=exact-process-precondition:$([string]$pre6B.reason)"
    }
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleB })
    $entry6B = Invoke-LayoutCheckpoint 6 'scenario-6-entry-b' 'entry' $script:BundleB
    $step6B = Invoke-MechanicalStep $context6 3 '点击测试 App B 的 Start' $pre6B { param($events) Test-UniqueStartCondition $events $script:BundleB } -CaptureBefore $entry6B -CaptureAfterName 'scenario-6-conflict'
    $request6B = [string]$step6B.Outcome.request_id
    Register-VerifiedRequest $request6B $script:BundleB 6
    $bTerminal6 = Wait-MachineCondition $context6 ([long]$step6B.AnchorByte) ([DateTimeOffset]$step6B.PromptAt) {
        param($events)
        # ADJ-20260808-0003 (C6): re-assert the verified B Start window before judging the B
        # create/rejection terminal. Extra operator UI actions observed while waiting for the
        # platform marker invalidate on the spot, never a platform marker-missing blocked.
        $uniqueStart = Test-UniqueStartCondition $events $script:BundleB
        if ([string]$uniqueStart.status -eq 'invalid') { return $uniqueStart }
        if ([string]$uniqueStart.status -eq 'pending') { return $uniqueStart }
        if (Test-CorrelatedMarker $events $script:BundleB $request6B 'CREATE_ACCEPTED') { return [pscustomobject]@{ status = 'pass'; reason = 'B-create-accepted'; request_id = $request6B } }
        # ADJ-20260808-0002 (C6): both the Extension create rejection and the UI promise rejection
        # are terminal for B. The code is extracted from the real Extension safeError comma-field
        # shape (`summary=code=<digits>,name=...,message=...`) or the historical top-level
        # `|code=<digits>|` field. When multiple rejected events exist, every one is checked and
        # any frozen code wins (never just the first).
        $rejected = @($events | Where-Object {
            ([string]$_.text -match 'VPN_CREATE_REJECTED\|' -or [string]$_.text -match 'START_PROMISE_REJECTED\|') -and
            [string]$_.text -match "requestId=$([regex]::Escape($request6B))(\||\s|$)"
        })
        if ($rejected.Count -gt 0) {
            $frozenCodes = @($Freeze.vpn_conflict_rejection_codes | ForEach-Object { [int]$_ })
            $frozenHit = $null
            $firstCode = $null
            foreach ($rej in $rejected) {
                $code = Get-RejectionErrorCode ([string]$rej.text)
                if ($null -eq $firstCode -and $null -ne $code) { $firstCode = $code }
                if ($null -ne $code -and $code -in $frozenCodes) { $frozenHit = $code; break }
            }
            if ($null -ne $frozenHit) { return [pscustomobject]@{ status = 'pass'; reason = 'B-frozen-conflict-code'; request_id = $request6B; code = $frozenHit } }
            # ADJ-20260808-0002 (C6): a rejection with no extractable code, or a non-frozen code,
            # is a platform result with an uncertain outcome: blocked (never scenario invalid).
            return [pscustomobject]@{ status = 'blocked'; reason = "B-conflict-code-not-frozen:$firstCode"; request_id = $request6B; code = $firstCode }
        }
        return [pscustomobject]@{ status = 'pending'; reason = 'B-create-terminal-missing' }
    }
    # ADJ-20260808-0003 (C6): a non-frozen B rejection code is a platform result with an
    # uncertain outcome: block the scenario (never scenario invalid), complete the current
    # observation, record S6 blocked `B-conflict-code-not-frozen:<code>`, and leave S7 as
    # not-run-after-platform-blocked. Only extra operations / wrong request / wrong order are
    # scenario invalid (already enforced by the mechanical step conditions above). Finally
    # cleanup + seal still run for the blocked S6/S7 pair.
    if ([string]$bTerminal6.status -eq 'blocked' -and [string]$bTerminal6.reason -match '^B-conflict-code-not-frozen') {
        $observation6 = Complete-ScenarioContext $context6
        [void](Assert-ScenarioEventContract 6 $observation6.Events @([ordered]@{ Bundle = $script:BundleA; RequestId = $request6A }, [ordered]@{ Bundle = $script:BundleB; RequestId = $request6B }))
        $bCode6 = Get-OptionalProperty $bTerminal6 'code' $null
        $results.Add([ordered]@{
            sequence_index = 6; scenario = 6; result = 'blocked'; reason = "B-conflict-code-not-frozen:$bCode6"
            a_reauth_path = $s6AReauthPath
            request_id_a = $request6A; request_id_b = $request6B; a_accepted = [bool]$aAccepted6; b_rejected = $true; b_rejection_code = $bCode6; b_accepted = $false
            accepted_session_count_in_window = @($observation6.Events | Where-Object { [string]$_.text -match 'CREATE_ACCEPTED' }).Count
            conflict_capture = [ordered]@{ name = 'scenario-6-conflict'; status = 'collected'; review_only = $false }
            observation = $observation6.Observation
        })
        $results.Add([ordered]@{ sequence_index = 7; scenario = 7; result = 'blocked'; reason = 'not-run-after-platform-blocked'; active_bundle = $null; request_id = $null })
        $script:PartialScenarios = @($results)
        return @($results)
    }
    # ADJ-20260808-0003 (C6): capture degradation surfaces as blocked (infra or non-infra); a
    # plain runner blocked is never a scenario invalid.
    if ([string]$bTerminal6.status -eq 'blocked') {
        throw "scenario-6 machine-verification-blocked step=3 reason=$([string](Get-OptionalProperty $bTerminal6 'reason' 'machine-verification-blocked'))"
    }
    if ([string]$bTerminal6.status -ne 'pass') { Throw-ScenarioInvalid 6 ([string]$bTerminal6.reason) -StepIndex 3 -StepId $step6B.StepId -ExpectedAction $step6B.ExpectedAction }
    $bAccepted6 = [string]$bTerminal6.reason -eq 'B-create-accepted'
    if ($bAccepted6) { [void]$script:SimulationActiveBundles.Add($script:BundleB) }
    $observation6 = Complete-ScenarioContext $context6
    [void](Assert-ScenarioEventContract 6 $observation6.Events @([ordered]@{ Bundle = $script:BundleA; RequestId = $request6A }, [ordered]@{ Bundle = $script:BundleB; RequestId = $request6B }))
    # ADJ-20260808-0003: scan the complete window for any accepted request outside the two
    # registered ids (CREATE_ACCEPTED markers are now inside the allowed requestId range).
    $unexpectedAccepted6 = @($observation6.Events | Where-Object {
        [string]$_.text -match 'CREATE_ACCEPTED' -and
        [string]$_.text -notmatch "requestId=$([regex]::Escape($request6A))(\||\s|$)" -and
        [string]$_.text -notmatch "requestId=$([regex]::Escape($request6B))(\||\s|$)"
    })
    if ($unexpectedAccepted6.Count -gt 0) { Throw-ScenarioInvalid 6 'unexpected-accepted-request-in-window' -StepIndex 3 -StepId $step6B.StepId -ExpectedAction $step6B.ExpectedAction }
    $aAcceptedCount6 = @($observation6.Events | Where-Object { [string]$_.text -match 'CREATE_ACCEPTED' -and [string]$_.text -match "requestId=$([regex]::Escape($request6A))(\||\s|$)" }).Count
    $bAcceptedCount6 = @($observation6.Events | Where-Object { [string]$_.text -match 'CREATE_ACCEPTED' -and [string]$_.text -match "requestId=$([regex]::Escape($request6B))(\||\s|$)" }).Count
    $dualAccepted6 = $aAcceptedCount6 -gt 0 -and $bAcceptedCount6 -gt 0
    # ADJ-20260808-0003: when B was rejected, the B Extension process may still be alive (the
    # platform may spawn the Extension before rejecting the create); B process state is observed
    # but never required absent. A `:vpn` present remains the required A-side window-end check.
    $process6 = if ($bAccepted6) { Get-ExactProcessCheckpoint @($script:BundleA, $script:BundleB) } else { Get-ExactProcessCheckpoint @($script:BundleA) -ObservedBundles @($script:BundleB) }
    $scenario6Result = if ($dualAccepted6 -or $bAccepted6) { 'fail' } elseif (-not $observation6.CompleteWindowObserved -or $observation6.CaptureDegraded -or [string]$process6.status -ne 'pass') { 'blocked' } elseif ([string]$bTerminal6.reason -eq 'B-frozen-conflict-code') { 'pass' } else { 'blocked' }
    $s6Reason = if ($dualAccepted6) { 'two-accepted-sessions-observed' } elseif ($bAccepted6) { 'B-create-accepted-instead-of-conflict-rejection' } else { 'B-explicit-conflict-rejection' }
    # ADJ-20260808-0003: the scenario-6-conflict capture is a required decisive capture, never
    # review-only; the record must not claim review_only semantics for a required capture.
    $results.Add([ordered]@{ sequence_index = 6; scenario = 6; result = $scenario6Result; reason = $s6Reason; a_reauth_path = $s6AReauthPath; request_id_a = $request6A; request_id_b = $request6B; a_accepted = [bool]$aAccepted6; b_rejected = (-not $bAccepted6); b_rejection_code = Get-OptionalProperty $bTerminal6 'code' $null; b_accepted = [bool]$bAccepted6; accepted_session_count_in_window = $aAcceptedCount6 + $bAcceptedCount6; machine_process_checkpoint = $process6; conflict_capture = [ordered]@{ name = 'scenario-6-conflict'; status = 'collected'; review_only = $false }; observation = $observation6.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation6

    # S7 consumes only S6's verified active A request. No final semantic confirmation is requested.
    if ($scenario6Result -ne 'pass') {
        $results.Add([ordered]@{ sequence_index = 7; scenario = 7; result = 'blocked'; reason = 'S6-active-checkpoint-unavailable'; active_bundle = $null; request_id = $null })
        $script:PartialScenarios = @($results)
        return @($results)
    }
    $activeBundle = $script:BundleA
    $activeRequest = $request6A
    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $activeBundle })
    $entry7 = Invoke-LayoutCheckpoint 7 'scenario-7-entry-a' 'entry' $activeBundle
    $pre7 = Get-ExactProcessCheckpoint @($activeBundle)
    $context7 = New-ScenarioContext 7
    $step7Stop = Invoke-MechanicalStep $context7 1 '点击测试 App A 的 Stop' $pre7 { param($events) Test-UniqueStopCondition $events $activeBundle $activeRequest } -CaptureBefore $entry7 -CaptureAfterName 'scenario-7-after-stop' -CaptureAfterReviewOnly
    if (Test-SimulationStepHasEffect 7 1) { [void]$script:SimulationActiveBundles.Remove($activeBundle) }
    $script:ProbeContexts[7] = New-ProcessProbeContext -Scenario 7 -Bundle $activeBundle -RequireBundlePresent $false -RequiredCount ([int]$Freeze.process_absent_required_count) -SpacingSeconds ([double]$Freeze.process_absent_probe_spacing_seconds)
    $during7 = {
        param($events)
        if ((Test-StrictFallbackPrerequisites $events $activeBundle $activeRequest).Met) { [void](Invoke-ProcessFinalStateProbeSeries $script:ProbeContexts[7] $script:CurrentWindowEnd) }
    }
    $observation7 = Complete-ScenarioContext $context7 $during7
    [void](Assert-ScenarioEventContract 7 $observation7.Events @() @([ordered]@{ Bundle = $activeBundle; RequestId = $activeRequest }))
    $hasDestroyBegin7 = (Test-CorrelatedMarker $observation7.Events $activeBundle $activeRequest 'VPN_DESTROY_BEGIN') -or @($observation7.Events | Where-Object { [string]$_.text -match 'VPN_FD_SNAPSHOT' -and [string]$_.text -match 'phase=pre-destroy' -and [string]$_.text -match "requestId=$([regex]::Escape($activeRequest))(\||\s|$)" }).Count -gt 0
    if (-not (Test-CorrelatedMarker $observation7.Events $activeBundle $activeRequest 'VPN_ONDESTROY') -or -not $hasDestroyBegin7) { Throw-ScenarioInvalid 7 'Stop-postcondition-missing-onDestroy-or-destroy-begin' -StepIndex 1 -StepId $step7Stop.StepId -ExpectedAction $step7Stop.ExpectedAction }
    $final7 = Get-VpnFinalState $observation7.Events $activeBundle $activeRequest $script:ProbeContexts[7] $false ([int]$Freeze.process_absent_required_count) ([double]$Freeze.process_absent_probe_spacing_seconds)
    $terminalAssessed7 = $final7.result -eq 'pass'
    $faultDegraded7 = $false
    $cleanupDone7 = $false
    $cleanupVerified7 = $false
    $cleanupCompletedAt7 = $null
    if ($terminalAssessed7) {
        $preUninstall = Invoke-ReviewOnlyCapture 'scenario-7-pre-uninstall' 7
        foreach ($faultOperation in @('FaultA', 'FaultB')) { if ((Invoke-FaultArtifact $faultOperation 7) -ne 'collected') { $faultDegraded7 = $true } }
        if ($script:InstalledB) {
            $uninstallB = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleB } -AllowFailure
            $script:CleanupActions.Add([ordered]@{ operation = 'Uninstall'; bundle = $script:BundleB; exit_code = $uninstallB.ExitCode })
            if ($uninstallB.ExitCode -eq 0) { $script:InstalledB = $false }
        }
        if ($script:InstalledA) {
            $uninstallA = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleA } -AllowFailure
            $script:CleanupActions.Add([ordered]@{ operation = 'Uninstall'; bundle = $script:BundleA; exit_code = $uninstallA.ExitCode })
            if ($uninstallA.ExitCode -eq 0) { $script:InstalledA = $false }
        }
        if ($script:StagingSent -or $script:StagingMayExist) { [void](Invoke-RemoveStagingVerified 'RemoveStaging') }
        $cleanupVerified7 = Test-TargetedCleanupState
        $cleanupDone7 = $true
        $cleanupCompletedAt7 = (Get-Now).ToString('o')
        $postCleanup = Invoke-ReviewOnlyCapture 'scenario-7-post-cleanup' 7
    } else {
        [void](Invoke-ReviewOnlyCapture 'scenario-7-final-state' 7)
    }
    $scenario7Result = if ($final7.result -eq 'fail') { 'fail' } elseif (-not $observation7.CompleteWindowObserved -or $observation7.CaptureDegraded -or $final7.result -ne 'pass' -or -not $cleanupVerified7 -or $faultDegraded7) { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{
        sequence_index = 7; scenario = 7; result = $scenario7Result; reason = $final7.reason; active_bundle = $activeBundle; request_id = $activeRequest; terminal_mode = $final7.terminal_mode
        terminal_assessed = [bool]$terminalAssessed7; terminal_mode_at_cleanup = $(if ($terminalAssessed7) { $final7.terminal_mode } else { $null }); process_target = [string]$script:ProbeContexts[7].ProcessTarget; process_target_verified = [bool]($scenario2Result -eq 'pass'); process_final_state_probes = @($script:ProbeContexts[7].Probes); bundle_present_during_probe = [bool]$script:ProbeContexts[7].BundlePresent
        cleanup_completed_at = $cleanupCompletedAt7; post_cleanup_capture = [bool]$cleanupDone7; post_cleanup_capture_name = $(if ($cleanupDone7) { 'scenario-7-post-cleanup' } else { 'scenario-7-final-state' }); bundle_process_cleanup_verified = [bool]$cleanupVerified7; fault_capture_degraded = [bool]$faultDegraded7; observation = $observation7.Observation
    })
    $script:PartialScenarios = @($results)
    return @($results)
}

function Test-TargetedCleanupState {
    $bundleStates = [Collections.Generic.List[object]]::new()
    $verified = $true
    foreach ($bundle in @($script:BundleA, $script:BundleB)) {
        $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
        $pidResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
        $dumpText = Get-HdcCombinedText $dumpResult
        $bundleAbsent = $dumpText -match 'failed to get information|not exist|not found'
        $processAbsent = $pidResult.ExitCode -in @(0, 1) -and [string]::IsNullOrWhiteSpace([string]$pidResult.Stdout) -and [string]::IsNullOrWhiteSpace([string]$pidResult.Stderr)
        if (-not $bundleAbsent -or -not $processAbsent) { $verified = $false }
        $bundleStates.Add([ordered]@{
            bundle = $bundle
            bundle_query_exit = $dumpResult.ExitCode
            bundle_absent = [bool]$bundleAbsent
            pid_query_exit = $pidResult.ExitCode
            process_absent = [bool]$processAbsent
        })
    }
    $stagingProbe = Invoke-HdcOperation 'StagingProbe' -AllowFailure
    $stagingAbsent = Test-StagingAbsent $stagingProbe
    if (-not $stagingAbsent -or $script:StagingSent -or $script:StagingMayExist) { $verified = $false }
    $script:CleanupVerification = [ordered]@{
        status = $(if ($verified) { 'verified-clean' } else { 'blocked-unknown-residual' })
        verified_absent = [bool]$verified
        checked_at = (Get-Now).ToString('o')
        bundles = @($bundleStates)
        staging = [ordered]@{
            probe_exit = $stagingProbe.ExitCode
            staging_absent = [bool]$stagingAbsent
            staging_sent_flag = [bool]$script:StagingSent
            staging_may_exist_flag = [bool]$script:StagingMayExist
        }
    }
    if ($verified) { $script:InstalledA = $false; $script:InstalledB = $false; $script:StagingSent = $false; $script:StagingMayExist = $false }
    return [bool]$verified
}

function Invoke-PreciseFinallyCleanup {
    param([Parameter(Mandatory)][string]$Reason)
    if ($DryRun) { return }
    if ($script:InstalledB) {
        $forceBResult = Invoke-HdcOperation 'ForceStop' @{ Bundle = $script:BundleB; Reason = $Reason } -AllowFailure
        $script:CleanupActions.Add([ordered]@{ operation = 'finally-force-stop'; bundle = $script:BundleB; exit_code = $forceBResult.ExitCode; not_used_as_revoke = $true })
        $uninstallBResult = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleB } -AllowFailure
        $script:CleanupActions.Add([ordered]@{ operation = 'finally-uninstall'; bundle = $script:BundleB; exit_code = $uninstallBResult.ExitCode; installed_flag = $true })
        if ($uninstallBResult.ExitCode -eq 0) { $script:InstalledB = $false }
    }
    if ($script:InstalledA) {
        $forceAResult = Invoke-HdcOperation 'ForceStop' @{ Bundle = $script:BundleA; Reason = $Reason } -AllowFailure
        $script:CleanupActions.Add([ordered]@{ operation = 'finally-force-stop'; bundle = $script:BundleA; exit_code = $forceAResult.ExitCode; not_used_as_revoke = $true })
        $uninstallAResult = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleA } -AllowFailure
        $script:CleanupActions.Add([ordered]@{ operation = 'finally-uninstall'; bundle = $script:BundleA; exit_code = $uninstallAResult.ExitCode; installed_flag = $true })
        if ($uninstallAResult.ExitCode -eq 0) { $script:InstalledA = $false }
    }
    if ($script:StagingSent -or $script:StagingMayExist) {
        [void](Invoke-RemoveStagingVerified 'finally-remove-staging')
    }
    [void](Test-TargetedCleanupState)
}

function Get-PublicRawReferences {
    $references = foreach ($artifact in $script:RawHilogArtifacts) {
        [ordered]@{ scenario = $artifact.scenario; reference = $artifact.reference; sha256 = $artifact.sha256; bytes = $artifact.bytes; host_path_sha256 = Get-TextSha256 ([string]$artifact.path) }
    }
    return @($references)
}

function Get-PublicScreenshotReferences {
    $references = foreach ($artifact in $script:CaptureArtifacts) {
        $entry = [ordered]@{ scenario = $artifact.scenario; name = $artifact.name; status = $artifact.status; failures = $artifact.failures }
        if ($artifact.status -eq 'collected') {
            $entry['screen'] = [ordered]@{ reference = "RAW-SCREEN-$($artifact.name)"; sha256 = Get-FileSha256 $artifact.screen_path; bytes = (Get-Item -LiteralPath $artifact.screen_path).Length; host_path_sha256 = Get-TextSha256 ([string]$artifact.screen_path) }
        }
        $entry
    }
    return @($references)
}

function Get-PublicLayoutReferences {
    $references = foreach ($artifact in $script:CaptureArtifacts) {
        $entry = [ordered]@{ scenario = $artifact.scenario; name = $artifact.name; status = $artifact.status; failures = $artifact.failures }
        if ($artifact.status -eq 'collected') {
            $entry['layout'] = [ordered]@{ reference = "RAW-LAYOUT-$($artifact.name)"; sha256 = Get-FileSha256 $artifact.layout_path; bytes = (Get-Item -LiteralPath $artifact.layout_path).Length; host_path_sha256 = Get-TextSha256 ([string]$artifact.layout_path) }
        }
        $entry
    }
    return @($references)
}

function Get-PublicFaultReferences {
    return @($script:FaultArtifacts | ForEach-Object {
        [ordered]@{
            scenario = $_.scenario
            operation = $_.operation
            reference = $_.reference
            status = $_.status
            sha256 = $_.sha256
            bytes = $_.bytes
            failures = $_.failures
            host_path_sha256 = Get-TextSha256 ([string]$_.path)
        }
    })
}

function New-CompleteRecord {
    param(
        [Parameter(Mandatory)]$Freeze,
        [Parameter(Mandatory)][object[]]$Scenarios,
        [Parameter(Mandatory)][string]$Overall,
        [Parameter(Mandatory)][string]$RecordStatus,
        [Parameter(Mandatory)][DateTimeOffset]$StartedAt,
        [Parameter(Mandatory)][DateTimeOffset]$EndedAt,
        [AllowNull()][string]$Failure,
        [AllowNull()][string]$InfrastructureReason,
        [Parameter(Mandatory)]$RepositoryBefore,
        [Parameter(Mandatory)][string]$FreezeSha256,
        [Parameter(Mandatory)][string]$FreezeContractSha256,
        [Parameter(Mandatory)][string]$ConfirmationContractSha256,
        [Parameter(Mandatory)][string]$ManifestSha256
    )
    $scenario2 = @($Scenarios | Where-Object { [int]$_.scenario -eq 2 })[0]
    $s3Record = @($Scenarios | Where-Object { [int]$_.scenario -eq 3 })[0]
    # Tri-state clean-reactivation proof: true/false only when scenario 3 was actually measured with
    # a terminal evaluation; $null when scenario 3 was never probed/measured (e.g. blocked early), so
    # "not probed" is never masqueraded as a false proof. Scenario entries are ordered dictionaries,
    # so PSObject property lookup cannot see their keys; use IDictionary index access.
    $s3ProofValue = $null
    if ($null -ne $s3Record) {
        # ADJ-20260808-0003: a present key with a null value must stay null (S3 was never
        # probed/measured); casting null to [bool] would masquerade "not probed" as false.
        if ($s3Record -is [Collections.IDictionary]) {
            if ($s3Record.Contains('clean_reactivation_proof')) {
                $rawProof = $s3Record['clean_reactivation_proof']
                $s3ProofValue = if ($null -eq $rawProof) { $null } else { [bool]$rawProof }
            }
        } elseif ($null -ne $s3Record.PSObject.Properties['clean_reactivation_proof']) {
            $rawProof = $s3Record.PSObject.Properties['clean_reactivation_proof'].Value
            $s3ProofValue = if ($null -eq $rawProof) { $null } else { [bool]$rawProof }
        }
    }
    $isEvidence = $script:ExecutionMode -eq 'live'
    # Non-evidence modes (dry-run/live-simulation) stay blocked unless the measured aggregation is an
    # explicit fail: a hard fail (e.g. post-destroy FD_STILL_OPEN) must never be downgraded to blocked.
    if (-not $isEvidence -and $Overall -notin @('fail', 'invalid')) { $Overall = 'blocked'; $RecordStatus = 'blocked' }
    $record = [ordered]@{
        schema_version = 1
        evidence_id = $Freeze.evidence_id
        exception = 'E3-PHYS-PREFLIGHT'
        information_status = 'current-measured'
        plan_status = $Freeze.plan_status
        record_status = $RecordStatus
        stage_or_gate = 'E3'
        related_stages_or_gates = @('E8')
        campaign_id = $Freeze.campaign_id
        attempt = $Freeze.attempt
        retry = [ordered]@{
            basis = $Freeze.retry.basis
            infrastructure_reason = $Freeze.retry.infrastructure_reason
            prior_record_reference = $(if ([string]$Freeze.retry.prior_record_path -eq 'N/A') { 'N/A' } else { 'PRIOR-BLOCKED-RECORD' })
            prior_record_path_sha256 = $(if ([string]$Freeze.retry.prior_record_path -eq 'N/A') { 'N/A' } else { Get-TextSha256 ([string]$Freeze.retry.prior_record_path) })
            prior_record_sha256 = $Freeze.retry.prior_record_sha256
        }
        prior_blocked_binding = $(if ($null -ne $script:PriorBlockedBinding) {
            [ordered]@{
                source = [string]$script:PriorBlockedBinding.source
                evidence_id = [string]$script:PriorBlockedBinding.evidence_id
                scenario_results_sha256 = [string]$script:PriorBlockedBinding.scenario_results_sha256
                hash_manifest_sha256 = [string]$script:PriorBlockedBinding.hash_manifest_sha256
                campaign_seal_sha256 = [string]$script:PriorBlockedBinding.campaign_seal_sha256
                binding_source = 'freeze-manifest'
            }
        } else { 'N/A' })
        execution_mode = $script:ExecutionMode
        simulation = [bool]$LiveSimulation
        is_evidence = [bool]$isEvidence
        non_evidence_reason = $(if ($isEvidence) { 'N/A' } else { 'host-only dry-run or LiveSimulation; no physical-device evidence' })
        target_tuple = [ordered]@{
            distribution = [string]$Freeze.target_tuple.distribution
            device_model = $Freeze.target_tuple.device_model
            device_alias = 'PHYS-1'
            full_system_build = $Freeze.target_tuple.full_system_build
            api = $Freeze.target_tuple.api
            architecture = 'arm64'
            kernel_arch = $Freeze.target_tuple.kernel_arch
            app_abi = $Freeze.target_tuple.app_abi
            sdk_api_syscap = "$($Freeze.sdk.version) / API $($Freeze.sdk.api) / $($Freeze.sdk.syscap_basis)"
            channel = 'ordinary-development-signing-only'
        }
        hdc_target_reference = 'PHYS-1 out-of-repository controlled mapping; real target never projected'
        signing = $Freeze.signing
        code_sha = $Freeze.code_sha
        upstream_sha = 'N/A - no Go, NetBird, WireGuard, or other upstream runtime allowed'
        source_archive_sha256 = $Freeze.source.archive_sha256
        source_manifest_sha256 = $Freeze.source.manifest_sha256
        sdk_sha256 = @($Freeze.sdk.files | ForEach-Object { [ordered]@{ path_reference_sha256 = Get-TextSha256 ([string]$_.path); sha256 = $_.sha256 } })
        runner_sha256 = $Freeze.runner_sha256
        artifact_sha256 = $Freeze.artifact_sha256
        hdc = [ordered]@{ version = $Freeze.hdc.version; sha256 = $Freeze.hdc.sha256 }
        freeze_manifest_sha256 = $FreezeSha256
        # ADJ-20260810-0001 (C6): dual contract projection - the standard final freeze contract
        # (full projection, including the ready freeze's preflight_inputs_frozen_at) and the stable
        # confirmation contract (two-phase-invariant projection that confirmation/review records
        # actually bind). Both are projected with their exact names so consumers can verify either
        # binding without re-deriving it.
        freeze_contract_sha256 = $FreezeContractSha256
        confirmation_contract_sha256 = $ConfirmationContractSha256
        preflight_inputs_frozen_at = $Freeze.preflight_inputs_frozen_at
        scenario_window_seconds = 60
        observation_semantics = 'ADJ-20260808-0002 strong-reliable protocol (mechanical-action-only-machine-verified-v1): one continuous campaign HiLog capture; pre-scenario byte anchors exclude prior buffer; device_observed_at bounds first mechanical action prompt through last action plus at least 60 seconds; frozen CST=>+08:00 zone map; device clock skew tolerance 3s; operator sees only single-step "现在只做X，完成后按回车" and Read-Host is mechanical enter only (no READY/ACK/token/y-n semantic gates); machine layout gates (deterministic-layout-v1) before Allow/Deny and after decisive captures; scenario 1 is fully machine-operated install; scenario 3/7 terminal prefers callback destroy terminal plus post-destroy fd snapshot, otherwise strict-process-boundary needs unique stop/onDestroy/destroy-begin plus consecutive absent host process probes (>=2, >=3s apart, bundle present for scenario 3); process probes pidof only the <bundle>:vpn Extension ability process (ADJ-20260808-0001) while BundleDump proves the bundle/main App stays installed; any extra Start/Stop/UI_STOP_SKIPPED/wrong requestId/order is scenario invalid and stops later scenarios as not-run-due-to-invalid; scenario 5 revokes via atomic Settings navigation steps with machine layout gates plus force-stop then :vpn absent + bundle present; scenario 6 is fully machine: unique A CREATE_ACCEPTED, unique B CREATE_REJECTED with frozen code 2203002, no dual accepted and no operator dual-active fields; scenario 7 binds S6 verified A request only and never asks FINAL-CLEANUP; overall priority integrity invalid > scenario invalid > fail > blocked > pass; probe results are recorded before any cleanup and never backfilled from finally'
        settings_reallow_expected_path = $Freeze.settings_reallow_expected_path
        settings_reallow_path_policy = $Freeze.settings_reallow_path_policy
        settings_revoke_mechanism = $Freeze.settings_revoke_mechanism
        settings_vpn_page_policy = $Freeze.settings_vpn_page_policy
        settings_vpn_page_observation_only = $true
        destroy_terminal_policy = $Freeze.destroy_terminal_policy
        process_absent_required_count = [int]$Freeze.process_absent_required_count
        process_absent_probe_spacing_seconds = [double]$Freeze.process_absent_probe_spacing_seconds
        process_probe_target = [string]$Freeze.process_probe_target # ADJ-20260808-0001: pidof targets <bundle>:vpn Extension process, not bundle UI process
        cleanup_baseline = 'A/B absent; no A/B process; no active VPN; unrelated VPN isolated; staging removed before send'
        scenarios = @($Scenarios)
        scenario_aggregation = [ordered]@{
            mapping = '1=cleanup_and_install; 2=allow_and_fd; 3=active_stop; 4=deny; 5=settings_revoke; 6=second_vpn_conflict; 7=final_cleanup'
            scenario_2_rule = 'overall is pass only when allow, vpn_on_create, and vpn_connection_create_fd are all pass; fail dominates blocked'
            scenario_2_assertions = $(if ($null -ne $scenario2) { $scenario2.assertions } else { $null })
            scenario_5_rule = 'settings-app-info-force-stop revoke under strong protocol: step3 strict machine layout gate verifies the visible AppDetail subtree, expected label, and force-stop control; step4 force-stop capture is observation-only; final effect requires :vpn Extension process consecutive absent plus bundle present; no operator technical-fact confirmation'
            scenario_6_rule = 'machine-only conflict: unique A CREATE_ACCEPTED + unique B CREATE_REJECTED with frozen code 2203002; dual accepted or B accepted is fail; any extra Start/Stop/order deviation is invalid; no_dual/dual operator fields are non-authoritative and must be absent/null'
            scenario_7_rule = 'binds only S6 machine-verified active A request/bundle; expects UI_STOP/onDestroy/pre-destroy/destroy-begin and :vpn final state; wrong bundle stop or extra start is invalid; no FINAL-CLEANUP operator confirmation; uninstall cleanup only after terminal assessment; finally-absent never backfills'
            s3_strict_process_boundary_gate = 'scenario 3 strict-process-boundary fallback pass additionally requires scenario 5 same-bundle fresh request CREATE_ACCEPTED plus post-create open (clean_reactivation_proof); without it overall stays blocked'
            # Tri-state mirror of the scenario-3 entry: true/false when S3 was measured with a
            # terminal evaluation; null when S3 was never probed/measured (never a disguised false).
            s3_clean_reactivation_proof = $s3ProofValue
            overall_rule = 'integrity invalid > scenario invalid > fail > blocked > pass; first scenario invalid stops later scenarios as not-run-due-to-invalid; scenario 3 strict-process-boundary without clean reactivation proof => blocked; finally cleanup/seal never changes overall'
            measured_scenario_overall = Get-ScenarioAggregation $Scenarios
            overall = $Overall
        }
        started_at = $StartedAt.ToString('o')
        ended_at = $EndedAt.ToString('o')
        clock_source = [ordered]@{
            host = 'DateTimeOffset.Now recorded at observation'
            device = 'HiLog year/zone timestamp; CST frozen to +08:00; unknown zone blocked'
            device_zone_map = $script:FrozenDeviceZoneMap
            device_clock_skew_tolerance_seconds = [double]$script:DeviceClockSkewToleranceSeconds
            host_observed_time_recorded = $true
            virtual_clock = [bool]$LiveSimulation
        }
        raw_hilog_reference = Get-PublicRawReferences
        operator_wait_state_reference = [ordered]@{ path = 'operator-wait-state.json'; sha256 = Get-FileSha256 (Join-Path $script:EvidencePath 'operator-wait-state.json'); sealed_by = 'hash-manifest.json'; pollable_without_device_commands = $true }
        transcript_reference = [ordered]@{ path = 'projection/transcript.redacted.jsonl'; sha256 = Get-FileSha256 $script:ProjectionTranscript; projection_only = $true; raw_transcript_exists = $false; chain_head = $script:TranscriptPreviousHash }
        screenshot_reference = Get-PublicScreenshotReferences
        layout_state_reference = Get-PublicLayoutReferences
        fault_reference = [ordered]@{ strategy = 'static read-only A/B-targeted find by exact frozen bundle name; each output is an independent RawRoot artifact'; degraded = [bool](@($script:CaptureDegraded | Where-Object { $_.component -in @('FaultA', 'FaultB') }).Count -gt 0); artifacts = Get-PublicFaultReferences }
        hash_manifest_reference = [ordered]@{ path = 'hash-manifest.json'; sha256 = $ManifestSha256; sealed_by = 'campaign-seal.json' }
        forbidden_capabilities_audit = [ordered]@{
            no_go = $true; no_netbird = $true; no_wireguard = $true; no_private_fork = $true; no_manage_vpn = $true
            no_privileged_bypass = $true; no_automated_device_input = $true
            source_audit_basis = 'frozen source manifest and runner allowlist'
        }
        actual = $(if ([string]::IsNullOrEmpty($Failure)) { 'Bounded seven-scenario observation; see scenarios and raw references.' } else { "Runner stopped: $Failure" })
        overall = $Overall
        verdict = $Overall
        scope_statement = 'Exact frozen target E3 reachability only; no E4-E7, product, data-plane, or E8 OPEN conclusion.'
        cleanup_result = [ordered]@{
            status = $script:CleanupVerification.status
            verified_absent = [bool]$script:CleanupVerification.verified_absent
            installed_a_remaining = [bool]$script:InstalledA
            installed_b_remaining = [bool]$script:InstalledB
            staging_sent_remaining = [bool]$script:StagingSent
            staging_may_exist_remaining = [bool]$script:StagingMayExist
            targeted_verification = $script:CleanupVerification.bundles
            actions = @($script:CleanupActions)
            pre_uninstall_fd_snapshot_required = $true
            force_stop_role = 'notUsedAsRevoke residual cleanup only'
        }
        capture_degraded = @($script:CaptureDegraded)
        observation_only_degraded = @($script:ObservationOnlyDegraded)
        integrity_violations = @()
        repository_before = $RepositoryBefore.Fingerprint
        hdc_logical_calls = $script:HdcLogicalCallCount
        hdc_processes_started = $script:HdcProcessStartCount
        operator = [ordered]@{ role = $Freeze.operator_role; attestation = $(if ($isEvidence) { 'collected-separately' } else { 'not-attested-non-evidence' }) }
        reviewer = 'pending'
        reviewer_role = $Freeze.independent_reviewer_role
        reviewed_at = 'pending'
        review_record = 'pending'
        # ADJ-20260810-0001 (C6): sealed projection of the governance bindings. Only hashes and the
        # authorization ID are projected; real paths are never leaked (path_sha256 only), and each
        # binding is anchored to the freeze contract.
        machine_fresh_confirmation = $(if ($null -ne $script:MachineFreshConfirmation) {
            [ordered]@{
                status = 'pass'
                authorization_id = [string]$script:MachineFreshConfirmation.authorization_id
                record_sha256 = [string]$script:MachineFreshConfirmation.record_sha256
                record_path_sha256 = [string]$script:MachineFreshConfirmation.record_path_sha256
                confirmation_contract_sha256 = $ConfirmationContractSha256
            }
        } else { 'N/A' })
        independent_review_record = $(if ($null -ne $script:IndependentReviewRecord) {
            [ordered]@{
                status = 'pass'
                reviewer_role = [string]$script:IndependentReviewRecord.reviewer_role
                record_sha256 = [string]$script:IndependentReviewRecord.record_sha256
                record_path_sha256 = [string]$script:IndependentReviewRecord.record_path_sha256
                confirmation_contract_sha256 = $ConfirmationContractSha256
            }
        } else { 'N/A' })
    }
    if (-not [string]::IsNullOrEmpty($Failure)) { $record['failure'] = $Failure }
    if (-not [string]::IsNullOrEmpty($InfrastructureReason)) { $record['infrastructure_reason'] = $InfrastructureReason }
    if ($null -ne $script:ScenarioInvalid) {
        $record['invalidated_step'] = [ordered]@{
            scenario = [int]$script:ScenarioInvalid.scenario
            step_index = $script:ScenarioInvalid.step_index
            step_id = $script:ScenarioInvalid.step_id
            reason = [string]$script:ScenarioInvalid.reason
            detected_at = [string]$script:ScenarioInvalid.detected_at
        }
        $record['scenario_invalid'] = $script:ScenarioInvalid
    }
    return $record
}

function Write-CollectionManifest {
    param([Parameter(Mandatory)][string]$Root)
    $manifestPath = Join-Path $Root 'hash-manifest.json'
    $excluded = @('hash-manifest.json', 'scenario-results.json', 'campaign-seal.json')
    $files = Get-ChildItem -LiteralPath $Root -File -Recurse | Where-Object { $_.Name -notin $excluded } | Sort-Object FullName
    $entries = foreach ($file in $files) {
        [ordered]@{ path = [IO.Path]::GetRelativePath($Root, $file.FullName).Replace('\', '/'); sha256 = Get-FileSha256 $file.FullName; bytes = $file.Length }
    }
    $external = @()
    foreach ($artifact in $script:RawHilogArtifacts) {
        $external += [ordered]@{ reference = $artifact.reference; sha256 = $artifact.sha256; bytes = $artifact.bytes; host_path_sha256 = Get-TextSha256 ([string]$artifact.path) }
    }
    foreach ($artifact in $script:CaptureArtifacts | Where-Object { $_.status -eq 'collected' }) {
        foreach ($path in @($artifact.screen_path, $artifact.layout_path)) {
            $external += [ordered]@{ reference = "RAW-$([IO.Path]::GetFileName($path))"; sha256 = Get-FileSha256 $path; bytes = (Get-Item -LiteralPath $path).Length; host_path_sha256 = Get-TextSha256 ([string]$path) }
        }
    }
    foreach ($artifact in $script:FaultArtifacts) {
        $external += [ordered]@{ reference = $artifact.reference; sha256 = $artifact.sha256; bytes = $artifact.bytes; host_path_sha256 = Get-TextSha256 ([string]$artifact.path) }
    }
    Write-JsonFile $manifestPath ([ordered]@{
        schema_version = 1
        algorithm = 'SHA-256'
        generated_at = (Get-Now).ToString('o')
        transcript_chain_head = $script:TranscriptPreviousHash
        scope = 'collection artifacts; scenario-results.json is sealed separately to avoid a self-reference cycle'
        files = @($entries)
        external_raw_files = @($external)
    })
    return $manifestPath
}

function Write-CampaignSeal {
    param([Parameter(Mandatory)][string]$Root)
    $recordPath = Join-Path $Root 'scenario-results.json'
    $manifestPath = Join-Path $Root 'hash-manifest.json'
    Write-JsonFile (Join-Path $Root 'campaign-seal.json') ([ordered]@{
        schema_version = 1
        algorithm = 'SHA-256'
        record = [ordered]@{ path = 'scenario-results.json'; sha256 = Get-FileSha256 $recordPath }
        manifest = [ordered]@{ path = 'hash-manifest.json'; sha256 = Get-FileSha256 $manifestPath }
        sealed_at = (Get-Now).ToString('o')
    })
}

function Test-TranscriptIntegrity {
    param([Parameter(Mandatory)][string]$Path)
    $violations = [Collections.Generic.List[string]]::new()
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @('transcript-missing') }
    $previousHash = ('0' * 64)
    $expectedIndex = 1
    foreach ($line in (Get-Content -LiteralPath $Path)) {
        $doc = $null
        try {
            try {
                $doc = [System.Text.Json.JsonDocument]::Parse($line)
            } catch {
                $violations.Add('transcript-json-invalid')
                continue
            }
            try {
                $root = $doc.RootElement
                $payloadElement = $root.GetProperty('payload')
                $payloadRaw = $payloadElement.GetRawText()
                $payloadCanonical = $root.GetProperty('payload_canonical').GetString()
                $storedEntryHash = $root.GetProperty('entry_hash').GetString()
                $index = $payloadElement.GetProperty('index').GetInt32()
                $entryPreviousHash = $payloadElement.GetProperty('previous_hash').GetString()
            } catch {
                $violations.Add('transcript-json-invalid')
                continue
            }
            if ($index -ne $expectedIndex) { $violations.Add('transcript-order-invalid') }
            if ([string]$entryPreviousHash -ne $previousHash) { $violations.Add('transcript-previous-hash-invalid') }
            # Compare payload raw JSON text to stored payload_canonical without ConvertFrom-Json object roundtrip
            # (ISO date / single-element array type drift would otherwise false-positive).
            if ($payloadRaw -ne [string]$payloadCanonical) { $violations.Add('transcript-payload-canonical-mismatch') }
            $entryHash = Get-TextSha256 ([string]$payloadCanonical)
            if ($entryHash -ne [string]$storedEntryHash) { $violations.Add('transcript-entry-hash-invalid') }
            $previousHash = $entryHash
            $expectedIndex++
        } finally {
            if ($null -ne $doc) { $doc.Dispose() }
        }
    }
    if ($previousHash -ne $script:TranscriptPreviousHash) { $violations.Add('transcript-chain-head-invalid') }
    return @($violations | Select-Object -Unique)
}

function Test-EvidenceIntegrity {
    param([Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][object[]]$Scenarios)
    $violations = [Collections.Generic.List[string]]::new()
    foreach ($violation in @(Test-TranscriptIntegrity $script:ProjectionTranscript)) { $violations.Add($violation) }
    $manifestPath = Join-Path $Root 'hash-manifest.json'
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        $violations.Add('hash-manifest-missing')
    } else {
        try {
            $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -Depth 40
            foreach ($entry in @($manifest.files)) {
                $filePath = Get-NormalizedPath (Join-Path $Root ([string]$entry.path).Replace('/', [IO.Path]::DirectorySeparatorChar))
                if (-not (Test-IsUnderPath $filePath $Root) -or -not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
                    $violations.Add("manifest-file-missing:$($entry.path)")
                } elseif ((Get-FileSha256 $filePath) -ne [string]$entry.sha256) {
                    $violations.Add("manifest-hash-mismatch:$($entry.path)")
                }
            }
        } catch { $violations.Add('hash-manifest-invalid') }
    }
    $sequence = @($Scenarios | ForEach-Object { [int]$_.scenario })
    if (($sequence -join ',') -ne '1,2,3,4,5,6,7') { $violations.Add('scenario-order-invalid') }
    # ADJ-20260808-0003: the sealed operator-wait state must end complete. A crash/invalid still
    # seals via finally, so a non-complete sealed wait state means the state file was tampered or
    # the seal ran before completion: evidence integrity invalid.
    $waitStatePath = Join-Path $Root 'operator-wait-state.json'
    if (-not (Test-Path -LiteralPath $waitStatePath -PathType Leaf)) {
        $violations.Add('operator-wait-state-missing')
    } else {
        try {
            $waitState = Get-Content -LiteralPath $waitStatePath -Raw | ConvertFrom-Json -Depth 20
            if ([string]$waitState.phase -ne 'complete' -or -not [bool]$waitState.complete -or [string]::IsNullOrWhiteSpace([string]$waitState.completed_at)) {
                $violations.Add('operator-wait-state-not-complete')
            }
        } catch { $violations.Add('operator-wait-state-invalid') }
    }
    foreach ($artifact in $script:CaptureArtifacts) {
        if ($artifact.status -eq 'collected' -and (-not (Test-Path -LiteralPath $artifact.screen_path -PathType Leaf) -or -not (Test-Path -LiteralPath $artifact.layout_path -PathType Leaf))) {
            $violations.Add("capture-reference-missing:$($artifact.name)")
        }
    }
    return @($violations | Select-Object -Unique)
}

function Get-FailureClassification {
    param([Parameter(Mandatory)][string]$Message)
    if ($Message.StartsWith('SCENARIO_INVALID', [StringComparison]::Ordinal) -or $Message -match '^scenario-[1-7] SCENARIO_INVALID') {
        return [pscustomobject]@{ Overall = 'invalid'; RecordStatus = 'invalidated'; InfrastructureReason = $null; RetryAuthorized = $false }
    }
    if ($Message.StartsWith('FUNCTIONAL_FAIL', [StringComparison]::Ordinal)) {
        return [pscustomobject]@{ Overall = 'fail'; RecordStatus = 'collected'; InfrastructureReason = $null; RetryAuthorized = $false }
    }
    if ($Message.StartsWith('RUNNER_HOST_FAILURE', [StringComparison]::Ordinal)) {
        return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = 'runner-host-failure'; RetryAuthorized = $true }
    }
    # Generic capture_degraded / time-parse / missing artifacts are non-infrastructure. Only real capture
    # process exit/stderr/start/timeout and HDC transport failures authorize USB retry.
    if ($Message -match '(?i)exit\s*=\s*(124|125)|\bHDC(?:\s+operation)?\s+timeout\b|HDC infrastructure interruption|hdc-usb-interruption|HDC Process\.Start|\bUSB\b|\boffline\b|\bdisconnect(?:ed)?\b|transport (?:offline|error|fail)|target.+not found|connect(?:ion)?.+fail|channel.+fail|continuous capture infrastructure failure|raw-hilog-(?:start|process|stderr)|capture process exited|unable to start continuous campaign capture') {
        return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = 'hdc-usb-interruption'; RetryAuthorized = $true }
    }
    if ($Message -match '(?i)System\.IO\.IOException|disk full|not enough space|collection-storage-failure') {
        return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = 'collection-storage-failure'; RetryAuthorized = $true }
    }
    # "access denied" alone is not storage infrastructure; keep storage match narrower than permission probes.
    if ($Message -match '(?i)access denied writing|access denied while writing') {
        return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = 'collection-storage-failure'; RetryAuthorized = $true }
    }
    # Unknown exceptions / non-infrastructure capture or install confirmation gaps: blocked, no retry.
    return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = $null; RetryAuthorized = $false }
}

function Set-CaptureDegradedScenarios {
    param([Parameter(Mandatory)][object[]]$Scenarios)
    if ($script:CaptureDegraded.Count -eq 0) { return }
    $globalDegradation = @($script:CaptureDegraded | Where-Object { [int]$_.scenario -eq 0 }).Count -gt 0
    $affected = @($script:CaptureDegraded | Where-Object { [int]$_.scenario -in 1..7 } | ForEach-Object { [int]$_.scenario } | Select-Object -Unique)
    # Degradation never overrides an explicit fail: a hard fail (e.g. post-destroy FD_STILL_OPEN)
    # outranks capture degradation, so only non-fail results are downgraded to blocked here.
    foreach ($scenario in $Scenarios) {
        if (($globalDegradation -or [int]$scenario.scenario -in $affected) -and [string]$scenario.result -notin @('fail', 'invalid')) {
            $scenario.result = 'blocked'
            $scenario.reason = 'capture-degraded'
            if ([int]$scenario.scenario -eq 2) {
                $scenario.assertions = [ordered]@{ allow = 'blocked'; vpn_on_create = 'blocked'; vpn_connection_create_fd = 'blocked' }
            }
        }
    }
}

function Invoke-RunnerSelfTest {
    $failures = [Collections.Generic.List[string]]::new()
    function Check([bool]$Condition, [string]$Name) {
        if ($Condition) { Write-Host "SELFTEST_PASS=$Name" } else { $failures.Add($Name); Write-Host "SELFTEST_FAIL=$Name" }
    }
    try { [void](Get-HdcInvocation 'BundleDump' @{}); Check $false 'required-Bundle-rejected' } catch { Check ($_.Exception.Message -match 'requires parameter') 'required-Bundle-rejected' }
    try { [void](Get-HdcInvocation 'ScreenCap' @{}); Check $false 'required-Name-rejected' } catch { Check ($_.Exception.Message -match 'requires parameter') 'required-Name-rejected' }
    try { [void](Get-HdcInvocation 'NotAllowed' @{}); Check $false 'unknown-command-rejected' } catch { Check ($_.Exception.Message -match 'not allowlisted') 'unknown-command-rejected' }
    $script:ActualTarget = 'target-canary.example.test:8710'
    $sensitive = [ordered]@{
        boolean = $true
        nested = [ordered]@{
            target = $script:ActualTarget
            ipv4 = '10.23.45.67:8710'
            ipv6 = '[2001:db8::1234]:8710'
            ipv6_compressed_numeric = '2001:4860::1'
            ipv6_full_numeric = '2001:4860:0:0:0:0:0:1'
            host = 'host=device-canary.example.test:9911'
            mac = '00:11:22:33:44:55'
            serial = 'SN-CANARY12345678'
        }
    }
    $protected = Protect-SensitiveData $sensitive
    $roundTrip = ($protected | ConvertTo-Json -Depth 10) | ConvertFrom-Json -Depth 10
    $protectedText = $protected | ConvertTo-Json -Depth 10
    Check ($roundTrip.boolean.GetType() -eq [bool] -and $roundTrip.boolean) 'structured-redaction-preserves-Boolean'
    Check ($protectedText -notmatch '10\.23\.45\.67|2001:db8|2001:4860|device-canary|00:11:22:33:44:55|CANARY12345678|target-canary') 'structured-redaction-canaries'
    $script:PublicVersionLiterals = @('PLA-AL10 7.0.0.100(SP8C00E32R7P2)', 'SELFTEST-HDC-1.0')
    $versionRedaction = Protect-SensitiveText 'build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=192.0.2.44|port=8710'
    Check ($versionRedaction.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $versionRedaction.Contains('api=26') -and $versionRedaction -notmatch '192\.0\.2\.44|8710') 'redaction-preserves-build-api-and-removes-ip-port'
    $api26IpLike = Protect-SensitiveText 'full_system_build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=198.51.100.77|port=8710|bare=7.0.0.100'
    Check ($api26IpLike.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $api26IpLike.Contains('api=26') -and $api26IpLike -match 'bare=<REDACTED_IPV4>' -and $api26IpLike -notmatch '198\.51\.100\.77|8710') 'api26-build-ip-like-literal-preserved-real-ip-redacted'
    # ADJ-20260810-0001 (C6): the frozen HDC version is a legitimate public literal too - an
    # IP-like HDC version (e.g. 7.0.0.100) must survive Protect-SensitiveText exactly like the
    # build string, or the observed hdc_version in the confirmation record would be corrupted.
    $savedVersionLiterals = $script:PublicVersionLiterals
    $script:PublicVersionLiterals = @('HDC-7.0.0.100')
    $hdcVersionRedaction = Protect-SensitiveText 'hdc_version=HDC-7.0.0.100|peer=192.0.2.44|port=8710'
    Check ($hdcVersionRedaction.Contains('HDC-7.0.0.100') -and $hdcVersionRedaction -notmatch '192\.0\.2\.44|8710') 'redaction-preserves-hdc-version-literal'
    $script:PublicVersionLiterals = $savedVersionLiterals
    $arrayInput = [object[]]@('alpha', [object[]]@('beta', 'gamma'))
    $arrayJson = (Protect-SensitiveData $arrayInput) | ConvertTo-Json -Depth 10 -Compress
    Check ($arrayJson -eq '["alpha",["beta","gamma"]]') 'structured-redaction-array-shape'
    $endpointShape = Protect-SensitiveData ([ordered]@{ host = 'device-canary.example.test'; port = 8710; build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' })
    $endpointShapeJson = $endpointShape | ConvertTo-Json -Compress
    Check ($endpointShapeJson -notmatch 'device-canary|8710' -and $endpointShapeJson.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)')) 'structured-host-port-redaction'
    try { [void](Get-OptionalJsonBoolean ([pscustomobject]@{ hook = 'true' }) 'hook' $false); Check $false 'strict-simulation-Boolean' } catch { Check ($_.Exception.Message -match 'JSON Boolean') 'strict-simulation-Boolean' }
    Check ((Test-PhysicalTargetToken 'usb-target:8710') -and -not (Test-PhysicalTargetToken 'PHYS-1') -and -not (Test-PhysicalTargetToken 'two targets')) 'physical-target-validation'
    $script:ActualTarget = 'usb-target:8710'
    $auditArguments = Get-HdcInvocation 'BundleDump' @{ Bundle = $script:BundleA }
    $liveArguments = Get-LiveHdcArguments $auditArguments 'BundleDump' @{ Bundle = $script:BundleA }
    Check ('<PHYS_1_TARGET>' -in $auditArguments -and 'usb-target:8710' -notin $auditArguments -and 'usb-target:8710' -in $liveArguments -and '<PHYS_1_TARGET>' -notin $liveArguments) 'HDC-argv-placeholder-substitution'
    Check ((Protect-SensitiveText 'target=USB-TARGET:8710') -notmatch '(?i)usb-target:8710') 'target-case-variant-redaction'
    Check ((Get-FailureClassification 'HDC operation failed: InstallA exit=124 stderr=timeout').InfrastructureReason -eq 'hdc-usb-interruption') 'HDC-timeout-infrastructure-classification'
    Check ((Get-FailureClassification 'HDC operation failed: InstallA exit=125 stderr=HDC Process.Start exception').InfrastructureReason -eq 'hdc-usb-interruption') 'HDC-exit125-infrastructure-classification'
    $genericCaptureClass = Get-FailureClassification 'capture_degraded scenario-4'
    Check ([string]::IsNullOrEmpty([string]$genericCaptureClass.InfrastructureReason) -and -not $genericCaptureClass.RetryAuthorized) 'generic-capture-degraded-not-usb'
    Check ((Get-FailureClassification 'scenario-4 continuous capture infrastructure failure: capture process exited').InfrastructureReason -eq 'hdc-usb-interruption') 'capture-infra-failure-classification'
    $timeParseClass = Get-FailureClassification 'scenario-4 continuous capture non-infrastructure blocked: device-time-parse-failed'
    Check ([string]::IsNullOrEmpty([string]$timeParseClass.InfrastructureReason) -and -not $timeParseClass.RetryAuthorized) 'capture-timeparse-not-usb'
    Check ((Get-FailureClassification 'unable to start continuous campaign capture').InfrastructureReason -eq 'hdc-usb-interruption') 'capture-start-infrastructure-classification'
    Check ((Get-FailureClassification 'scenario-2 machine-precondition-blocked step=1 reason=hdc-usb-interruption:process-check:cn.alfadb.netbird.e3physvpna').InfrastructureReason -eq 'hdc-usb-interruption') 'machine-precondition-blocked-infra-classification'
    $preMismatchClass = Get-FailureClassification 'scenario-2 machine-precondition-blocked step=1 reason=process-state-mismatch:cn.alfadb.netbird.e3physvpna expected-active=False actual-active=True'
    Check ($preMismatchClass.Overall -eq 'blocked' -and [string]::IsNullOrEmpty([string]$preMismatchClass.InfrastructureReason) -and -not $preMismatchClass.RetryAuthorized) 'machine-precondition-blocked-mismatch-no-infra'
    Check ((Get-FailureClassification 'System.IO.IOException: disk full while writing capture').InfrastructureReason -eq 'collection-storage-failure') 'storage-ioexception-classification'
    Check ((Get-FailureClassification 'access denied writing evidence manifest').InfrastructureReason -eq 'collection-storage-failure') 'storage-access-denied-classification'
    Check ((Get-FailureClassification 'RUNNER_HOST_FAILURE cleanup threw').InfrastructureReason -eq 'runner-host-failure') 'runner-host-prefix-classification'
    $unknownClass = Get-FailureClassification 'unexpected boom without known prefix'
    Check ($unknownClass.Overall -eq 'blocked' -and [string]::IsNullOrEmpty([string]$unknownClass.InfrastructureReason) -and -not $unknownClass.RetryAuthorized) 'unknown-exception-no-retry'
    Check ((Get-FailureClassification 'FUNCTIONAL_FAIL scenario-1 FINAL HAP A install rejected').Overall -eq 'fail') 'functional-fail-classification'
    $dumpBlockedClass = Get-FailureClassification 'scenario-1 FINAL HAP A install confirmation blocked: bundle-dump-absent'
    Check ($dumpBlockedClass.Overall -eq 'blocked' -and [string]::IsNullOrEmpty([string]$dumpBlockedClass.InfrastructureReason) -and -not $dumpBlockedClass.RetryAuthorized) 'install-dump-blocked-no-retry'
    $installWarnClass = Get-FailureClassification 'scenario-1 FINAL HAP A install outcome blocked: install-outcome-uncertain'
    Check ($installWarnClass.Overall -eq 'blocked' -and -not $installWarnClass.RetryAuthorized) 'install-uncertain-blocked-no-retry'
    try { [void](Get-HdcInvocation 'StagingProbe' @{}); Check $true 'StagingProbe-allowlisted' } catch { Check $false 'StagingProbe-allowlisted' }
    try { [void](Get-HdcInvocation 'StagingProbe' @{ Path = '/tmp/x' }); Check $false 'StagingProbe-rejects-parameters' } catch { Check ($_.Exception.Message -match 'does not accept parameter') 'StagingProbe-rejects-parameters' }
    Check ((Get-HdcInstallAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = 'install bundle successfully.'; Stderr = '' })).Status -eq 'pass') 'install-success-semantic'
    Check ((Get-HdcInstallAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = 'error: failed to execute your command.'; Stderr = '' })).Status -eq 'functional_fail') 'install-exit0-error-is-reject'
    Check ((Get-HdcInstallAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = 'SIMULATED_OK'; Stderr = '' })).Status -eq 'blocked') 'install-exit0-without-success-string-blocked'
    Check ((Get-HdcInstallAssessment ([pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = 'signature rejected' })).Status -eq 'functional_fail') 'install-signature-reject-is-fail'
    Check ((Get-HdcInstallAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = "install bundle successfully.`nwarning: prior cache error cleared"; Stderr = '' })).Status -eq 'pass') 'install-success-with-unrelated-warning-pass'
    Check ((Get-BundleDumpAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = '{ "app": { "bundleName": "' + $script:BundleA + '" } }'; Stderr = '' }) $script:BundleA).Status -eq 'pass') 'bundle-dump-present-semantic'
    Check ((Get-BundleDumpAssessment ([pscustomobject]@{ ExitCode = 0; Stdout = 'error: failed to get information and the parameters may be wrong.'; Stderr = '' }) $script:BundleA).Status -eq 'blocked') 'bundle-dump-absent-blocked'
    Check ((Get-BundleDumpAssessment ([pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = 'Permission denied' }) $script:BundleA).Status -eq 'blocked') 'bundle-dump-permission-blocked'
    Check ((Test-StagingAbsent ([pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = "ls: $($script:Staging): No such file or directory" }))) 'staging-absent-semantic'
    Check (-not (Test-StagingAbsent ([pscustomobject]@{ ExitCode = 0; Stdout = "drwxrwxrwx 3 shell shell 4096 $($script:Staging)"; Stderr = '' }))) 'staging-present-not-absent'
    Check (-not (Test-StagingAbsent ([pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = "ls: cannot access '$($script:Staging)': Permission denied" }))) 'staging-cannot-access-is-residual'
    Check ((Get-RequestIdFromEvents @() $script:BundleA) -eq $null) 'empty-events-requestid-legal'
    Check ((Get-DenyAssessment @() $script:BundleB $null $false $false).result -eq 'blocked') 'empty-events-deny-blocked'
    Check ((Get-DestroyAssessment @() $script:BundleA $null).result -eq 'blocked') 'empty-events-destroy-blocked'
    $issuedOnlyEvents = @(
        [pscustomobject]@{ text = "VPN_DESTROY_ISSUED|requestId=x-issued" },
        [pscustomobject]@{ text = "VPN_DESTROY_BEGIN|requestId=x-issued|trigger=onDestroy" },
        [pscustomobject]@{ text = "VPN_FD_SNAPSHOT|requestId=x-issued|phase=pre-destroy|marker=PRE_DESTROY_OPEN" }
    )
    $issuedOnlyAssessment = Get-DestroyAssessment $issuedOnlyEvents $script:BundleA 'x-issued'
    Check ($issuedOnlyAssessment.result -eq 'blocked' -and $issuedOnlyAssessment.reason -eq 'destroy-terminal-or-post-snapshot-missing') 'destroy-issued-or-pre-only-not-pass'
    $terminalOnlyEvents = @(
        [pscustomobject]@{ text = "VPN_DESTROY_RESOLVED|requestId=x-term|fdMarker=FD_CLOSED_CONFIRMED" },
        [pscustomobject]@{ text = "VPN_FD_SNAPSHOT|requestId=x-term|phase=pre-destroy|marker=PRE_DESTROY_OPEN" }
    )
    Check ((Get-DestroyAssessment $terminalOnlyEvents $script:BundleA 'x-term').reason -eq 'post-destroy-snapshot-missing') 'destroy-terminal-without-post-snapshot-classified'
    $snapshotOnlyEvents = @(
        [pscustomobject]@{ text = "VPN_FD_SNAPSHOT|requestId=x-snap|phase=post-destroy-resolved|marker=FD_CLOSED_CONFIRMED" }
    )
    Check ((Get-DestroyAssessment $snapshotOnlyEvents $script:BundleA 'x-snap').reason -eq 'destroy-terminal-missing') 'post-snapshot-without-terminal-classified'
    $inferredStopEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-infer" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-infer' },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a-infer|fdMarker=FD_CLOSED_CONFIRMED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-infer|phase=post-destroy-resolved|marker=FD_CLOSED_CONFIRMED' }
    )
    $inferredStopInfo = Get-StopRequestFromEvents -Events $inferredStopEvents -ExpectedBundle $script:BundleA
    $inferredAssessment = Get-DestroyAssessment $inferredStopEvents $script:BundleA $null
    Check ($null -ne $inferredStopInfo -and $inferredStopInfo.RequestId -eq 'a-infer' -and $inferredStopInfo.Bundle -eq $script:BundleA) 'stop-request-inferred-from-events'
    Check ($inferredAssessment.result -eq 'pass' -and $inferredAssessment.reason -eq 'terminal-and-post-destroy-snapshot-confirmed') 'destroy-assessment-infers-requestId-instead-of-missing'
    $stopOnlyEvents = @([pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-stop-only" })
    Check ((Get-DestroyAssessment $stopOnlyEvents $script:BundleA $null).reason -eq 'destroy-terminal-or-post-snapshot-missing') 'stop-without-destroy-classified-not-requestId-missing'
    $destroyOnlyEvents = @(
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-destroy-only' },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a-destroy-only|fdMarker=FD_CLOSED_CONFIRMED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-destroy-only|phase=post-destroy-resolved|marker=FD_CLOSED_CONFIRMED' }
    )
    Check ((Get-StopRequestFromEvents -Events $destroyOnlyEvents -ExpectedBundle $script:BundleA) -eq $null) 'destroy-only-not-a-stop-request'
    Check ((Get-DestroyAssessment $destroyOnlyEvents $script:BundleA $null).reason -eq 'destroy-requestId-unresolved') 'destroy-only-without-stop-unresolved'
    $wrongBundleEvents = @([pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleB)|requestId=b-wrong" })
    Check ((Get-StopRequestFromEvents -Events $wrongBundleEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-wrong-bundle-rejected'
    $multiCandidateEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-one" },
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-two" }
    )
    Check ((Get-StopRequestFromEvents -Events $multiCandidateEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-multi-candidate-rejected'
    $crossIdConflictEvents = @(
        [pscustomobject]@{ text = "STOP_PROMISE_RESOLVED|bundle=$($script:BundleA)|requestId=a-promise" },
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-ui" }
    )
    Check ((Get-StopRequestFromEvents -Events $crossIdConflictEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-ui-promise-cross-id-conflict-null'
    $sameIdUiPromiseEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-same" },
        [pscustomobject]@{ text = "STOP_PROMISE_RESOLVED|bundle=$($script:BundleA)|requestId=a-same" }
    )
    $sameIdUiPromise = Get-StopRequestFromEvents -Events $sameIdUiPromiseEvents -ExpectedBundle $script:BundleA
    Check ($null -ne $sameIdUiPromise -and $sameIdUiPromise.RequestId -eq 'a-same') 'stop-request-same-id-ui-and-promise-allowed'
    $sessionReleasedOnly = @([pscustomobject]@{ text = "STOP_SESSION_RELEASED|bundle=$($script:BundleA)|requestId=a-session|reason=ability-not-found" })
    Check ((Get-StopRequestFromEvents -Events $sessionReleasedOnly -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-session-released-not-stop'
    $latePromiseOnly = @([pscustomobject]@{ text = "STOP_PROMISE_LATE_RESOLVED|bundle=$($script:BundleA)|requestId=a-late" })
    Check ((Get-StopRequestFromEvents -Events $latePromiseOnly -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-late-promise-not-stop'
    $timestampPrefixed = @([pscustomobject]@{ text = "CST 2026-07-17 16:54:59.204 UI_STOP|bundle=$($script:BundleA)|requestId=a-ts" })
    $timestampPrefixedStop = Get-StopRequestFromEvents -Events $timestampPrefixed -ExpectedBundle $script:BundleA
    Check ($null -ne $timestampPrefixedStop -and $timestampPrefixedStop.RequestId -eq 'a-ts') 'stop-request-timestamp-prefix-allowed'
    $skippedOnlyEvents = @([pscustomobject]@{ text = "UI_STOP_SKIPPED|bundle=$($script:BundleA)|reason=no-active-request" })
    Check ((Get-StopRequestFromEvents -Events $skippedOnlyEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-ui-stop-skipped-not-stop'
    $summaryMentionEvents = @([pscustomobject]@{ text = "W A01b00/SCB: summary UI_STOP|bundle=$($script:BundleA)|requestId=a-summary was observed" })
    Check ((Get-StopRequestFromEvents -Events $summaryMentionEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-summary-text-mention-rejected'
    $trailingProseEvents = @([pscustomobject]@{ text = "expected UI_STOP|bundle=$($script:BundleA)|requestId=a-err but got UI_STOP_SKIPPED" })
    Check ((Get-StopRequestFromEvents -Events $trailingProseEvents -ExpectedBundle $script:BundleA) -eq $null) 'stop-request-trailing-prose-rejected'
    $trailingFieldEvents = @([pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-extra|extra=1" })
    $trailingFieldStop = Get-StopRequestFromEvents -Events $trailingFieldEvents -ExpectedBundle $script:BundleA
    Check ($null -ne $trailingFieldStop -and $trailingFieldStop.RequestId -eq 'a-extra') 'stop-request-trailing-field-allowed'
    $cstParse = Parse-HilogDeviceTime 'CST 2026-07-17 16:54:59.204  1604  1660 W A01b00/SCB: sample'
    Check ($cstParse.Ok -and $cstParse.DeviceTimeZone -eq 'CST' -and $cstParse.DeviceObservedAt -match '\+08:00') 'CST-zone-parse'
    $offsetParse = Parse-HilogDeviceTime '2026-07-17 16:54:59.204+08:00 UI_START|bundle=x|requestId=y'
    Check ($offsetParse.Ok -and $offsetParse.DeviceObservedAt -match '2026-07-17') 'offset-zone-parse'
    $unknownZone = Parse-HilogDeviceTime 'XYZ 2026-07-17 16:54:59.204 sample'
    Check ((-not $unknownZone.Ok) -and $unknownZone.Reason -match 'unknown-device-time-zone') 'unknown-zone-blocked'
    $jsonRedacted = Protect-SensitiveText '{"udid":"ABCDEF1234567890","deviceIds":["DEV-1"],"endpoint":"10.1.2.3:8710","build":"PLA-AL10 7.0.0.100(SP8C00E32R7P2)"}'
    Check ($jsonRedacted -match '"udid":"<REDACTED>"' -and $jsonRedacted -match 'deviceIds' -and $jsonRedacted -match '<REDACTED>' -and $jsonRedacted.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $jsonRedacted -notmatch 'ABCDEF1234567890|10\.1\.2\.3') 'quoted-json-udid-deviceids-redaction'

    # --- ADJ-20260807-0003 host process terminal probe unit checks ---
    $absentPidResult = [pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = '' }
    $absentPidExit0 = [pscustomobject]@{ ExitCode = 0; Stdout = ''; Stderr = '' }
    $presentPidResult = [pscustomobject]@{ ExitCode = 0; Stdout = '12345'; Stderr = '' }
    $errorPidResult = [pscustomobject]@{ ExitCode = 124; Stdout = ''; Stderr = 'timeout' }
    $unknownPidResult = [pscustomobject]@{ ExitCode = 2; Stdout = ''; Stderr = '' }
    $pidExit1WithStderrBundle = [pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = "pidof: $($script:BundleA): Permission denied" }
    $pidExit2 = [pscustomobject]@{ ExitCode = 2; Stdout = ''; Stderr = '' }
    $pidGarbage = [pscustomobject]@{ ExitCode = 0; Stdout = 'not-a-pid-line garbage'; Stderr = 'warn' }
    $probeDumpPass = [pscustomobject]@{ ExitCode = 0; Stdout = '{ "app": { "bundleName": "' + $script:BundleA + '" } }'; Stderr = '' }
    $probeDumpAbsentExit0 = [pscustomobject]@{ ExitCode = 0; Stdout = 'error: failed to get information and the parameters may be wrong.'; Stderr = '' }
    $probeDumpAbsentExit1 = [pscustomobject]@{ ExitCode = 1; Stdout = 'error: failed to get information and the parameters may be wrong.'; Stderr = '' }
    $probeDumpPermission = [pscustomobject]@{ ExitCode = 0; Stdout = 'Permission denied'; Stderr = '' }
    $probeDumpStderr = [pscustomobject]@{ ExitCode = 0; Stdout = '{ "app": { "bundleName": "' + $script:BundleA + '" } }'; Stderr = 'Permission denied' }
    $probeDumpExit2 = [pscustomobject]@{ ExitCode = 2; Stdout = 'garbage'; Stderr = '' }
    $probeDumpGarbage = [pscustomobject]@{ ExitCode = 0; Stdout = 'totally-unrelated-output'; Stderr = '' }
    $probeDumpInfra = [pscustomobject]@{ ExitCode = 124; Stdout = ''; Stderr = 'timeout' }
    $probeAbsentWithBundle = Get-ProcessProbeStatus $absentPidResult $probeDumpPass $script:BundleA
    Check ($probeAbsentWithBundle.status -eq 'absent' -and $probeAbsentWithBundle.bundle_present) 'probe-absent-with-bundle-present'
    Check ((Get-ProcessProbeStatus $absentPidExit0 $probeDumpPass $script:BundleA).status -eq 'absent') 'probe-absent-pid-exit0-blank'
    # Non-pass BundleDump never accumulates as absent, even with clear "not found" text.
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpAbsentExit0 $script:BundleA).status -eq 'unknown') 'probe-dump-absent-exit0-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpAbsentExit1 $script:BundleA).status -eq 'unknown') 'probe-dump-absent-exit1-unknown'
    Check ((Get-ProcessProbeStatus $presentPidResult $probeDumpPass $script:BundleA).status -eq 'present') 'probe-present-classified'
    Check ((Get-ProcessProbeStatus $errorPidResult $probeDumpPass $script:BundleA).status -eq 'error') 'probe-pid-error-classified'
    Check ((Get-ProcessProbeStatus $unknownPidResult $probeDumpPass $script:BundleA).status -eq 'unknown') 'probe-pid-unknown-classified'
    Check ((Get-ProcessProbeStatus $pidExit1WithStderrBundle $probeDumpPass $script:BundleA).status -eq 'unknown') 'probe-pid-exit1-stderr-bundle-unknown'
    Check ((Get-ProcessProbeStatus $pidExit2 $probeDumpPass $script:BundleA).status -eq 'unknown') 'probe-pid-exit2-unknown'
    Check ((Get-ProcessProbeStatus $pidGarbage $probeDumpPass $script:BundleA).status -eq 'unknown') 'probe-pid-garbage-stderr-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpPermission $script:BundleA).status -eq 'unknown') 'probe-dump-permission-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpStderr $script:BundleA).status -eq 'unknown') 'probe-dump-stderr-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpExit2 $script:BundleA).status -eq 'unknown') 'probe-dump-exit2-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpGarbage $script:BundleA).status -eq 'unknown') 'probe-dump-garbage-unknown'
    Check ((Get-ProcessProbeStatus $absentPidResult $probeDumpInfra $script:BundleA).status -eq 'error') 'probe-dump-error-classified'

    # --- C7/ADJ-20260808-0001 process probe target and scheduling checks ---
    # pidof must target the <bundle>:vpn Extension ability process exactly; no broad process query.
    $pidAudit = Get-HdcInvocation 'PidOf' @{ Bundle = $script:BundleA }
    $pidJoined = $pidAudit -join ' '
    Check ($pidAudit[-1] -eq "$($script:BundleA):vpn" -and $pidJoined -match ' pidof [^ ]+:vpn$' -and $pidJoined -notmatch '(^|\s)(ps|pgrep|process|proc)(\s|$)') 'pidof-targets-bundle-vpn-extension-process'
    Check ((Get-HdcInvocation 'PidOf' @{ Bundle = $script:BundleB })[-1] -eq "$($script:BundleB):vpn") 'pidof-b-target-extension-process'
    # Scheduling must round-trip the exact DateTimeOffset (never [string], which drops sub-second)
    # and add a small margin so the recorded spacing actually reaches the frozen 3.0s rule.
    $lastProbeExact = [DateTimeOffset]::Parse('2099-01-01T00:00:03.900+00:00')
    $nextProbeExact = ([DateTimeOffset]$lastProbeExact).AddSeconds(3.0 + 0.1)
    Check (($nextProbeExact - $lastProbeExact).TotalSeconds -ge 3.0 -and $nextProbeExact.Millisecond -eq 0) 'probe-scheduling-keeps-subsecond-and-margin'
    # Operator wait state carries only mechanical action and machine-verification fields.
    $waitStateTemp = Join-Path ([IO.Path]::GetTempPath()) ('e3-waitstate-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($waitStateTemp) | Out-Null
    $savedEvidencePath = $script:EvidencePath
    try {
        $script:EvidencePath = $waitStateTemp
        Write-OperatorWaitState 'waiting' -Scenario 4 -StepIndex 2 -StepId 'abc123' -ExpectedAction '点击 Deny' -CaptureBefore ([ordered]@{ status = 'collected' }) -MachinePrecondition ([ordered]@{ status = 'pass' })
        $waitingJson = Get-Content -LiteralPath (Join-Path $waitStateTemp 'operator-wait-state.json') -Raw
        Check ($waitingJson -match '"phase"\s*:\s*"waiting"' -and $waitingJson -match '"scenario"\s*:\s*4' -and $waitingJson -match '"step_index"\s*:\s*2' -and $waitingJson -match '"step_id"\s*:\s*"abc123"' -and $waitingJson -match '"expected_action"' -and $waitingJson -match '"capture_before"' -and $waitingJson -match '"machine_precondition"' -and $waitingJson -match '"updated_at"') 'wait-state-waiting-shape'
        Write-OperatorWaitState 'complete'
        $completeJson = Get-Content -LiteralPath (Join-Path $waitStateTemp 'operator-wait-state.json') -Raw
        Check ($completeJson -match '"phase"\s*:\s*"complete"' -and $completeJson -match '"complete"\s*:\s*true' -and $completeJson -match '"completed_at"' -and $completeJson -match '"history"') 'wait-state-complete-shape'
        Check ($completeJson -notmatch '(?i)udid|\bserial\b|target|hap[_-]?[ab]|endpoint|secret|token|password') 'wait-state-no-sensitive-keys'
    } finally {
        $script:EvidencePath = $savedEvidencePath
        Remove-Item -LiteralPath $waitStateTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
    # Aggregation s3_clean_reactivation_proof is tri-state: true/false only when S3 was measured;
    # null (not false) when S3 was never probed/measured (blocked scenarios carry no proof key).
    $aggBlockedScenarios = New-BlockedScenarios 'fixture'
    Check (-not $aggBlockedScenarios[2].Contains('clean_reactivation_proof')) 'blocked-s3-entry-has-no-proof-key'

    function New-TestProbeState {
        param([object[]]$Probes = @(), [bool]$Started = $true, [bool]$Aborted = $false, [bool]$Terminal = $false, [int]$ConsecutiveAbsent = 0, [bool]$BundlePresent = $true)
        $list = [Collections.Generic.List[object]]::new()
        foreach ($p in @($Probes)) { $list.Add($p) }
        return [pscustomobject]@{
            Scenario = 0; Bundle = $script:BundleA; RequireBundlePresent = $false
            RequiredCount = 2; SpacingSeconds = 3.0
            Started = $Started; Finished = ($Terminal -or $Aborted); Aborted = $Aborted; Terminal = $Terminal
            ConsecutiveAbsent = $ConsecutiveAbsent; BundlePresent = $BundlePresent
            Probes = $list; LastProbeAt = $null; OverrideProbeIndex = 0
        }
    }
    $probeT0 = '2099-01-01T00:00:00+00:00'
    $probeT1 = '2099-01-01T00:00:03+00:00'
    $fallbackEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-fb" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-fb' },
        [pscustomobject]@{ text = 'VPN_DESTROY_BEGIN|requestId=a-fb|trigger=onDestroy' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-fb|phase=pre-destroy|marker=PRE_DESTROY_OPEN' }
    )
    $goodProbes = New-TestProbeState -Probes @(
        [ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 },
        [ordered]@{ time = $probeT1; status = 'absent'; bundle_present = $true; consecutive_absent = 2 }
    ) -Terminal $true -ConsecutiveAbsent 2 -BundlePresent $true
    $fallbackPass = Get-VpnFinalState -Events $fallbackEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($fallbackPass.result -eq 'pass' -and $fallbackPass.terminal_mode -eq 'strict-process-boundary' -and $fallbackPass.reason -eq 'strict-process-boundary-terminal') 'strict-fallback-pass-with-probes'
    $callbackFirstEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-cb" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-cb' },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a-cb|fdMarker=FD_CLOSED_CONFIRMED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-cb|phase=post-destroy-resolved|marker=FD_CLOSED_CONFIRMED' }
    )
    $callbackFirst = Get-VpnFinalState -Events $callbackFirstEvents -Bundle $script:BundleA -RequestId $null -ProbeState $null
    Check ($callbackFirst.result -eq 'pass' -and $callbackFirst.terminal_mode -eq 'callback-post-fd') 'callback-terminal-preferred-over-fallback'
    $callbackNoOnDestroyEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-no-od" },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a-no-od|fdMarker=FD_CLOSED_CONFIRMED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-no-od|phase=post-destroy-resolved|marker=FD_CLOSED_CONFIRMED' }
    )
    Check ((Get-DestroyAssessment $callbackNoOnDestroyEvents $script:BundleA 'a-no-od').reason -eq 'destroy-ondestroy-missing') 'destroy-assessment-requires-same-request-ondestroy'
    $callbackNoOnDestroy = Get-VpnFinalState -Events $callbackNoOnDestroyEvents -Bundle $script:BundleA -RequestId 'a-no-od' -ProbeState $null
    Check ($callbackNoOnDestroy.result -eq 'blocked' -and $callbackNoOnDestroy.result -ne 'pass') 'callback-without-ondestroy-cannot-pass'
    $fdStillOpenEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-fd" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-fd' },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a-fd|fdMarker=FD_STILL_OPEN' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a-fd|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN' }
    )
    $fdStillOpen = Get-VpnFinalState -Events $fdStillOpenEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($fdStillOpen.result -eq 'fail' -and $fdStillOpen.reason -eq 'fd-still-open-after-destroy' -and $fdStillOpen.terminal_mode -eq 'callback-post-fd') 'fd-still-open-fail-not-overridable-by-probes'
    $oneAbsentProbes = New-TestProbeState -Probes @([ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 }) -ConsecutiveAbsent 1
    $oneAbsent = Get-VpnFinalState -Events $fallbackEvents -Bundle $script:BundleA -RequestId $null -ProbeState $oneAbsentProbes -RequireBundlePresent $true
    Check ($oneAbsent.result -eq 'blocked' -and $oneAbsent.reason -eq 'strict-fallback-process-absent-insufficient') 'strict-fallback-one-absent-blocked'
    $closeProbes = New-TestProbeState -Probes @(
        [ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 },
        [ordered]@{ time = '2099-01-01T00:00:01+00:00'; status = 'absent'; bundle_present = $true; consecutive_absent = 2 }
    ) -Terminal $true -ConsecutiveAbsent 2
    $closeSpacing = Get-VpnFinalState -Events $fallbackEvents -Bundle $script:BundleA -RequestId $null -ProbeState $closeProbes -RequireBundlePresent $true
    Check ($closeSpacing.result -eq 'blocked' -and $closeSpacing.reason -eq 'strict-fallback-probe-spacing-insufficient') 'strict-fallback-spacing-insufficient-blocked'
    $presentMidProbes = New-TestProbeState -Probes @(
        [ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 },
        [ordered]@{ time = $probeT1; status = 'present'; bundle_present = $false; consecutive_absent = 0 },
        [ordered]@{ time = '2099-01-01T00:00:06+00:00'; status = 'absent'; bundle_present = $true; consecutive_absent = 1 }
    ) -ConsecutiveAbsent 1
    $presentMid = Get-VpnFinalState -Events $fallbackEvents -Bundle $script:BundleA -RequestId $null -ProbeState $presentMidProbes -RequireBundlePresent $true
    Check ($presentMid.result -eq 'blocked' -and $presentMid.reason -eq 'strict-fallback-process-absent-insufficient') 'strict-fallback-present-interspersed-blocked'
    $errorProbes = New-TestProbeState -Probes @([ordered]@{ time = $probeT0; status = 'error'; bundle_present = $false; consecutive_absent = 0 }) -Aborted $true
    $probeError = Get-VpnFinalState -Events $fallbackEvents -Bundle $script:BundleA -RequestId $null -ProbeState $errorProbes -RequireBundlePresent $true
    Check ($probeError.result -eq 'blocked' -and $probeError.reason -eq 'strict-fallback-probe-unknown-or-error') 'strict-fallback-probe-error-blocked'
    $wrongRequestEvents = @([pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleB)|requestId=b-wrong-fb" })
    $wrongRequest = Get-VpnFinalState -Events $wrongRequestEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($wrongRequest.result -eq 'blocked' -and $wrongRequest.reason -eq 'strict-fallback-stop-unique-missing') 'strict-fallback-wrong-request-blocked'
    $noOnDestroyEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-nod" },
        [pscustomobject]@{ text = 'VPN_DESTROY_BEGIN|requestId=a-nod|trigger=onDestroy' }
    )
    $noOnDestroy = Get-VpnFinalState -Events $noOnDestroyEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($noOnDestroy.result -eq 'blocked' -and $noOnDestroy.reason -eq 'strict-fallback-ondestroy-missing') 'strict-fallback-no-ondestroy-blocked'
    $noBeginEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-nob" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-nob' }
    )
    $noBegin = Get-VpnFinalState -Events $noBeginEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($noBegin.result -eq 'blocked' -and $noBegin.reason -eq 'strict-fallback-destroy-begin-missing') 'strict-fallback-no-begin-blocked'
    $issuedOnlyFinalEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a-iss" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a-iss' },
        [pscustomobject]@{ text = 'VPN_DESTROY_ISSUED|requestId=a-iss' }
    )
    $issuedOnlyFinal = Get-VpnFinalState -Events $issuedOnlyFinalEvents -Bundle $script:BundleA -RequestId $null -ProbeState $goodProbes -RequireBundlePresent $true
    Check ($issuedOnlyFinal.result -eq 'blocked' -and $issuedOnlyFinal.reason -eq 'strict-fallback-destroy-begin-missing') 'issued-never-counts-as-begin'
    $strictNoProof = @(
        [ordered]@{ scenario = 1; result = 'pass' }, [ordered]@{ scenario = 2; result = 'pass' },
        [ordered]@{ scenario = 3; result = 'pass'; terminal_mode = 'strict-process-boundary'; clean_reactivation_proof = $false },
        [ordered]@{ scenario = 4; result = 'pass' }, [ordered]@{ scenario = 5; result = 'pass' },
        [ordered]@{ scenario = 6; result = 'pass' }, [ordered]@{ scenario = 7; result = 'pass' }
    )
    Check ((Get-ScenarioAggregation $strictNoProof) -eq 'blocked') 's3-strict-fallback-without-reactivation-blocks-aggregation'
    $strictWithProof = @(
        [ordered]@{ scenario = 1; result = 'pass' }, [ordered]@{ scenario = 2; result = 'pass' },
        [ordered]@{ scenario = 3; result = 'pass'; terminal_mode = 'strict-process-boundary'; clean_reactivation_proof = $true },
        [ordered]@{ scenario = 4; result = 'pass' }, [ordered]@{ scenario = 5; result = 'pass' },
        [ordered]@{ scenario = 6; result = 'pass' }, [ordered]@{ scenario = 7; result = 'pass' }
    )
    Check ((Get-ScenarioAggregation $strictWithProof) -eq 'pass') 's3-strict-fallback-with-reactivation-passes-aggregation'

    # Capture degradation must never downgrade an explicit fail; aggregation keeps any fail first.
    $script:CaptureDegraded.Clear()
    $script:CaptureDegraded.Add([ordered]@{ scenario = 0; component = 'raw-hilog-process'; reason = 'simulated death'; category = 'non-infrastructure'; infrastructure_reason = $null })
    $degradeScenarios = @(
        [ordered]@{ scenario = 2; result = 'blocked'; reason = 'pre-existing-block' },
        [ordered]@{ scenario = 3; result = 'fail'; reason = 'fd-still-open-after-destroy' },
        [ordered]@{ scenario = 4; result = 'pass'; reason = 'clean-pass' }
    )
    Set-CaptureDegradedScenarios $degradeScenarios
    Check (([string]$degradeScenarios[1].result -eq 'fail' -and [string]$degradeScenarios[1].reason -eq 'fd-still-open-after-destroy') -and [string]$degradeScenarios[0].result -eq 'blocked' -and [string]$degradeScenarios[0].reason -eq 'capture-degraded' -and [string]$degradeScenarios[2].result -eq 'blocked') 'capture-degraded-never-overrides-fail'
    $script:CaptureDegraded.Clear()
    $aggFailFirst = @(
        [ordered]@{ scenario = 1; result = 'blocked' }, [ordered]@{ scenario = 2; result = 'blocked' },
        [ordered]@{ scenario = 3; result = 'fail'; terminal_mode = 'callback-post-fd' },
        [ordered]@{ scenario = 4; result = 'blocked' }, [ordered]@{ scenario = 5; result = 'blocked' },
        [ordered]@{ scenario = 6; result = 'blocked' }, [ordered]@{ scenario = 7; result = 'blocked' }
    )
    Check ((Get-ScenarioAggregation $aggFailFirst) -eq 'fail') 'aggregation-any-fail-priority-over-blocked'

    # S5 post-destroy FD_STILL_OPEN is a hard fail; pre-destroy open never fails.
    $s5PostDestroyOpenEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a5-fd" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a5-fd' },
        [pscustomobject]@{ text = 'VPN_DESTROY_RESOLVED|requestId=a5-fd|fdMarker=FD_STILL_OPEN' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a5-fd|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN' }
    )
    Check (Test-S5PostDestroyStillOpen $s5PostDestroyOpenEvents $script:BundleA 'a5-fd') 's5-post-destroy-fd-still-open-fail'
    $s5PreDestroyOpenEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a5-pre" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a5-pre' },
        [pscustomobject]@{ text = 'VPN_DESTROY_BEGIN|requestId=a5-pre|trigger=onDestroy' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a5-pre|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN' }
    )
    Check (-not (Test-S5PostDestroyStillOpen $s5PreDestroyOpenEvents $script:BundleA 'a5-pre')) 's5-pre-destroy-open-not-fail'
    Check (-not (Test-S5PostDestroyStillOpen $s5PreDestroyOpenEvents $script:BundleA 'a-other-request')) 's5-fd-still-open-request-correlated'
    Check (-not (Test-S5PostDestroyStillOpen @() $script:BundleA $null)) 's5-fd-still-open-null-request-not-fail'
    # Same-bundle/request marker only: correct request on the wrong bundle must never hit, and an
    # explicit matching bundle field must still hit.
    $s5WrongBundleOpenEvents = @(
        [pscustomobject]@{ text = "UI_STOP|bundle=$($script:BundleA)|requestId=a5-wb" },
        [pscustomobject]@{ text = 'VPN_ONDESTROY|requestId=a5-wb' },
        [pscustomobject]@{ text = "VPN_DESTROY_RESOLVED|bundle=$($script:BundleB)|requestId=a5-wb|fdMarker=FD_STILL_OPEN" },
        [pscustomobject]@{ text = "VPN_FD_SNAPSHOT|bundle=$($script:BundleB)|requestId=a5-wb|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    Check (-not (Test-S5PostDestroyStillOpen $s5WrongBundleOpenEvents $script:BundleA 'a5-wb')) 's5-fd-still-open-wrong-bundle-not-hit'
    Check (Test-S5PostDestroyStillOpen $s5WrongBundleOpenEvents $script:BundleB 'a5-wb') 's5-fd-still-open-explicit-bundle-must-equal'

    # Test-ProcessAbsentEvidence re-checks recorded probe timestamps; execution Wait is never trusted.
    Check ((Test-ProcessAbsentEvidence $goodProbes 2 3.0).Met) 'probe-evidence-two-absent-spaced-met'
    $evidenceOneAbsent = New-TestProbeState -Probes @([ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 }) -ConsecutiveAbsent 1
    Check ((Test-ProcessAbsentEvidence $evidenceOneAbsent 2 3.0).Reason -eq 'process-absent-probes-insufficient') 'probe-evidence-one-absent-insufficient'
    $evidenceCloseSpaced = New-TestProbeState -Probes @(
        [ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 },
        [ordered]@{ time = '2099-01-01T00:00:01+00:00'; status = 'absent'; bundle_present = $true; consecutive_absent = 2 }
    ) -Terminal $true -ConsecutiveAbsent 2
    Check ((Test-ProcessAbsentEvidence $evidenceCloseSpaced 2 3.0).Reason -eq 'probe-spacing-insufficient') 'probe-evidence-spacing-insufficient'
    $evidencePresentIntruder = New-TestProbeState -Probes @(
        [ordered]@{ time = $probeT0; status = 'absent'; bundle_present = $true; consecutive_absent = 1 },
        [ordered]@{ time = $probeT1; status = 'present'; bundle_present = $false; consecutive_absent = 0 },
        [ordered]@{ time = '2099-01-01T00:00:06+00:00'; status = 'absent'; bundle_present = $true; consecutive_absent = 1 }
    ) -ConsecutiveAbsent 1
    Check ((Test-ProcessAbsentEvidence $evidencePresentIntruder 2 3.0).Reason -eq 'process-absent-probes-insufficient') 'probe-evidence-present-interspersed-insufficient'
    $evidenceAborted = New-TestProbeState -Probes @([ordered]@{ time = $probeT0; status = 'error'; bundle_present = $false; consecutive_absent = 0 }) -Aborted $true
    Check ((Test-ProcessAbsentEvidence $evidenceAborted 2 3.0).Reason -eq 'probe-unknown-or-error') 'probe-evidence-aborted-blocked'
    $evidenceUnstarted = New-TestProbeState -Probes @() -Started $false
    Check ((Test-ProcessAbsentEvidence $evidenceUnstarted 2 3.0).Reason -eq 'probes-not-started') 'probe-evidence-not-started-blocked'

    # Test-PostCreateOpen matches the open=true field boundary only; reopen=true never hits.
    $postCreateOpenEvents = @(
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|reopen=true|marker=CREATE_ACCEPTED' },
        [pscustomobject]@{ text = 'VPN_FD_SNAPSHOT|requestId=a7|phase=post-create|open=truex|marker=CREATE_ACCEPTED' }
    )
    Check (Test-PostCreateOpen $postCreateOpenEvents $script:BundleA 'a5') 'post-create-open-exact-field'
    Check (-not (Test-PostCreateOpen $postCreateOpenEvents $script:BundleA 'a6')) 'post-create-reopen-not-open'
    Check (-not (Test-PostCreateOpen $postCreateOpenEvents $script:BundleA 'a7')) 'post-create-open-prefix-not-open'
    # Same-bundle/request marker only: correct request on the wrong bundle must never hit, and an
    # explicit matching bundle field must still hit.
    $postCreateWrongBundleEvents = @(
        [pscustomobject]@{ text = "VPN_FD_SNAPSHOT|bundle=$($script:BundleB)|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" }
    )
    Check (-not (Test-PostCreateOpen $postCreateWrongBundleEvents $script:BundleA 'a5')) 'post-create-open-wrong-bundle-not-hit'
    Check (Test-PostCreateOpen $postCreateWrongBundleEvents $script:BundleB 'a5') 'post-create-open-explicit-bundle-must-equal'

    $captureTemp = Join-Path ([IO.Path]::GetTempPath()) ('e3-capture-selftest-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($captureTemp) | Out-Null
    $stdoutPath = Join-Path $captureTemp 'campaign.log'
    $stderrPath = Join-Path $captureTemp 'campaign.stderr.log'
    [IO.File]::WriteAllText($stdoutPath, '', [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText($stderrPath, '', [Text.UTF8Encoding]::new($false))
    $capture = New-CampaignCaptureState $stdoutPath $stderrPath $null
    try {
        $stamp = (Get-Now).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        [IO.File]::AppendAllText($stdoutPath, "$stamp UI_START|bundle=$($script:BundleB)|requestId=partial", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        Check ($capture.Events.Count -eq 0 -and $capture.PendingBytes.Length -gt 0) 'incremental-reader-retains-partial-line'
        $partialAnchor = [long]$capture.ReadOffset
        [IO.File]::AppendAllText($stdoutPath, "|continued`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        $actionStart = (Get-Now).AddSeconds(-1)
        $actionEnd = (Get-Now).AddSeconds(1)
        $oldPartialEvents = @(Get-ScenarioWindowEvents $capture $partialAnchor $actionStart $actionEnd)
        Check ($capture.Events.Count -eq 1 -and $capture.PendingBytes.Length -eq 0 -and $oldPartialEvents.Count -eq 0) 'pre-anchor-partial-line-excluded'
        [IO.File]::AppendAllText($stdoutPath, "$stamp VPN_ONCREATE|bundle=$($script:BundleB)|requestId=partial", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        Check ($capture.Events.Count -eq 1 -and $capture.PendingBytes.Length -gt 0) 'last-poll-does-not-consume-half-line'
        [IO.File]::AppendAllText($stdoutPath, "`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        $newEvents = @(Get-ScenarioWindowEvents $capture $partialAnchor $actionStart $actionEnd)
        Check ($capture.Events.Count -eq 2 -and $capture.PendingBytes.Length -eq 0 -and @($newEvents | Where-Object { $_.text -match 'VPN_ONCREATE' }).Count -eq 1) 'partial-line-completion-and-final-read'

        # 3s device-clock lag still included; pre-anchor historical buffer excluded.
        $skewAction = [DateTimeOffset]::Parse((Get-Now).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture))
        $lagStamp = $skewAction.AddSeconds(-3).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        $oldStamp = $skewAction.AddSeconds(-30).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        [IO.File]::AppendAllText($stdoutPath, "$oldStamp OLD_BUFFER|bundle=$($script:BundleB)|requestId=old`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        $afterHistory = [long]$capture.ReadOffset
        [IO.File]::AppendAllText($stdoutPath, "$lagStamp UI_START|bundle=$($script:BundleB)|requestId=skew3`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        $skewEvents = @(Get-ScenarioWindowEvents $capture $afterHistory $skewAction $skewAction.AddSeconds(5))
        $historyLeak = @(Get-ScenarioWindowEvents $capture $afterHistory $skewAction $skewAction.AddSeconds(5) | Where-Object { $_.text -match 'OLD_BUFFER' })
        Check (@($skewEvents | Where-Object { $_.text -match 'requestId=skew3' }).Count -eq 1 -and $historyLeak.Count -eq 0) 'device-clock-skew-3s-includes-action-excludes-old-buffer'
        $cstLine = 'CST 2026-07-17 16:54:59.204  1  1 I A02900/E3: UI_START|bundle=' + $script:BundleA + '|requestId=cst1'
        [IO.File]::AppendAllText($stdoutPath, $cstLine + "`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        Check (@($capture.Events | Where-Object { $_.device_time_zone -eq 'CST' -and $_.device_observed_at -match '\+08:00' }).Count -ge 1) 'capture-stores-CST-zone'
        $script:InfrastructureReasonObserved = $null
        $script:CaptureDegraded.Clear()
        [IO.File]::WriteAllText($stderrPath, 'usb transport interrupted', [Text.UTF8Encoding]::new($false))
        [void](Test-CampaignCaptureHealth $capture)
        $stderrEntry = @($script:CaptureDegraded | Where-Object { $_.component -eq 'raw-hilog-stderr' })[0]
        Check ($script:InfrastructureReasonObserved -eq 'hdc-usb-interruption' -and $capture.Degraded -and [string]$stderrEntry.category -eq 'infrastructure') 'stderr-growth-sets-hdc-usb-interruption'
        $script:InfrastructureReasonObserved = $null
        $capture.Degraded = $false
        $capture.SimulatedDead = $true
        $script:CaptureDegraded.Clear()
        [void](Test-CampaignCaptureHealth $capture)
        $deathEntry = @($script:CaptureDegraded | Where-Object { $_.component -eq 'raw-hilog-process' })[0]
        Check ($script:InfrastructureReasonObserved -eq 'hdc-usb-interruption' -and [string]$deathEntry.category -eq 'infrastructure') 'process-exit-sets-hdc-usb-interruption'

        # Time-parse degradation is non-infrastructure and must not authorize USB retry by itself.
        $script:InfrastructureReasonObserved = $null
        $capture.Degraded = $false
        $capture.SimulatedDead = $false
        $script:CaptureDegraded.Clear()
        [IO.File]::AppendAllText($stdoutPath, "not-a-hilog-line-without-timestamp`n", [Text.UTF8Encoding]::new($false))
        Update-CampaignCapture $capture
        $timeEntry = @($script:CaptureDegraded | Where-Object { $_.component -eq 'raw-hilog-time-parse' })[0]
        Check ($null -ne $timeEntry -and [string]$timeEntry.category -eq 'non-infrastructure' -and [string]::IsNullOrEmpty([string]$script:InfrastructureReasonObserved)) 'timeparse-degradation-noninfra'

        # Fault artifact degradation records CaptureDegraded without marking continuous Capture.Degraded.
        $script:InfrastructureReasonObserved = $null
        $capture.Degraded = $false
        $script:CaptureDegraded.Clear()
        Check (-not (Test-FaultInfrastructureFailure ([pscustomobject]@{ ExitCode = 1; Stdout = ''; Stderr = 'Permission denied' }))) 'fault-permission-not-infra'
        Check (-not (Test-FaultInfrastructureFailure ([pscustomobject]@{ ExitCode = 127; Stdout = ''; Stderr = 'unknown command' }))) 'fault-unsupported-not-infra'
        Check ((Test-FaultInfrastructureFailure ([pscustomobject]@{ ExitCode = 124; Stdout = ''; Stderr = 'timeout' }))) 'fault-exit124-is-infra'
        Add-CaptureDegradation $capture 'FaultA' 'targeted fault artifact unavailable: FaultA-exit-1' -Scenario 7 -Category 'non-infrastructure' -MarkContinuousDegraded $false
        Check (-not $capture.Degraded -and [string]$script:CaptureDegraded[0].category -eq 'non-infrastructure' -and [string]::IsNullOrEmpty([string]$script:InfrastructureReasonObserved)) 'fault-permission-noninfra-no-continuous-degrade'
        $script:CaptureDegraded.Clear()
        Add-CaptureDegradation $capture 'FaultA' 'targeted fault artifact unavailable: FaultA-exit-124' -Scenario 7 -Category infrastructure -InfrastructureReason 'hdc-usb-interruption' -MarkContinuousDegraded $false
        Check (-not $capture.Degraded -and $script:InfrastructureReasonObserved -eq 'hdc-usb-interruption') 'fault-infra-no-continuous-degrade'
    } finally {
        Remove-Item -LiteralPath $captureTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
    $multiBCreate = @(
        [pscustomobject]@{ text = "UI_START|bundle=$($script:BundleB)|requestId=b-primary" },
        [pscustomobject]@{ text = "UI_START|bundle=$($script:BundleB)|requestId=b-secondary" },
        [pscustomobject]@{ text = "VPN_ONCREATE|bundle=$($script:BundleB)|requestId=b-secondary" }
    )
    Check ((Get-DenyAssessment $multiBCreate $script:BundleB 'b-primary' $true $true).result -eq 'fail') 'deny-any-B-requestId-create-fails'
    $rejectNeedsId = @(
        [pscustomobject]@{ text = "UI_START|bundle=$($script:BundleB)|requestId=b-rej" },
        [pscustomobject]@{ text = "START_PROMISE_REJECTED|bundle=$($script:BundleB)|requestId=b-rej" }
    )
    Check ((Get-DenyAssessment $rejectNeedsId $script:BundleB 'b-rej' $false $true).result -eq 'pass') 'deny-reject-pass-needs-requestId'
    Check ((Get-DenyAssessment $rejectNeedsId $script:BundleB $null $false $true).result -eq 'blocked') 'deny-reject-without-requestId-blocked'
    $pollutionEvents = @(
        [pscustomobject]@{ offset_seconds = 1; text = "UI_START|bundle=$($script:BundleB)|requestId=b4" },
        [pscustomobject]@{ offset_seconds = 2; text = "UI_START|bundle=$($script:BundleA)|requestId=a-noise" },
        [pscustomobject]@{ offset_seconds = 3; text = 'VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a-noise' },
        [pscustomobject]@{ offset_seconds = 4; text = "START_PROMISE_REJECTED|bundle=$($script:BundleB)|requestId=b4" }
    )
    Check ((Get-DenyAssessment $pollutionEvents $script:BundleB 'b4' $true $true).result -eq 'pass') 'deny-strict-B-requestId'
    $lateBCreate = $pollutionEvents + [pscustomobject]@{ offset_seconds = 62; text = 'VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4' }
    $coveredEvents = @($lateBCreate | Where-Object { $_.offset_seconds -le 65 })
    Check ((Get-DenyAssessment $coveredEvents $script:BundleB 'b4' $true $true).result -eq 'fail') 'ACK-plus-60-catches-late-B-create'
    # --- ADJ-20260808-0002 Get-RejectionErrorCode unit checks ---
    # Real Extension safeError comma-field shape: summary=code=<digits>,name=...,message=...
    Check ((Get-RejectionErrorCode 'VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with active VPN') -eq 2203002) 'rejection-code-real-summary-shape'
    # Historical top-level |code=<digits>| field.
    Check ((Get-RejectionErrorCode 'VPN_CREATE_REJECTED|requestId=b6|phase=conflict|code=2203001|summary=active-A') -eq 2203001) 'rejection-code-top-level-field'
    # safeError payload with the code not in the first comma field.
    Check ((Get-RejectionErrorCode 'VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=name=BusinessError,code=2203002,message=x') -eq 2203002) 'rejection-code-summary-mid-field'
    # A code quoted inside message=... prose must never be mis-parsed.
    Check ($null -eq (Get-RejectionErrorCode 'VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=name=BusinessError,message=look for code=2203002 in the log')) 'rejection-code-message-prose-ignored'
    # No code at all.
    Check ($null -eq (Get-RejectionErrorCode 'START_PROMISE_REJECTED|bundle=b|requestId=b6|summary=denied')) 'rejection-code-missing-null'
    # A standalone numeric key with a non-boundary suffix is not a code.
    Check ($null -eq (Get-RejectionErrorCode 'VPN_CREATE_REJECTED|requestId=b6|code=2203002x|summary=x')) 'rejection-code-prefix-suffix-ignored'
    $scenarioSet = New-BlockedScenarios 'fixture'
    Check ($scenarioSet.Count -eq 7 -and $scenarioSet[1].assertions.Count -eq 3) 'complete-seven-scenario-template'
    $integrityTemp = Join-Path ([IO.Path]::GetTempPath()) ('e3-integrity-selftest-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($integrityTemp) | Out-Null
    $savedProjection = $script:ProjectionTranscript
    try {
        $script:ProjectionTranscript = Join-Path $integrityTemp 'missing-transcript.jsonl'
        $missingCaptureScreen = Join-Path $integrityTemp 'missing.png'
        $missingCaptureLayout = Join-Path $integrityTemp 'missing.json'
        $script:CaptureArtifacts.Add([ordered]@{ scenario = 2; name = 'scenario-2-missing'; status = 'collected'; screen_path = $missingCaptureScreen; layout_path = $missingCaptureLayout })
        $badOrder = @($scenarioSet[1], $scenarioSet[0]) + @($scenarioSet[2..6])
        $integrityResults = @(Test-EvidenceIntegrity $integrityTemp $badOrder)
        Check ('hash-manifest-missing' -in $integrityResults) 'manifest-missing-invalidates'
        Check ('scenario-order-invalid' -in $integrityResults) 'scenario-order-invalidates'
        Check ('capture-reference-missing:scenario-2-missing' -in $integrityResults) 'capture-missing-invalidates'
        [IO.File]::WriteAllText($script:ProjectionTranscript, "{not-json}`n", [Text.UTF8Encoding]::new($false))
        Check ('transcript-json-invalid' -in @(Test-TranscriptIntegrity $script:ProjectionTranscript)) 'transcript-chain-invalidates'
    } finally {
        $script:CaptureArtifacts.Clear()
        $script:ProjectionTranscript = $savedProjection
        Remove-Item -LiteralPath $integrityTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
    # --- ADJ-20260810-0001 host-governed TargetBindingConfirm pure-function coverage ---
    $confirmPlan = @(Get-TargetBindingConfirmPlan)
    Check ($confirmPlan.Count -eq 3 -and (($confirmPlan | ForEach-Object { [string]$_.operation }) -join ',') -eq 'Version,TupleModel,TupleBuild') 'confirm-plan-exactly-three-whitelisted'
    $confirmForbidden = @('MkdirStaging', 'SendA', 'SendB', 'InstallA', 'InstallB', 'StartEntry', 'Uninstall', 'RemoveStaging', 'StagingProbe', 'FaultA', 'FaultB', 'HilogStream', 'ScreenCap', 'DumpLayout', 'ReceiveScreen', 'ReceiveLayout', 'BundleDump', 'PidOf', 'ForceStop')
    Check (@($confirmPlan | Where-Object { [string]$_.operation -in $confirmForbidden }).Count -eq 0) 'confirm-plan-no-install-cleanup-capture'
    foreach ($confirmStep in $confirmPlan) { [void](Get-HdcInvocation $confirmStep.operation $confirmStep.parameters) }
    Check $true 'confirm-plan-argv-allowlisted'
    $savedModeDryRun = [bool]$script:DryRun
    $savedModeLiveSimulation = [bool]$script:LiveSimulation
    $savedModeSelfTest = [bool]$script:SelfTest
    $savedModeConfirm = [bool]$script:TargetBindingConfirm
    $savedModeRecord = [string]$script:ConfirmationRecord
    $savedModeEvidenceRoot = [string]$script:EvidenceRoot
    $savedModeRawRoot = [string]$script:RawRoot
    $savedModeExecution = [string]$script:ExecutionMode
    try {
        $script:DryRun = $true; $script:LiveSimulation = $false; $script:SelfTest = $false; $script:TargetBindingConfirm = $true; $script:ConfirmationRecord = 'C:/outside/confirm.json'
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-exclusive-dryrun' } catch { Check ($_.Exception.Message -match 'mutually exclusive') 'confirm-mode-exclusive-dryrun' }
        $script:DryRun = $false; $script:LiveSimulation = $true
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-exclusive-livesim' } catch { Check ($_.Exception.Message -match 'mutually exclusive') 'confirm-mode-exclusive-livesim' }
        $script:LiveSimulation = $false; $script:SelfTest = $true
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-exclusive-selftest' } catch { Check ($_.Exception.Message -match 'mutually exclusive') 'confirm-mode-exclusive-selftest' }
        $script:SelfTest = $false; $script:ConfirmationRecord = ''
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-requires-record' } catch { Check ($_.Exception.Message -match 'requires ConfirmationRecord') 'confirm-mode-requires-record' }
        $script:TargetBindingConfirm = $false; $script:ConfirmationRecord = 'C:/outside/confirm.json'
        try { Assert-ModeExclusivity; Check $false 'record-only-valid-with-confirm-mode' } catch { Check ($_.Exception.Message -match 'only valid with TargetBindingConfirm') 'record-only-valid-with-confirm-mode' }
        # ADJ-20260810-0001 (C6): confirm mode explicitly rejects EvidenceRoot/RawRoot; it never
        # initializes campaign roots and must fail loudly instead of silently ignoring them.
        $script:TargetBindingConfirm = $true; $script:EvidenceRoot = 'C:/outside/evidence'
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-rejects-evidence-root' } catch { Check ($_.Exception.Message -match 'EvidenceRoot is not allowed') 'confirm-mode-rejects-evidence-root' }
        $script:EvidenceRoot = ''; $script:RawRoot = 'C:/outside/raw'
        try { Assert-ModeExclusivity; Check $false 'confirm-mode-rejects-raw-root' } catch { Check ($_.Exception.Message -match 'RawRoot is not allowed') 'confirm-mode-rejects-raw-root' }
        $script:RawRoot = ''
        $script:DryRun = $false; $script:TargetBindingConfirm = $true; $script:ConfirmationRecord = 'C:/outside/confirm.json'
        Assert-ModeExclusivity
        Check $true 'confirm-mode-alone-legal'
    } finally {
        $script:DryRun = $savedModeDryRun; $script:LiveSimulation = $savedModeLiveSimulation; $script:SelfTest = $savedModeSelfTest; $script:TargetBindingConfirm = $savedModeConfirm; $script:ConfirmationRecord = $savedModeRecord; $script:EvidenceRoot = $savedModeEvidenceRoot; $script:RawRoot = $savedModeRawRoot; $script:ExecutionMode = $savedModeExecution
    }
    # ADJ-20260810-0001 (C6): the consumer and producer contracts are exercised against a complete
    # fixture freeze with the fixed candidate pair and a matching independent review record.
    $script:RepoRoot = Get-GitRepositoryRoot
    $confirmTemp = Join-Path ([IO.Path]::GetTempPath()) ('e3-confirm-selftest-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($confirmTemp) | Out-Null
    $confirmRecordPath = Join-Path $confirmTemp 'target-binding-confirmation.json'
    $confirmFreezeBase = [ordered]@{
        schema_version = 2
        plan_status = 'ready'
        exception = 'E3-PHYS-PREFLIGHT'
        evidence_id = $script:CandidateEvidenceId
        campaign_id = $script:CandidateCampaignId
        attempt = 'initial'
        retry = [ordered]@{ basis = 'N/A'; infrastructure_reason = 'N/A'; prior_record_path = 'N/A'; prior_record_sha256 = 'N/A' }
        scenario_window_seconds = 60
        device_alias = 'PHYS-1'
        target_tuple = [ordered]@{ distribution = 'HarmonyOS'; device_model = 'PLA-AL10'; full_system_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'; api = '26'; kernel_arch = 'aarch64'; app_abi = 'arm64-v8a' }
        settings_reallow_expected_path = 'direct-system-activation'
        settings_reallow_path_policy = 'observation-only'
        settings_revoke_mechanism = 'settings-app-info-force-stop'
        settings_vpn_page_policy = 'observation-only'
        destroy_terminal_policy = 'callback-or-strict-process-boundary'
        process_absent_required_count = 2
        process_absent_probe_spacing_seconds = 3
        process_probe_target = '<bundle>:vpn'
        operator_trust_model = 'mechanical-action-only-machine-verified-v1'
        scenario_invalid_policy = 'stop-and-finally-cleanup-seal'
        layout_verification_profile = 'deterministic-layout-v1'
        vpn_conflict_rejection_codes = @(2203002)
        signing = [ordered]@{ type = 'ordinary-development'; device_in_profile = $true; device_in_profile_basis = 'selftest public verification basis'; public_fingerprint = 'SELFTEST-NON-SECRET'; verification_result = 'pass' }
        artifact_sha256 = [ordered]@{ hap_a = ('1' * 64); hap_b = ('2' * 64) }
        source = [ordered]@{ archive_path = 'C:/selftest/source.tar'; archive_sha256 = ('3' * 64); manifest_path = 'C:/selftest/manifest.json'; manifest_sha256 = ('4' * 64) }
        sdk = [ordered]@{ version = 'synthetic-6.1.1'; api = '24'; syscap_basis = 'synthetic public VPN SysCap basis'; files = @([ordered]@{ path = 'C:/selftest/sdk.bin'; sha256 = ('5' * 64) }) }
        hdc = [ordered]@{ version = 'SELFTEST-HDC-1.0'; sha256 = ('c' * 64) }
        runner_sha256 = ('b' * 64)
        code_sha = ('a' * 40)
        preflight_inputs_frozen_at = '2099-01-01T00:00:00+00:00'
        cleanup_baseline_frozen = $true
        collection_ready = $true
        independent_review_ready = $true
        operator_role = 'selftest-operator'
        independent_reviewer_role = 'selftest-independent-reviewer'
        independent_review_record = [ordered]@{ status = 'pending' }
    }
    # ADJ-20260810-0001 (C6): the fixture confirmation/review records bind the STABLE confirmation
    # contract (Get-ConfirmationContractSha256), not the full freeze contract: the full contract
    # includes preflight_inputs_frozen_at which legitimately advances between the blocked
    # confirmation freeze and the final ready freeze.
    $confirmationContractSha = Get-ConfirmationContractSha256 $confirmFreezeBase
    $confirmRecordFixture = [ordered]@{
        schema_version = 1
        record_kind = 'target-binding-confirmation'
        is_evidence = $false
        authorization_id = $script:AuthId
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $script:CandidateCampaignId
        evidence_id = $script:CandidateEvidenceId
        attempt = 'initial'
        retry = [ordered]@{ basis = 'N/A'; infrastructure_reason = 'N/A' }
        plan_status = 'ready'
        device_alias = 'PHYS-1'
        target_redacted = $true
        code_sha = ('a' * 40)
        runner_sha256 = ('b' * 64)
        freeze_manifest_sha256 = ('e' * 64)
        confirmation_contract_sha256 = $confirmationContractSha
        hdc_sha256 = ('c' * 64)
        hdc_version = 'SELFTEST-HDC-1.0'
        expected_model = 'PLA-AL10'
        expected_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
        observed_model = 'PLA-AL10'
        observed_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
        started_at = '2098-12-31T23:59:55+00:00'
        ended_at = '2098-12-31T23:59:59+00:00'
        command_attempted = 3
        command_completed = 3
        command_count = 3
        repository_fingerprint = ('g' * 64)
        verdict = 'pass'
        reason = 'N/A'
    }
    Write-JsonFile $confirmRecordPath $confirmRecordFixture
    $confirmRecordSha = Get-FileSha256 $confirmRecordPath
    [IO.File]::WriteAllText($confirmRecordPath + '.sha256', $confirmRecordSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    Check ((Get-Content -LiteralPath ($confirmRecordPath + '.sha256') -Raw).Trim() -eq (Get-FileSha256 $confirmRecordPath)) 'confirmation-record-sha-recompute'
    $reviewRecordPath = Join-Path $confirmTemp 'ready-freeze-review.json'
    $reviewFixture = [ordered]@{
        schema_version = 1
        record_kind = 'e3-ready-freeze-review'
        is_evidence = $false
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $script:CandidateCampaignId
        evidence_id = $script:CandidateEvidenceId
        code_sha = ('a' * 40)
        runner_sha256 = ('b' * 64)
        confirmation_contract_sha256 = $confirmationContractSha
        machine_confirmation_sha256 = $confirmRecordSha
        reviewer_role = 'selftest-independent-reviewer'
        operator_role = 'selftest-operator'
        verdict = 'pass'
        blockers = 0
        majors = 0
        started_at = '2098-12-31T23:59:59+00:00'
        ended_at = '2098-12-31T23:59:59+00:00'
    }
    Write-JsonFile $reviewRecordPath $reviewFixture
    $reviewSha = Get-FileSha256 $reviewRecordPath
    [IO.File]::WriteAllText($reviewRecordPath + '.sha256', $reviewSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    Check ((Get-Content -LiteralPath ($reviewRecordPath + '.sha256') -Raw).Trim() -eq (Get-FileSha256 $reviewRecordPath)) 'review-record-sha-recompute'
    function New-ConfirmFreeze {
        param([string]$PlanStatus, [hashtable]$WithConfirmation = $null, [hashtable]$WithReview = $null)
        $copy = (($confirmFreezeBase | ConvertTo-Json -Depth 20) | ConvertFrom-Json -Depth 20)
        $copy.plan_status = $PlanStatus
        # ADJ-20260810-0001 (C6): machine_fresh_confirmation does not exist on the base freeze; under
        # Set-StrictMode a direct assignment to the non-existent property would throw, so it must be
        # added with Add-Member -Force (independent_review_record already exists as status=pending).
        if ($null -ne $WithConfirmation) { Add-Member -InputObject $copy -NotePropertyName 'machine_fresh_confirmation' -NotePropertyValue $WithConfirmation -Force }
        if ($null -ne $WithReview) { $copy.independent_review_record = $WithReview }
        return $copy
    }
    $confirmOk = [ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $confirmRecordPath; record_sha256 = $confirmRecordSha }
    $confirmWrongAuth = [ordered]@{ status = 'pass'; authorization_id = 'AUTH-E3-PHYS1API26-20260810-9999'; record_path = $confirmRecordPath; record_sha256 = $confirmRecordSha }
    $confirmWrongSha = [ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $confirmRecordPath; record_sha256 = ('d' * 64) }
    $confirmPending = [ordered]@{ status = 'pending'; authorization_id = $script:AuthId; record_path = 'N/A'; record_sha256 = 'N/A' }
    $reviewOk = [ordered]@{ status = 'pass'; record_path = $reviewRecordPath; record_sha256 = $reviewSha; reviewer_role = 'selftest-independent-reviewer' }
    # ADJ-20260810-0001 (C6): JSON integer gate - Int32/Int64 positive, string/float/null negative.
    Check ((Test-JsonInteger ([int]3)) -and (Test-JsonInteger ([long]3)) -and -not (Test-JsonInteger '3') -and -not (Test-JsonInteger 3.0) -and -not (Test-JsonInteger $null) -and -not (Test-JsonInteger $true)) 'json-integer-helper-int64-positive-string-float-negative'
    $savedConfirmDryRun = [bool]$script:DryRun
    $savedConfirmLiveSimulation = [bool]$script:LiveSimulation
    $savedConfirmTargetBindingConfirm = [bool]$script:TargetBindingConfirm
    $savedConfirmExecution = [string]$script:ExecutionMode
    try {
        $script:DryRun = $false; $script:LiveSimulation = $false; $script:TargetBindingConfirm = $false; $script:ExecutionMode = 'live'
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready')); Check $false 'live-ready-requires-confirmation' } catch { Check $true 'live-ready-requires-confirmation' }
        $okResult = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmOk $reviewOk)
        Check ($null -ne $okResult -and [string]$okResult.RecordSha256 -eq $confirmRecordSha) 'live-ready-accepts-bound-pass-confirmation'
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmWrongAuth $reviewOk)); Check $false 'live-ready-rejects-wrong-authorization-id' } catch { Check ($_.Exception.Message -match 'authorization_id') 'live-ready-rejects-wrong-authorization-id' }
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmWrongSha $reviewOk)); Check $false 'live-ready-rejects-wrong-record-sha' } catch { Check ($_.Exception.Message -match 'mismatch|SHA-256') 'live-ready-rejects-wrong-record-sha' }
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked')); Check $false 'live-blocked-plan-rejected' } catch { Check $true 'live-blocked-plan-rejected' }
        $script:DryRun = $true; $script:ExecutionMode = 'dry-run'
        $null = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmPending)
        Check $true 'dryrun-blocked-allows-pending-confirmation'
        # ADJ-20260810-0001 (C6): a blocked DryRun that declares status=pass is FULLY validated
        # (a blocked DryRun can never hide a broken binding); pending is allowed and skipped.
        $blockedPassResult = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmOk)
        Check ($null -ne $blockedPassResult -and [string]$blockedPassResult.RecordSha256 -eq $confirmRecordSha) 'dryrun-blocked-pass-fully-validated'
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmWrongSha)); Check $false 'dryrun-blocked-pass-rejects-wrong-sha' } catch { Check ($_.Exception.Message -match 'SHA-256') 'dryrun-blocked-pass-rejects-wrong-sha' }
        # ADJ-20260810-0001 (C6): blocked DryRun review consistency - a declared-pass review can
        # never ride on a pending/absent machine confirmation, and a machine pass + review pass
        # pair is fully validated (ValidateDeclaredPass) exactly like a ready one; pending review
        # stays allowed and skipped.
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmPending $reviewOk)); Check $false 'dryrun-blocked-rejects-review-pass-on-pending-machine' } catch { Check ($_.Exception.Message -match 'machine_fresh_confirmation.status=pass') 'dryrun-blocked-rejects-review-pass-on-pending-machine' }
        $blockedReviewPassResult = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmOk $reviewOk)
        Check ($null -ne $blockedReviewPassResult -and [string]$blockedReviewPassResult.RecordSha256 -eq $confirmRecordSha) 'dryrun-blocked-machine-pass-review-pass-fully-validated'
        $pendingReviewBinding = [ordered]@{ status = 'pending'; record_path = 'N/A'; record_sha256 = 'N/A'; reviewer_role = 'selftest-independent-reviewer' }
        $null = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmOk $pendingReviewBinding)
        Check $true 'dryrun-blocked-allows-pending-review'
        $brokenReviewPath = Join-Path $confirmTemp 'neg-review-blocked-dryrun.json'
        $brokenReview = (($reviewFixture | ConvertTo-Json -Depth 20) | ConvertFrom-Json -Depth 20)
        $brokenReview.verdict = 'blocked'
        Write-JsonFile $brokenReviewPath $brokenReview
        $brokenReviewSha = Get-FileSha256 $brokenReviewPath
        [IO.File]::WriteAllText($brokenReviewPath + '.sha256', $brokenReviewSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $brokenReviewBinding = [ordered]@{ status = 'pass'; record_path = $brokenReviewPath; record_sha256 = $brokenReviewSha; reviewer_role = 'selftest-independent-reviewer' }
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'blocked' $confirmOk $brokenReviewBinding)); Check $false 'dryrun-blocked-review-pass-fully-validates-content' } catch { Check ($_.Exception.Message -match 'verdict') 'dryrun-blocked-review-pass-fully-validates-content' }
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready')); Check $false 'dryrun-ready-requires-confirmation' } catch { Check $true 'dryrun-ready-requires-confirmation' }
        $null = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmOk $reviewOk)
        Check $true 'dryrun-ready-accepts-bound-pass-confirmation'
        $script:DryRun = $false; $script:LiveSimulation = $true; $script:ExecutionMode = 'live-simulation'
        $null = Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready')
        Check $true 'livesim-keeps-existing-fixture-contract'
        # ADJ-20260810-0001 (C6): freeze-level negatives - the fixed candidate pair, initial
        # attempt, and retry N/A are enforced on the consumer side too, so the generic
        # infrastructure retry branch can never enter this AUTH path.
        $script:DryRun = $false; $script:LiveSimulation = $false; $script:ExecutionMode = 'live'
        $wrongPairFreeze = New-ConfirmFreeze 'ready' $confirmOk $reviewOk
        $wrongPairFreeze.campaign_id = 'E3-PHYS-PREFLIGHT-WRONG'
        try { [void](Assert-MachineFreshConfirmation $wrongPairFreeze); Check $false 'ready-consumer-rejects-wrong-freeze-campaign-pair' } catch { Check ($_.Exception.Message -match 'candidate pair') 'ready-consumer-rejects-wrong-freeze-campaign-pair' }
        $retryFreeze = New-ConfirmFreeze 'ready' $confirmOk $reviewOk
        $retryFreeze.attempt = 'infrastructure-blocked-retry-1'
        $retryFreeze.retry.basis = 'hdc-usb-interruption'
        $retryFreeze.retry.infrastructure_reason = 'hdc-usb-interruption'
        try { [void](Assert-MachineFreshConfirmation $retryFreeze); Check $false 'ready-consumer-rejects-retry-attempt' } catch { Check ($_.Exception.Message -match 'attempt|retry') 'ready-consumer-rejects-retry-attempt' }
        # ADJ-20260810-0001 (C6): anti-replication negatives - every independently replicable field
        # of the confirmation record is mutated and must be rejected by the consumer.
        function New-MutantRecord {
            param([scriptblock]$Mutate)
            $copy = (($confirmRecordFixture | ConvertTo-Json -Depth 20) | ConvertFrom-Json -Depth 20)
            & $Mutate $copy
            return $copy
        }
        function Write-MutantCase {
            param([string]$Name, $Record)
            $casePath = Join-Path $confirmTemp ("neg-$Name.json")
            Write-JsonFile $casePath $Record
            $caseSha = Get-FileSha256 $casePath
            [IO.File]::WriteAllText($casePath + '.sha256', $caseSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
            return [pscustomobject]@{ Path = $casePath; Sha256 = $caseSha }
        }
        function Test-RecordReject {
            param([string]$Name, [scriptblock]$Mutate, [string]$MessagePattern)
            $case = Write-MutantCase $Name (New-MutantRecord $Mutate)
            $freeze = New-ConfirmFreeze 'ready' ([ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $case.Path; record_sha256 = $case.Sha256 }) $reviewOk
            try { [void](Assert-MachineFreshConfirmation $freeze); Check $false $Name } catch { Check ($_.Exception.Message -match $MessagePattern) $Name }
        }
        Test-RecordReject 'consumer-rejects-campaign-id-mutation' { param($r) $r.campaign_id = 'E3-PHYS-PREFLIGHT-WRONG' } 'candidate IDs'
        Test-RecordReject 'consumer-rejects-evidence-id-mutation' { param($r) $r.evidence_id = 'EV-E3-WRONG-20990101-0001' } 'candidate IDs'
        Test-RecordReject 'consumer-rejects-code-sha-mutation' { param($r) $r.code_sha = ('f' * 40) } 'code_sha'
        Test-RecordReject 'consumer-rejects-runner-sha-mutation' { param($r) $r.runner_sha256 = ('9' * 64) } 'runner_sha256'
        Test-RecordReject 'consumer-rejects-hdc-sha-mutation' { param($r) $r.hdc_sha256 = ('8' * 64) } 'hdc_sha256'
        Test-RecordReject 'consumer-rejects-hdc-version-mutation' { param($r) $r.hdc_version = 'WRONG-HDC-9.9' } 'hdc_version'
        Test-RecordReject 'consumer-rejects-verdict-mutation' { param($r) $r.verdict = 'blocked' } 'verdict'
        Test-RecordReject 'consumer-rejects-is-evidence-mutation' { param($r) $r.is_evidence = $true } 'is_evidence'
        Test-RecordReject 'consumer-rejects-attempted-mutation' { param($r) $r.command_attempted = 2 } 'command_attempted'
        Test-RecordReject 'consumer-rejects-completed-mutation' { param($r) $r.command_completed = 2 } 'command_completed'
        Test-RecordReject 'consumer-rejects-attempted-string' { param($r) $r.command_attempted = '3' } 'command_attempted'
        Test-RecordReject 'consumer-rejects-completed-string' { param($r) $r.command_completed = '3' } 'command_completed'
        Test-RecordReject 'consumer-rejects-command-count-alias-mismatch' { param($r) $r.command_count = 2 } 'command_count'
        Test-RecordReject 'consumer-rejects-command-count-string' { param($r) $r.command_count = '3' } 'command_count'
        Test-RecordReject 'consumer-rejects-unknown-top-level-field' { param($r) Add-Member -InputObject $r -NotePropertyName 'canary_target' -NotePropertyValue 'usb-target:8710' } 'unknown top-level field'
        Test-RecordReject 'consumer-rejects-observed-build-mutation' { param($r) $r.observed_build = 'PLA-AL10 7.0.0.999(SP8C00E32R7P2)' } 'observed model/build'
        Test-RecordReject 'consumer-rejects-schema-mutation' { param($r) $r.schema_version = 2 } 'schema_version'
        Test-RecordReject 'consumer-rejects-reason-mutation' { param($r) $r.reason = 'drifted' } 'reason'
        Test-RecordReject 'consumer-rejects-attempt-field-mutation' { param($r) $r.attempt = 'infrastructure-blocked-retry-1' } 'attempt'
        Test-RecordReject 'consumer-rejects-retry-basis-mutation' { param($r) $r.retry.basis = 'hdc-usb-interruption' } 'retry'
        Test-RecordReject 'consumer-rejects-target-redacted-mutation' { param($r) $r.target_redacted = $false } 'target_redacted'
        Test-RecordReject 'consumer-rejects-device-alias-mutation' { param($r) $r.device_alias = 'PHYS-2' } 'device_alias'
        Test-RecordReject 'consumer-rejects-exception-mutation' { param($r) $r.exception = 'OTHER-EXCEPTION' } 'exception'
        Test-RecordReject 'consumer-rejects-contract-mutation' { param($r) $r.confirmation_contract_sha256 = ('0' * 64) } 'confirmation_contract_sha256'
        Test-RecordReject 'consumer-rejects-start-after-end' { param($r) $r.started_at = '2099-01-01T00:00:10+00:00' } 'started_at'
        Test-RecordReject 'consumer-rejects-ended-after-frozen' { param($r) $r.ended_at = '2099-01-01T00:00:30+00:00' } 'no later than freeze'
        # companion negatives: a lone record is never consumable, a wrong or non-hex companion is rejected.
        $loneRecordPath = Join-Path $confirmTemp 'neg-lone-record.json'
        Write-JsonFile $loneRecordPath $confirmRecordFixture
        $loneFreeze = New-ConfirmFreeze 'ready' ([ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $loneRecordPath; record_sha256 = (Get-FileSha256 $loneRecordPath) }) $reviewOk
        try { [void](Assert-MachineFreshConfirmation $loneFreeze); Check $false 'consumer-rejects-missing-companion' } catch { Check ($_.Exception.Message -match 'companion missing') 'consumer-rejects-missing-companion' }
        $badCompanionPath = Join-Path $confirmTemp 'neg-bad-companion.json'
        Write-JsonFile $badCompanionPath $confirmRecordFixture
        $badCompanionSha = Get-FileSha256 $badCompanionPath
        [IO.File]::WriteAllText($badCompanionPath + '.sha256', ('a' * 64) + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $badCompanionFreeze = New-ConfirmFreeze 'ready' ([ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $badCompanionPath; record_sha256 = $badCompanionSha }) $reviewOk
        try { [void](Assert-MachineFreshConfirmation $badCompanionFreeze); Check $false 'consumer-rejects-wrong-companion' } catch { Check ($_.Exception.Message -match 'does not match the record bytes') 'consumer-rejects-wrong-companion' }
        $nonHexCompanionPath = Join-Path $confirmTemp 'neg-nonhex-companion.json'
        Write-JsonFile $nonHexCompanionPath $confirmRecordFixture
        $nonHexSha = Get-FileSha256 $nonHexCompanionPath
        [IO.File]::WriteAllText($nonHexCompanionPath + '.sha256', 'not-a-sha' + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $nonHexFreeze = New-ConfirmFreeze 'ready' ([ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $nonHexCompanionPath; record_sha256 = $nonHexSha }) $reviewOk
        try { [void](Assert-MachineFreshConfirmation $nonHexFreeze); Check $false 'consumer-rejects-nonhex-companion' } catch { Check ($_.Exception.Message -match 'final SHA-256') 'consumer-rejects-nonhex-companion' }
        # path negatives: a missing record file is rejected before any content is read.
        $missingPath = Join-Path $confirmTemp 'neg-missing.json'
        $missingFreeze = New-ConfirmFreeze 'ready' ([ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $missingPath; record_sha256 = ('b' * 64) }) $reviewOk
        try { [void](Assert-MachineFreshConfirmation $missingFreeze); Check $false 'consumer-rejects-missing-record-file' } catch { Check ($_.Exception.Message -match 'file missing') 'consumer-rejects-missing-record-file' }
        # ADJ-20260810-0001 (C6): independent review record negatives (mechanical gate, not the
        # self-declared independent_review_ready boolean).
        function Test-ReviewReject {
            param([string]$Name, [scriptblock]$Mutate, [string]$MessagePattern)
            $copy = (($reviewFixture | ConvertTo-Json -Depth 20) | ConvertFrom-Json -Depth 20)
            & $Mutate $copy
            $casePath = Join-Path $confirmTemp ("neg-review-$Name.json")
            Write-JsonFile $casePath $copy
            $caseSha = Get-FileSha256 $casePath
            [IO.File]::WriteAllText($casePath + '.sha256', $caseSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
            $binding = [ordered]@{ status = 'pass'; record_path = $casePath; record_sha256 = $caseSha; reviewer_role = 'selftest-independent-reviewer' }
            $freeze = New-ConfirmFreeze 'ready' $confirmOk $binding
            try { [void](Assert-MachineFreshConfirmation $freeze); Check $false $Name } catch { Check ($_.Exception.Message -match $MessagePattern) $Name }
        }
        Test-ReviewReject 'consumer-rejects-review-verdict' { param($r) $r.verdict = 'blocked' } 'verdict'
        Test-ReviewReject 'consumer-rejects-review-blockers' { param($r) $r.blockers = 1 } 'blockers'
        Test-ReviewReject 'consumer-rejects-review-majors' { param($r) $r.majors = 1 } 'blockers'
        Test-ReviewReject 'consumer-rejects-review-blockers-string' { param($r) $r.blockers = '0' } 'blockers'
        Test-ReviewReject 'consumer-rejects-review-majors-string' { param($r) $r.majors = '0' } 'blockers'
        Test-ReviewReject 'consumer-rejects-review-exception' { param($r) $r.exception = 'OTHER-EXCEPTION' } 'exception'
        Test-ReviewReject 'consumer-rejects-review-unknown-field' { param($r) Add-Member -InputObject $r -NotePropertyName 'secret_target' -NotePropertyValue 'usb-target:8710' } 'unknown top-level field'
        Test-ReviewReject 'consumer-rejects-review-start-before-machine-end' { param($r) $r.started_at = '2098-12-31T23:59:30+00:00' } 'machine confirmation ended_at'
        Test-ReviewReject 'consumer-rejects-review-role' { param($r) $r.reviewer_role = 'some-other-reviewer' } 'reviewer_role'
        Test-ReviewReject 'consumer-rejects-review-machine-sha' { param($r) $r.machine_confirmation_sha256 = ('0' * 64) } 'machine_confirmation_sha256'
        Test-ReviewReject 'consumer-rejects-review-kind' { param($r) $r.record_kind = 'e3-ready-freeze-review-x' } 'record_kind'
        Test-ReviewReject 'consumer-rejects-review-is-evidence' { param($r) $r.is_evidence = $true } 'is_evidence'
        Test-ReviewReject 'consumer-rejects-review-contract' { param($r) $r.confirmation_contract_sha256 = ('0' * 64) } 'confirmation_contract_sha256'
        Test-ReviewReject 'consumer-rejects-review-code' { param($r) $r.code_sha = ('0' * 40) } 'code_sha'
        Test-ReviewReject 'consumer-rejects-review-runner' { param($r) $r.runner_sha256 = ('0' * 64) } 'runner_sha256'
        Test-ReviewReject 'consumer-rejects-review-campaign' { param($r) $r.campaign_id = 'E3-PHYS-PREFLIGHT-WRONG' } 'candidate IDs'
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmOk)); Check $false 'ready-requires-review-record' } catch { Check ($_.Exception.Message -match 'independent_review_record') 'ready-requires-review-record' }
        $pendingReview = [ordered]@{ status = 'pending'; record_path = 'N/A'; record_sha256 = 'N/A'; reviewer_role = 'selftest-independent-reviewer' }
        try { [void](Assert-MachineFreshConfirmation (New-ConfirmFreeze 'ready' $confirmOk $pendingReview)); Check $false 'ready-requires-pass-review-record' } catch { Check ($_.Exception.Message -match 'status must be pass') 'ready-requires-pass-review-record' }
        $loneReviewPath = Join-Path $confirmTemp 'neg-review-lone.json'
        Write-JsonFile $loneReviewPath $reviewFixture
        $loneReviewFreeze = New-ConfirmFreeze 'ready' $confirmOk ([ordered]@{ status = 'pass'; record_path = $loneReviewPath; record_sha256 = (Get-FileSha256 $loneReviewPath); reviewer_role = 'selftest-independent-reviewer' })
        try { [void](Assert-MachineFreshConfirmation $loneReviewFreeze); Check $false 'consumer-rejects-review-missing-companion' } catch { Check ($_.Exception.Message -match 'companion missing') 'consumer-rejects-review-missing-companion' }
        # ADJ-20260810-0001 (C6): two-phase positive - the blocked confirmation freeze freezes at
        # T1 BEFORE the machine confirmation runs, and the final ready freeze freezes at T2 AFTER
        # the confirmation and review end times. The full Get-FreezeContract hashes differ (the
        # governance/time field preflight_inputs_frozen_at advanced), but the stable confirmation
        # contract is byte-identical, so the confirmation/review records bound on the blocked
        # phase are consumable by the ready phase; the time gate (started<=ended<=frozen_at) is
        # checked against the FINAL ready freeze's preflight_inputs_frozen_at.
        $twoPhaseBlocked = New-ConfirmFreeze 'blocked'
        $twoPhaseBlocked.preflight_inputs_frozen_at = '2099-01-01T00:00:00+00:00'
        $twoPhaseReady = New-ConfirmFreeze 'ready'
        $twoPhaseReady.preflight_inputs_frozen_at = '2099-01-01T00:00:10+00:00'
        Check ((Get-FreezeContractSha256 $twoPhaseBlocked) -ne (Get-FreezeContractSha256 $twoPhaseReady)) 'two-phase-full-contract-hashes-differ'
        Check ((Get-ConfirmationContractSha256 $twoPhaseBlocked) -eq (Get-ConfirmationContractSha256 $twoPhaseReady)) 'two-phase-confirmation-contract-hash-identical'
        $twoPhaseRecord = New-TargetBindingConfirmationRecord $twoPhaseBlocked ('e' * 64) (Get-ConfirmationContractSha256 $twoPhaseBlocked) ([pscustomobject]@{ Fingerprint = 'g' * 64 }) ([DateTimeOffset]::Parse('2099-01-01T00:00:01+00:00')) ([DateTimeOffset]::Parse('2099-01-01T00:00:05+00:00')) 'pass' 'N/A' 'SELFTEST-HDC-1.0' 'PLA-AL10' 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' 3 3
        $twoPhaseRecordPath = Join-Path $confirmTemp 'two-phase-confirmation.json'
        Write-JsonFile $twoPhaseRecordPath $twoPhaseRecord
        $twoPhaseRecordSha = Get-FileSha256 $twoPhaseRecordPath
        [IO.File]::WriteAllText($twoPhaseRecordPath + '.sha256', $twoPhaseRecordSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $twoPhaseReview = [ordered]@{
            schema_version = 1
            record_kind = 'e3-ready-freeze-review'
            is_evidence = $false
            exception = 'E3-PHYS-PREFLIGHT'
            campaign_id = $script:CandidateCampaignId
            evidence_id = $script:CandidateEvidenceId
            code_sha = ('a' * 40)
            runner_sha256 = ('b' * 64)
            confirmation_contract_sha256 = (Get-ConfirmationContractSha256 $twoPhaseBlocked)
            machine_confirmation_sha256 = $twoPhaseRecordSha
            reviewer_role = 'selftest-independent-reviewer'
            operator_role = 'selftest-operator'
            verdict = 'pass'
            blockers = 0
            majors = 0
            started_at = '2099-01-01T00:00:06+00:00'
            ended_at = '2099-01-01T00:00:09+00:00'
        }
        $twoPhaseReviewPath = Join-Path $confirmTemp 'two-phase-review.json'
        Write-JsonFile $twoPhaseReviewPath $twoPhaseReview
        $twoPhaseReviewSha = Get-FileSha256 $twoPhaseReviewPath
        [IO.File]::WriteAllText($twoPhaseReviewPath + '.sha256', $twoPhaseReviewSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $twoPhaseBinding = [ordered]@{ status = 'pass'; authorization_id = $script:AuthId; record_path = $twoPhaseRecordPath; record_sha256 = $twoPhaseRecordSha }
        $twoPhaseReviewBinding = [ordered]@{ status = 'pass'; record_path = $twoPhaseReviewPath; record_sha256 = $twoPhaseReviewSha; reviewer_role = 'selftest-independent-reviewer' }
        # ADJ-20260810-0001 (C6): the two-phase positive consumes the T2-advanced ready freeze
        # ($twoPhaseReady, frozen at 00:00:10 past the confirmation end 00:00:05 and the review end
        # 00:00:09) - never a fresh T1 clone - while keeping the review binding. The full contract
        # hash differs from the blocked phase, but the stable confirmation contract is identical, so
        # the blocked-phase records are consumable by the ready-phase freeze.
        Add-Member -InputObject $twoPhaseReady -NotePropertyName 'machine_fresh_confirmation' -NotePropertyValue $twoPhaseBinding -Force
        Add-Member -InputObject $twoPhaseReady -NotePropertyName 'independent_review_record' -NotePropertyValue $twoPhaseReviewBinding -Force
        $twoPhaseResult = Assert-MachineFreshConfirmation $twoPhaseReady
        Check ($null -ne $twoPhaseResult -and [string]$twoPhaseResult.RecordSha256 -eq $twoPhaseRecordSha) 'two-phase-ready-consumes-blocked-phase-records'
        # negative: mutating a stable contract core field that passes the freeze static value gate
        # (operator_role) changes the confirmation contract hash, so the records bound on the
        # blocked phase must be rejected by the ready-phase consumer. settings_revoke_mechanism is
        # NOT used here: it has a dedicated static freeze gate, so it would be rejected before the
        # contract check ever runs.
        $twoPhaseMutated = New-ConfirmFreeze 'ready' $twoPhaseBinding $twoPhaseReviewBinding
        $twoPhaseMutated.operator_role = 'some-other-operator'
        try { [void](Assert-MachineFreshConfirmation $twoPhaseMutated); Check $false 'two-phase-rejects-stable-contract-mutation' } catch { Check ($_.Exception.Message -match 'confirmation_contract_sha256') 'two-phase-rejects-stable-contract-mutation' }
    } finally {
        $script:DryRun = $savedConfirmDryRun; $script:LiveSimulation = $savedConfirmLiveSimulation; $script:TargetBindingConfirm = $savedConfirmTargetBindingConfirm; $script:ExecutionMode = $savedConfirmExecution
        Remove-Item -LiteralPath $confirmTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
    Check (-not (Test-Path -LiteralPath $confirmTemp)) 'confirm-consumer-selftest-leaves-no-files'
    # ADJ-20260810-0001 (C6): record write function tests. A host-only run with no PHYS target
    # yields a blocked record with attempted=0/completed=0 and a matching companion, with zero HDC
    # processes; pre-existing record/companion outputs are rejected without changing bytes; and the
    # selftest leaves no files behind.
    $writeTemp = Join-Path ([IO.Path]::GetTempPath()) ('e3-confirm-write-selftest-' + [guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($writeTemp) | Out-Null
    try {
        $blockedRecordPath = Join-Path $writeTemp 'target-binding-confirmation.json'
        $blockedDraft = New-TargetBindingConfirmationRecord $confirmFreezeBase ('e' * 64) $confirmationContractSha ([pscustomobject]@{ Fingerprint = 'g' * 64 }) ([DateTimeOffset]::Parse('2099-01-01T00:00:00+00:00')) ([DateTimeOffset]::Parse('2099-01-01T00:00:00+00:00')) 'blocked' 'preflight: PHYS_1_TARGET must contain exactly one real target token' 'SELFTEST-HDC-1.0' $null $null 0 0
        $blockedSha = Write-TargetBindingConfirmationRecordPair $blockedRecordPath $blockedDraft
        Check (Test-Path -LiteralPath ($blockedRecordPath + '.sha256')) 'blocked-record-write-companion-exists'
        Check ((Get-Content -LiteralPath ($blockedRecordPath + '.sha256') -Raw).Trim() -eq $blockedSha -and $blockedSha -eq (Get-FileSha256 $blockedRecordPath)) 'blocked-record-write-companion-matches'
        $blockedJson = Get-Content -LiteralPath $blockedRecordPath -Raw | ConvertFrom-Json -Depth 20
        Check ([string]$blockedJson.verdict -eq 'blocked' -and [int]$blockedJson.command_attempted -eq 0 -and [int]$blockedJson.command_completed -eq 0 -and [string]$blockedJson.reason -match 'PHYS_1_TARGET') 'blocked-record-attempted0-completed0'
        Check ($script:HdcProcessStartCount -eq 0) 'blocked-record-write-zero-hdc'
        # ADJ-20260810-0001 (C6): a blocked record may carry any attempted/completed <= 3 (partial
        # probe progress); only a pass exit requires exactly 3 (producer- and consumer-enforced).
        $blockedPartialDraft = New-TargetBindingConfirmationRecord $confirmFreezeBase ('e' * 64) $confirmationContractSha ([pscustomobject]@{ Fingerprint = 'g' * 64 }) ([DateTimeOffset]::Parse('2099-01-01T00:00:00+00:00')) ([DateTimeOffset]::Parse('2099-01-01T00:00:03+00:00')) 'blocked' 'probe-2-tuple-model-mismatch' 'SELFTEST-HDC-1.0' 'PLA-AL10' $null 2 2
        $blockedPartialPath = Join-Path $writeTemp 'target-binding-confirmation-partial.json'
        [void](Write-TargetBindingConfirmationRecordPair $blockedPartialPath $blockedPartialDraft)
        $blockedPartialJson = Get-Content -LiteralPath $blockedPartialPath -Raw | ConvertFrom-Json -Depth 20
        Check ([string]$blockedPartialJson.verdict -eq 'blocked' -and [int]$blockedPartialJson.command_attempted -eq 2 -and [int]$blockedPartialJson.command_completed -eq 2 -and [int]$blockedPartialJson.command_count -eq 2) 'blocked-record-allows-attempted-completed-le-3'
        $recordBytes = [IO.File]::ReadAllBytes($blockedRecordPath)
        $companionBytes = [IO.File]::ReadAllBytes($blockedRecordPath + '.sha256')
        try { [void](Write-TargetBindingConfirmationRecordPair $blockedRecordPath $blockedDraft); Check $false 'preexisting-record-rejected' } catch { Check ($_.Exception.Message -match 'already exists') 'preexisting-record-rejected' }
        Check ([Convert]::ToBase64String([IO.File]::ReadAllBytes($blockedRecordPath)) -eq [Convert]::ToBase64String($recordBytes)) 'preexisting-record-bytes-unchanged'
        Check ([Convert]::ToBase64String([IO.File]::ReadAllBytes($blockedRecordPath + '.sha256')) -eq [Convert]::ToBase64String($companionBytes)) 'preexisting-record-companion-unchanged'
        $companionOnlyPath = Join-Path $writeTemp 'companion-only.json'
        [IO.File]::WriteAllText($companionOnlyPath + '.sha256', ('1' * 64) + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        try { [void](Write-TargetBindingConfirmationRecordPair $companionOnlyPath $blockedDraft); Check $false 'preexisting-companion-rejected' } catch { Check ($_.Exception.Message -match 'already exists') 'preexisting-companion-rejected' }
        Check (-not (Test-Path -LiteralPath $companionOnlyPath)) 'preexisting-companion-no-record-written'
        Check ((Get-Content -LiteralPath ($companionOnlyPath + '.sha256') -Raw).Trim() -eq ('1' * 64)) 'preexisting-companion-bytes-unchanged'
    } finally {
        Remove-Item -LiteralPath $writeTemp -Recurse -Force -ErrorAction SilentlyContinue
    }
    Check (-not (Test-Path -LiteralPath $writeTemp)) 'confirm-write-selftest-leaves-no-files'
    $recordDraft = New-TargetBindingConfirmationRecord $confirmFreezeBase ('e' * 64) $confirmationContractSha ([pscustomobject]@{ Fingerprint = 'g' * 64 }) ([DateTimeOffset]::Parse('2099-01-01T00:00:00+00:00')) ([DateTimeOffset]::Parse('2099-01-01T00:00:05+00:00')) 'pass' 'N/A' 'SELFTEST-HDC-1.0' 'PLA-AL10' 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' 3 3
    $recordDraftJson = $recordDraft | ConvertTo-Json -Depth 20
    Check ($recordDraftJson -match '"record_kind"\s*:\s*"target-binding-confirmation"' -and $recordDraftJson -match '"is_evidence"\s*:\s*false' -and $recordDraftJson -match '"verdict"\s*:\s*"pass"' -and $recordDraftJson -match '"command_attempted"\s*:\s*3' -and $recordDraftJson -match '"command_completed"\s*:\s*3' -and $recordDraftJson -match '"target_redacted"\s*:\s*true' -and $recordDraftJson -match '"device_alias"\s*:\s*"PHYS-1"' -and $recordDraftJson -match '"attempt"\s*:\s*"initial"') 'confirmation-record-required-fields'
    Check ($recordDraftJson -notmatch '(?i)(udid|serial|"target"\s*:|token|password|secret|endpoint|device-canary)') 'confirmation-record-no-target-or-secret'

    Check ($script:HdcProcessStartCount -eq 0) 'SelfTest-zero-HDC-processes'
    if ($failures.Count -gt 0) { throw "self-test failures: $($failures -join ', ')" }
    Write-Host 'SELFTEST_RESULT=pass HDC_PROCESSES=0'
}

# ADJ-20260810-0001 (C6): mode exclusivity is enforced BEFORE the SelfTest early exit so invalid
# switch combinations (e.g. -SelfTest -TargetBindingConfirm, or confirm mode combined with
# EvidenceRoot/RawRoot) are rejected without running anything.
Assert-ModeExclusivity

if ($SelfTest) {
    Invoke-RunnerSelfTest
    exit 0
}

if ([string]::IsNullOrWhiteSpace($FreezeManifest) -or [string]::IsNullOrWhiteSpace($HapA) -or [string]::IsNullOrWhiteSpace($HapB) -or [string]::IsNullOrWhiteSpace($HdcPath)) {
    throw 'FreezeManifest, HapA, HapB, and HdcPath are required unless SelfTest is used'
}
if (-not $TargetBindingConfirm -and [string]::IsNullOrWhiteSpace($EvidenceRoot)) {
    throw 'EvidenceRoot is required unless SelfTest or TargetBindingConfirm is used'
}
if ($HdcTimeoutSeconds -lt 1 -or $HdcTimeoutSeconds -gt 120) { throw 'HdcTimeoutSeconds must be between 1 and 120' }
if ($OperatorTimeoutSeconds -lt 1 -or $OperatorTimeoutSeconds -gt 900) { throw 'OperatorTimeoutSeconds must be between 1 and 900' }
if ($LiveSimulation) {
    if ([string]::IsNullOrWhiteSpace($SimulationFixture) -or -not (Test-Path -LiteralPath $SimulationFixture -PathType Leaf)) { throw 'LiveSimulation requires SimulationFixture' }
    $script:Simulation = Get-Content -LiteralPath $SimulationFixture -Raw | ConvertFrom-Json -Depth 50
}

$script:RepoRoot = Get-GitRepositoryRoot
$freezePath = Get-NormalizedPath $FreezeManifest
if (-not (Test-Path -LiteralPath $freezePath -PathType Leaf)) { throw 'FreezeManifest file missing' }
$freeze = Get-Content -LiteralPath $freezePath -Raw | ConvertFrom-Json -Depth 50
$script:Freeze = $freeze
$freezeSha256 = Get-FileSha256 $freezePath
$script:PublicVersionLiterals = @([string]$freeze.target_tuple.full_system_build, [string]$freeze.sdk.version, [string]$freeze.hdc.version)
# ADJ-20260810-0001 (C6): the frozen HDC version is a legitimate public literal too - without it,
# an IP-like HDC version (e.g. 7.0.0.100) would be redacted by Protect-SensitiveText before it
# enters the confirmation record, corrupting the observed hdc_version field.
Assert-FreezeManifest $freeze $freezePath
$freezeContractSha256 = Get-FreezeContractSha256 $freeze
# ADJ-20260810-0001 (C6): the stable two-phase projection that confirmation/review records bind.
# The full freeze contract hash (above) is the final ready freeze's own identity; the confirmation
# contract is the phase-invariant projection that must survive the blocked -> ready transition.
$confirmationContractSha256 = Get-ConfirmationContractSha256 $freeze
$repositoryBefore = Get-RepositoryState
if ([string]$freeze.code_sha -ne $repositoryBefore.Head) { throw 'freeze code_sha does not match repository HEAD' }
if (-not $DryRun -and -not $repositoryBefore.Clean) { throw 'Live, LiveSimulation, and TargetBindingConfirm require a clean repository state' }
if (-not $script:NoDeviceMode -and -not $TargetBindingConfirm) { Assert-TargetEnvironment }
if ($TargetBindingConfirm) {
    # ADJ-20260810-0001 host-governed fresh confirmation: three whitelisted target-binding probes
    # only; no campaign roots, no is_evidence, no campaign/evidence consumption, no capture, no
    # install/start, and no final-campaign cleanup queries. Blocked output exits nonzero.
    $confirmationResult = Invoke-TargetBindingConfirm $freeze $freezeSha256 $confirmationContractSha256 $repositoryBefore
    $resultSuffix = if ($confirmationResult.Verdict -eq 'pass') { " RECORD_SHA256=$($confirmationResult.RecordSha256)" } else { '' }
    Write-Host "RUNNER_RESULT=$($confirmationResult.Verdict) MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=$($confirmationResult.CommandAttempted) COMMAND_COMPLETED=$($confirmationResult.CommandCompleted) RECORD=$($confirmationResult.RecordPath)$resultSuffix"
    if ($confirmationResult.Verdict -eq 'pass') { exit 0 }
    exit 2
}
[void](Initialize-OutputRoots)
$startedAt = Get-Now
$scenarios = New-BlockedScenarios 'not-run'
$overall = 'blocked'
$recordStatus = 'blocked'
$fatalMessage = $null
$infrastructureReason = $null
$integrityViolations = [Collections.Generic.List[string]]::new()

try {
    Add-TranscriptRecord 'preflight-gates-pass' ([ordered]@{
        exception = $freeze.exception
        campaign_id = $freeze.campaign_id
        attempt = $freeze.attempt
        plan_status = $freeze.plan_status
        execution_mode = $script:ExecutionMode
        repository = $repositoryBefore.Fingerprint
        freeze_manifest_sha256 = $freezeSha256
    })
    # ADJ-20260810-0001 (C6): the preflight transcript projects the governance bindings without
    # leaking real paths (path_sha256 only), anchored to the STABLE confirmation contract (the
    # contract the confirmation record actually binds), not the full freeze contract.
    Add-TranscriptRecord 'machine-fresh-confirmation' ([ordered]@{
        status = $(if ($null -ne $script:MachineFreshConfirmation) { 'pass' } else { 'not-required' })
        authorization_id = $(if ($null -ne $script:MachineFreshConfirmation) { [string]$script:MachineFreshConfirmation.authorization_id } else { 'N/A' })
        record_sha256 = $(if ($null -ne $script:MachineFreshConfirmation) { [string]$script:MachineFreshConfirmation.record_sha256 } else { 'N/A' })
        record_path_sha256 = $(if ($null -ne $script:MachineFreshConfirmation) { [string]$script:MachineFreshConfirmation.record_path_sha256 } else { 'N/A' })
        confirmation_contract_sha256 = $confirmationContractSha256
    })
    # ADJ-20260810-0001 (C6): symmetric independent-review-record projection into the preflight
    # transcript (status/reviewer_role/record_sha256/record_path_sha256 only, never the real path),
    # mirroring machine-fresh-confirmation and anchored to the stable confirmation contract, so both
    # governance bindings are visible in the transcript without leaking host paths.
    Add-TranscriptRecord 'independent-review-record' ([ordered]@{
        status = $(if ($null -ne $script:IndependentReviewRecord) { 'pass' } else { 'not-required' })
        reviewer_role = $(if ($null -ne $script:IndependentReviewRecord) { [string]$script:IndependentReviewRecord.reviewer_role } else { 'N/A' })
        record_sha256 = $(if ($null -ne $script:IndependentReviewRecord) { [string]$script:IndependentReviewRecord.record_sha256 } else { 'N/A' })
        record_path_sha256 = $(if ($null -ne $script:IndependentReviewRecord) { [string]$script:IndependentReviewRecord.record_path_sha256 } else { 'N/A' })
        confirmation_contract_sha256 = $confirmationContractSha256
    })
    Initialize-PriorBlockedBinding $freeze
    if ($DryRun) {
        $scenarios = Invoke-DryRunCampaign
        $overall = 'blocked'
        $recordStatus = 'blocked'
    } else {
        $scenarios = Invoke-StrongLiveCampaign $freeze
        $measuredOverall = Get-ScenarioAggregation $scenarios
        if ($LiveSimulation -and $measuredOverall -ne 'invalid') {
            $overall = 'blocked'
            $recordStatus = 'blocked'
        } else {
            $overall = $measuredOverall
            $recordStatus = $(if ($measuredOverall -eq 'invalid') { 'invalidated' } else { 'collected' })
        }
    }
} catch {
    $rawException = [string]$_.Exception.Message
    if ($_.InvocationInfo.ScriptLineNumber -gt 0) { $rawException += " (runner-line=$($_.InvocationInfo.ScriptLineNumber))" }
    $phase = [string]$script:CampaignPhase
    if ($phase -match '^scenario-([1-7])$' -and $rawException -notmatch 'scenario-[1-7]') {
        $rawException = "scenario-$($Matches[1]) $rawException"
    } elseif ($phase -eq 'preflight' -and $rawException -notmatch '(?i)^preflight\b|collection preparation blocked|scenario-[1-7]') {
        $rawException = "preflight: $rawException"
    }
    $fatalMessage = Protect-SensitiveText $rawException
    $classification = Get-FailureClassification $fatalMessage
    $isScenarioInvalid = $null -ne $script:ScenarioInvalid -or [string]$classification.Overall -eq 'invalid'
    $overall = if ($isScenarioInvalid) { 'invalid' } elseif ($script:ExecutionMode -eq 'live') { $classification.Overall } else { 'blocked' }
    $recordStatus = if ($isScenarioInvalid) { 'invalidated' } elseif ($script:ExecutionMode -eq 'live') { $classification.RecordStatus } else { 'blocked' }
    $infrastructureReason = $classification.InfrastructureReason
    $failedScenario = if ($null -ne $script:ScenarioInvalid) { [int]$script:ScenarioInvalid.scenario } elseif ($fatalMessage -match 'scenario-([1-7])') { [int]$Matches[1] } else { $null }
    $defaultReason = if ($phase -eq 'preflight' -or $fatalMessage -match '(?i)^preflight\b|collection preparation blocked') { 'preflight-or-collection-preparation-blocked' } elseif ($isScenarioInvalid) { 'not-run-due-to-invalid' } else { 'not-run-after-runner-failure' }
    $scenarios = New-BlockedScenarios $defaultReason
    foreach ($partialScenario in @($script:PartialScenarios)) {
        # Preserve already-measured scenario results; preflight/exception must not overwrite them.
        $scenarios[[int]$partialScenario.scenario - 1] = $partialScenario
    }
    if ($isScenarioInvalid) {
        $invalidScenarioNumber = if ($null -ne $failedScenario) { [int]$failedScenario } else { 0 }
        for ($index = 0; $index -lt 7; $index++) {
            $scenarioNumber = $index + 1
            $alreadyMeasured = @($script:PartialScenarios | Where-Object { [int]$_.scenario -eq $scenarioNumber }).Count -gt 0
            if ($alreadyMeasured) { continue }
            $entry = $scenarios[$index]
            if ($scenarioNumber -eq $invalidScenarioNumber) {
                $entry.result = 'invalid'
                $entry.reason = if ($null -ne $script:ScenarioInvalid) { [string]$script:ScenarioInvalid.reason } else { $fatalMessage }
            } else {
                $entry.result = 'invalid'
                $entry.reason = 'not-run-due-to-invalid'
            }
            if ($scenarioNumber -eq 2 -and $null -eq $entry.assertions) {
                $entry.assertions = [ordered]@{ allow = 'invalid'; vpn_on_create = 'invalid'; vpn_connection_create_fd = 'invalid' }
            }
        }
    } elseif ($null -ne $failedScenario) {
        $alreadyMeasured = @($script:PartialScenarios | Where-Object { [int]$_.scenario -eq $failedScenario }).Count -gt 0
        if (-not $alreadyMeasured) {
            $scenarioEntry = $scenarios[$failedScenario - 1]
            $scenarioEntry.result = $overall
            $scenarioEntry.reason = $fatalMessage
            if ($failedScenario -eq 2) {
                $scenarioEntry.assertions = [ordered]@{ allow = $overall; vpn_on_create = 'blocked'; vpn_connection_create_fd = 'blocked' }
            }
        }
    }
    Add-TranscriptRecord 'runner-exception' ([ordered]@{ message = $fatalMessage; campaign_phase = $phase; campaign_started = [bool]$script:CampaignStarted; infrastructure_reason = $infrastructureReason })
} finally {
    try {
        $cleanupReason = if ($null -ne $fatalMessage) { 'exception-cleanup' } else { 'final-cleanup' }
        Invoke-PreciseFinallyCleanup $cleanupReason
    } catch {
        $cleanupFailure = Protect-SensitiveText $_.Exception.Message
        $script:CleanupVerification = [ordered]@{ status = 'blocked-cleanup-exception'; verified_absent = $false; bundles = @() }
        $script:CaptureDegraded.Add([ordered]@{ scenario = 7; component = 'finally-cleanup'; reason = $cleanupFailure; category = 'non-infrastructure'; infrastructure_reason = $null })
        if ([string]::IsNullOrEmpty($fatalMessage)) {
            $fatalMessage = "RUNNER_HOST_FAILURE $cleanupFailure"
            $infrastructureReason = 'runner-host-failure'
        }
    }
    if ($null -ne $script:CampaignCapture) {
        try { Stop-CampaignHilogCapture $script:CampaignCapture } catch {
            $stopFailure = Protect-SensitiveText $_.Exception.Message
            $script:CaptureDegraded.Add([ordered]@{ scenario = 0; component = 'raw-hilog-finalize'; reason = $stopFailure; category = 'non-infrastructure'; infrastructure_reason = $null })
        }
    }
    Set-CaptureDegradedScenarios $scenarios
    if ([string]::IsNullOrEmpty($infrastructureReason) -and -not [string]::IsNullOrEmpty($script:InfrastructureReasonObserved)) { $infrastructureReason = $script:InfrastructureReasonObserved }
    $measuredOverall = Get-ScenarioAggregation $scenarios
    if ($measuredOverall -eq 'invalid' -or $null -ne $script:ScenarioInvalid) {
        $overall = 'invalid'
        $recordStatus = 'invalidated'
    } elseif ($script:CaptureDegraded.Count -gt 0 -or (-not $DryRun -and -not [bool]$script:CleanupVerification.verified_absent)) {
        # Capture degradation / cleanup uncertainty never downgrades an explicit scenario fail.
        $overall = if ($measuredOverall -eq 'fail') { 'fail' } else { 'blocked' }
        $recordStatus = 'blocked'
    } elseif ($script:ExecutionMode -eq 'live') {
        $overall = $measuredOverall
        $recordStatus = 'collected'
    } else {
        # DryRun / LiveSimulation are non-evidence (record_status stays blocked). An explicit measured
        # fail still surfaces as overall/verdict fail and is never collapsed to blocked; non-fail stays blocked.
        $overall = if ($measuredOverall -eq 'fail') { 'fail' } else { 'blocked' }
        $recordStatus = 'blocked'
    }
    if ($script:HdcProcessStartCount -ne 0 -and $script:NoDeviceMode) {
        $integrityViolations.Add('nondevice-mode-started-hdc-process')
    }
    Add-TranscriptRecord 'campaign-finalizing' ([ordered]@{
        overall = $overall
        record_status = $recordStatus
        installed_a = [bool]$script:InstalledA
        installed_b = [bool]$script:InstalledB
        staging_sent = [bool]$script:StagingSent
        staging_may_exist = [bool]$script:StagingMayExist
        hdc_processes_started = $script:HdcProcessStartCount
    })
    $endedAt = Get-Now
    $attestation = [ordered]@{
        schema_version = 2
        evidence_id = $freeze.evidence_id
        campaign_id = $freeze.campaign_id
        attempt = $freeze.attempt
        execution_mode = $script:ExecutionMode
        operator_role = $freeze.operator_role
        trust_model = 'mechanical-action-only-machine-verified-v1'
        attested = [bool]($script:ExecutionMode -eq 'live' -and $null -eq $fatalMessage)
        statement = 'Operator performed only the single-step mechanical actions prompted by the runner and pressed Enter after each action. Operator attestation records mechanical step completion times only and does not contribute semantic verdicts; all scenario results are machine-verified.'
        mechanical_actions = @($script:OperatorActions)
        record_status = $(if ($script:ExecutionMode -eq 'live') { 'collected' } else { 'blocked' })
        reviewer = 'pending'
        reviewed_at = 'pending'
    }
    Write-JsonFile (Join-Path $script:EvidencePath 'operator-attestation.json') $attestation
    # Seal the final (complete) operator-wait state via the collection manifest so no dynamic
    # state is left unbound; the pollable path itself is inside EvidenceRoot.
    Write-OperatorWaitState 'complete'
    if ($LiveSimulation -and (Get-OptionalJsonBoolean $script:Simulation 'tamper_wait_state_after_complete' $false)) {
        # ADJ-20260808-0003 adversarial fixture: rewrite the sealed wait state to a non-complete
        # phase; Test-EvidenceIntegrity must flag operator-wait-state-not-complete.
        $waitPath = Join-Path $script:EvidencePath 'operator-wait-state.json'
        $waitDoc = Get-Content -LiteralPath $waitPath -Raw | ConvertFrom-Json -Depth 20
        $waitDoc.phase = 'waiting'
        $waitDoc.complete = $false
        $waitDoc.completed_at = $null
        Write-JsonFile $waitPath $waitDoc
    }
    $manifestPath = Write-CollectionManifest $script:EvidencePath
    $manifestSha256 = Get-FileSha256 $manifestPath
    $record = New-CompleteRecord $freeze $scenarios $overall $recordStatus $startedAt $endedAt $fatalMessage $infrastructureReason $repositoryBefore $freezeSha256 $freezeContractSha256 $confirmationContractSha256 $manifestSha256
    $recordPath = Join-Path $script:EvidencePath 'scenario-results.json'
    Write-JsonFile $recordPath $record
    Write-CampaignSeal $script:EvidencePath

    try {
        $repositoryAfter = Get-RepositoryState
        if ($repositoryAfter.Fingerprint -ne $repositoryBefore.Fingerprint) { $integrityViolations.Add('repository-drift') }
    } catch {
        $integrityViolations.Add('repository-state-after-unavailable')
    }
    if ($LiveSimulation -and (Get-OptionalJsonBoolean $script:Simulation 'tamper_payload_after_manifest' $false)) {
        $lines = @(Get-Content -LiteralPath $script:ProjectionTranscript)
        if ($lines.Count -gt 0) {
            $tamperedEntry = $lines[0] | ConvertFrom-Json -Depth 40
            $tamperedEntry.payload.kind = 'simulation-tampered-payload'
            $lines[0] = $tamperedEntry | ConvertTo-Json -Depth 40 -Compress
            [IO.File]::WriteAllLines($script:ProjectionTranscript, $lines, [Text.UTF8Encoding]::new($false))
        }
    }
    if ($LiveSimulation -and (Get-OptionalJsonBoolean $script:Simulation 'tamper_transcript_after_manifest' $false)) {
        [IO.File]::AppendAllText($script:ProjectionTranscript, "tamper`n", [Text.UTF8Encoding]::new($false))
    }
    foreach ($violation in @(Test-EvidenceIntegrity $script:EvidencePath $scenarios)) { $integrityViolations.Add($violation) }
    if ($integrityViolations.Count -gt 0) {
        $record.integrity_violations = @($integrityViolations | Select-Object -Unique)
        $record.record_status = 'invalidated'
        $record.overall = 'invalid'
        $record.verdict = 'invalid'
        $record.scenario_aggregation.overall = 'invalid'
        $overall = 'invalid'
        $recordStatus = 'invalidated'
        Write-JsonFile $recordPath $record
        Write-CampaignSeal $script:EvidencePath
    }
}

if ($script:NoDeviceMode -and $script:HdcProcessStartCount -ne 0) { throw 'host-only safety invariant violated: HDC process count is nonzero' }
if ($null -ne $fatalMessage) { Write-Host "RUNNER_FAILURE=$fatalMessage" }
Write-Host "RUNNER_RESULT=$overall RECORD_STATUS=$recordStatus MODE=$($script:ExecutionMode) EVIDENCE_ROOT=$($script:EvidencePath) RAW_ROOT_HASH=$(Get-TextSha256 $script:RawPath) HDC_PROCESSES=$($script:HdcProcessStartCount)"
if ($null -ne $fatalMessage -or $overall -eq 'invalid') { exit 2 }
exit 0
