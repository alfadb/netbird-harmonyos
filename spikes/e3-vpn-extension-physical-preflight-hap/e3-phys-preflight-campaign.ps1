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
    [switch]$SelfTest
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
$script:ExecutionMode = if ($DryRun) { 'dry-run' } elseif ($LiveSimulation) { 'live-simulation' } else { 'live' }
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
$script:ProbeContexts = @{}
$script:CurrentWindowEnd = $null

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
    return [ordered]@{
        exception = $Freeze.exception
        campaign_id = $Freeze.campaign_id
        scenario_window_seconds = $Freeze.scenario_window_seconds
        device_alias = $Freeze.device_alias
        target_tuple = $Freeze.target_tuple
        settings_reallow_expected_path = $Freeze.settings_reallow_expected_path
        settings_reallow_path_policy = $Freeze.settings_reallow_path_policy
        settings_revoke_mechanism = $Freeze.settings_revoke_mechanism
        settings_vpn_page_policy = $Freeze.settings_vpn_page_policy
        destroy_terminal_policy = $Freeze.destroy_terminal_policy
        process_absent_required_count = $Freeze.process_absent_required_count
        process_absent_probe_spacing_seconds = $Freeze.process_absent_probe_spacing_seconds
        signing = $Freeze.signing
        artifact_sha256 = $Freeze.artifact_sha256
        source = $Freeze.source
        sdk = $Freeze.sdk
        hdc = $Freeze.hdc
        runner_sha256 = $Freeze.runner_sha256
        code_sha = $Freeze.code_sha
        preflight_inputs_frozen_at = $Freeze.preflight_inputs_frozen_at
        cleanup_baseline_frozen = $Freeze.cleanup_baseline_frozen
        collection_ready = $Freeze.collection_ready
        independent_review_ready = $Freeze.independent_review_ready
        operator_role = $Freeze.operator_role
        independent_reviewer_role = $Freeze.independent_reviewer_role
    }
}

function Get-FreezeContractSha256 {
    param([Parameter(Mandatory)]$Freeze)
    return Get-TextSha256 ((Get-FreezeContract $Freeze) | ConvertTo-Json -Depth 30 -Compress)
}

function Assert-FreezeManifest {
    param([Parameter(Mandatory)]$Freeze, [Parameter(Mandatory)][string]$FreezePath)
    if ((Get-RequiredProperty $Freeze 'schema_version') -ne 1) { throw 'unsupported freeze schema_version' }
    $planStatus = [string](Get-RequiredProperty $Freeze 'plan_status')
    if ($DryRun) {
        if ($planStatus -notin @('blocked', 'ready')) { throw 'DryRun plan_status must be blocked or ready' }
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
    $frozenAt = [DateTimeOffset]::MinValue
    if (-not [DateTimeOffset]::TryParse([string](Get-RequiredProperty $Freeze 'preflight_inputs_frozen_at'), [ref]$frozenAt)) {
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
    if (-not (Test-Path -LiteralPath $FreezePath -PathType Leaf)) { throw 'FreezeManifest file missing' }
    [void](Get-PriorBlockedBinding $Freeze)
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
        'PidOf' { return $common + @('shell', 'pidof', $bundle) }
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
        'PidOf' { '' }
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
    if ($Operation -eq 'Uninstall') {
        if ([string]$Parameters.Bundle -eq $script:BundleA) { $script:SimulationInstalledA = $false }
        if ([string]$Parameters.Bundle -eq $script:BundleB) { $script:SimulationInstalledB = $false }
    }
    if ($Operation -in @('ReceiveScreen', 'ReceiveLayout')) {
        $extension = if ($Operation -eq 'ReceiveScreen') { '.png' } else { '.json' }
        $destination = Join-Path $script:RawPath ('capture-' + [string]$Parameters.Name + $extension)
        if ($Operation -eq 'ReceiveScreen') {
            [IO.File]::WriteAllBytes($destination, [byte[]](1, 2, 3, 4))
        } else {
            Write-JsonFile $destination ([ordered]@{ simulated = $true; capture = [string]$Parameters.Name })
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

function Read-OperatorResponse {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][ValidateSet('ready', 'action', 'confirmation')][string]$Kind,
        [Parameter(Mandatory)][string]$Expected,
        [Parameter(Mandatory)][string]$Prompt
    )
    if ($LiveSimulation) {
        $operatorFixture = Get-OptionalProperty $script:Simulation 'operator'
        $delayName = if ($Kind -eq 'ready') { 'ready_delay_seconds' } else { 'action_ack_delay_seconds' }
        $delay = [double](Get-OptionalProperty $operatorFixture $delayName $(if ($Kind -eq 'ready') { 1.0 } else { 5.0 }))
        if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
        $script:VirtualSeconds += $delay
        if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
        $failScenarios = @(Get-OptionalProperty $operatorFixture 'fail_scenarios' @())
        $valid = $Scenario -notin @($failScenarios | ForEach-Object { [int]$_ })
        return [pscustomobject]@{ Valid = $valid; AnsweredAt = Get-Now; TimedOut = $false; DelaySeconds = $delay }
    }
    Write-Host $Prompt
    $readTask = [Console]::In.ReadLineAsync()
    $deadline = (Get-Now).AddSeconds($OperatorTimeoutSeconds)
    while (-not $readTask.IsCompleted -and (Get-Now) -lt $deadline) {
        $remainingMilliseconds = [int][Math]::Max(1, [Math]::Min(250, ($deadline - (Get-Now)).TotalMilliseconds))
        [void]$readTask.Wait($remainingMilliseconds)
        if ($null -ne $script:CampaignCapture) { Update-CampaignCapture $script:CampaignCapture }
    }
    if (-not $readTask.IsCompleted) {
        return [pscustomobject]@{ Valid = $false; AnsweredAt = Get-Now; TimedOut = $true; DelaySeconds = $OperatorTimeoutSeconds }
    }
    $answer = $readTask.GetAwaiter().GetResult()
    return [pscustomobject]@{ Valid = $answer -eq $Expected; AnsweredAt = Get-Now; TimedOut = $false; DelaySeconds = $null }
}

function Get-SimulationConfirmation {
    param([Parameter(Mandatory)][string]$Name)
    if (-not $LiveSimulation) { return $false }
    $confirmations = Get-OptionalProperty (Get-OptionalProperty $script:Simulation 'operator') 'confirmations'
    return Get-OptionalJsonBoolean $confirmations $Name $false
}

function Confirm-VisibleFact {
    param([Parameter(Mandatory)][int]$Scenario, [Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Prompt)
    if ($LiveSimulation) {
        $valid = Get-SimulationConfirmation $Name
        Add-TranscriptRecord 'operator-confirmation' ([ordered]@{ scenario = $Scenario; name = $Name; confirmed = [bool]$valid; simulated = $true })
        return [bool]$valid
    }
    $nonce = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $response = Read-OperatorResponse $Scenario 'confirmation' "$Name $nonce" "$Prompt 若为真：逐字输入完整令牌 $Name $nonce ；若为假：直接按回车留空。"
    Add-TranscriptRecord 'operator-confirmation' ([ordered]@{ scenario = $Scenario; name = $Name; confirmed = [bool]$response.Valid; timed_out = [bool]$response.TimedOut; simulated = $false })
    return [bool]$response.Valid
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

function Add-SimulationScenarioOutput {
    param([Parameter(Mandatory)]$Capture, [Parameter(Mandatory)][int]$Scenario, [Parameter(Mandatory)][DateTimeOffset]$ActionPromptAt)
    $scenarioEvents = Get-OptionalProperty $script:Simulation 'scenario_events'
    $items = @(Get-OptionalProperty $scenarioEvents ([string]$Scenario) @()) | Sort-Object { [double](Get-OptionalProperty $_ 'offset_seconds' 0.0) }
    foreach ($item in $items) {
        $offset = [double](Get-OptionalProperty $item 'offset_seconds' 0.0)
        $deviceStamp = $ActionPromptAt.AddSeconds($offset).ToString('yyyy-MM-dd HH:mm:ss.fffzzz', [Globalization.CultureInfo]::InvariantCulture)
        $text = ([string](Get-OptionalProperty $item 'text' '')).Replace('<DEVICE_OBSERVED_AT>', $deviceStamp, [StringComparison]::Ordinal)
        $withoutNewline = Get-OptionalJsonBoolean $item 'append_without_newline' $false
        [IO.File]::AppendAllText($Capture.StdoutPath, $text + $(if ($withoutNewline) { '' } else { [Environment]::NewLine }), [Text.UTF8Encoding]::new($false))
    }
    $dieScenario = [int](Get-OptionalProperty $script:Simulation 'capture_die_scenario' 0)
    if ($dieScenario -eq $Scenario) { $Capture.SimulatedDead = $true }
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

function Invoke-ScenarioObservation {
    param(
        [Parameter(Mandatory)][int]$Scenario,
        [Parameter(Mandatory)][string]$Instruction,
        [scriptblock]$OnAction,
        [scriptblock]$DuringWait
    )
    $capture = $script:CampaignCapture
    if ($null -eq $capture) { throw "scenario-$Scenario continuous capture is not initialized" }
    Update-CampaignCapture $capture
    $capture.ActiveScenario = $Scenario
    $script:CampaignPhase = "scenario-$Scenario"
    [void](Test-CampaignCaptureHealth $capture)
    # Wait for operator READY first; READY latency must not count toward the 60s window.
    $prepareNonce = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $readyPromptAt = Get-Now
    Write-Host "PREPARE scenario=$Scenario nonce=$prepareNonce"
    $ready = Read-OperatorResponse $Scenario 'ready' "READY $prepareNonce" "请准备场景 $Scenario 的可见界面，然后逐字输入 READY $prepareNonce"
    Update-CampaignCapture $capture
    $anchorByte = [long]$capture.ReadOffset
    $windowStartedAt = Get-Now
    $actionNonce = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $actionPromptAt = Get-Now
    Write-Host "ACTION scenario=$Scenario nonce=$actionNonce action=$Instruction"
    $actionError = $null
    try {
        if ($null -ne $OnAction) { & $OnAction }
        $ack = Read-OperatorResponse $Scenario 'action' "ACK $actionNonce" "完成上述 ACTION 后，逐字输入 ACK $actionNonce"
    } catch {
        $actionError = $_
        $ack = [pscustomobject]@{ Valid = $false; AnsweredAt = Get-Now; TimedOut = $false; DelaySeconds = 0.0 }
    }
    $requiredObservationEnd = $ack.AnsweredAt.AddSeconds($script:WindowSeconds)
    $script:CurrentWindowEnd = $requiredObservationEnd
    if ($LiveSimulation) {
        # Append the full scenario event stream once, then reveal only offset<=elapsed events to
        # DuringWait as the virtual clock advances. Future destroy/stop markers must not arm probes early.
        Add-SimulationScenarioOutput $capture $Scenario $actionPromptAt
        Update-CampaignCapture $capture
        while ((Get-Now) -lt $requiredObservationEnd -and -not $capture.Degraded) {
            $currentEvents = @(Get-ScenarioWindowEvents $capture $anchorByte $actionPromptAt (Get-Now))
            if ($null -ne $DuringWait) { & $DuringWait -CurrentEvents $currentEvents }
            if ((Get-Now) -ge $requiredObservationEnd -or $capture.Degraded) { break }
            $now = Get-Now
            $nextAt = $requiredObservationEnd
            foreach ($ev in @($capture.Events)) {
                if ([long]$ev.raw_byte_start -lt $anchorByte) { continue }
                if ([string]::IsNullOrWhiteSpace([string]$ev.device_observed_at)) { continue }
                $dt = [DateTimeOffset]::MinValue
                if (-not [DateTimeOffset]::TryParse([string]$ev.device_observed_at, [ref]$dt)) { continue }
                if ($dt -gt $now -and $dt -lt $nextAt) { $nextAt = $dt }
            }
            if ((Get-Now) -ge $requiredObservationEnd) { break }
            if ($nextAt -le (Get-Now)) {
                Wait-Until $requiredObservationEnd
                break
            }
            Wait-Until $nextAt
        }
        if ((Get-Now) -lt $requiredObservationEnd -and -not $capture.Degraded) { Wait-Until $requiredObservationEnd }
        Update-CampaignCapture $capture
    } else {
        while ((Get-Now) -lt $requiredObservationEnd -and -not $capture.Degraded) {
            Update-CampaignCapture $capture
            $currentEvents = @(Get-ScenarioWindowEvents $capture $anchorByte $actionPromptAt (Get-Now))
            if ($null -ne $DuringWait) { & $DuringWait -CurrentEvents $currentEvents }
            Start-Sleep -Milliseconds 250
        }
        Update-CampaignCapture $capture
    }
    $observedThrough = Get-Now
    $windowEvents = @(Get-ScenarioWindowEvents $capture $anchorByte $actionPromptAt $observedThrough)
    if ($null -ne $DuringWait) { & $DuringWait -CurrentEvents $windowEvents }
    $script:CurrentWindowEnd = $null
    [void](Test-CampaignCaptureHealth $capture)
    # Continuous HiLog health only. Screen/layout/fault artifact degradation is tracked on
    # CaptureDegraded/CaptureArtifacts and must not collapse the observation window or abort later scenarios.
    $windowDegraded = [bool]$capture.Degraded
    $coverageAfterAck = if ($null -ne $capture.LastHealthyAt) { [Math]::Max(0.0, ($capture.LastHealthyAt - $ack.AnsweredAt).TotalSeconds) } else { 0.0 }
    $completeWindowObserved = -not $windowDegraded -and $coverageAfterAck -ge $script:WindowSeconds -and $observedThrough -ge $requiredObservationEnd
    $observation = [ordered]@{
        scenario = $Scenario
        campaign_capture_started_at = $capture.StartedAt.ToString('o')
        initial_anchor = $capture.InitialAnchor
        scenario_anchor_byte = $anchorByte
        window_started_at = $windowStartedAt.ToString('o')
        ready_prompt_at = $readyPromptAt.ToString('o')
        ready_ack_at = $ready.AnsweredAt.ToString('o')
        action_prompt_at = $actionPromptAt.ToString('o')
        action_ack_at = $ack.AnsweredAt.ToString('o')
        required_observation_end_at = $requiredObservationEnd.ToString('o')
        observation_ended_at = $observedThrough.ToString('o')
        action_interval_seconds = ($ack.AnsweredAt - $actionPromptAt).TotalSeconds
        measured_coverage_before_action_prompt_seconds = ($actionPromptAt - $windowStartedAt).TotalSeconds
        measured_coverage_after_ack_seconds = $coverageAfterAck
        complete_window_observed = [bool]$completeWindowObserved
        ready_valid = [bool]$ready.Valid
        action_ack_valid = [bool]$ack.Valid
        capture_degraded = [bool]$windowDegraded
        capture_health = [ordered]@{
            process_present = [bool]($null -ne $capture.Process)
            process_exited = $(if ($null -ne $capture.Process) { try { [bool]$capture.Process.HasExited } catch { $true } } else { [bool]$capture.SimulatedDead })
            stderr_bytes = [long]$capture.LastStderrBytes
            last_healthy_at = $(if ($null -ne $capture.LastHealthyAt) { $capture.LastHealthyAt.ToString('o') } else { $null })
            measured = $true
        }
        device_clock_skew_tolerance_seconds = [double]$script:DeviceClockSkewToleranceSeconds
        events = Protect-SensitiveData $windowEvents
    }
    Add-TranscriptRecord 'scenario-observation' $observation
    $capture.ActiveScenario = 0
    if ($null -ne $actionError) { throw $actionError }
    return [pscustomobject]@{ Observation = $observation; Events = $windowEvents; ReadyValid = [bool]$ready.Valid; AckValid = [bool]$ack.Valid; CaptureDegraded = [bool]$windowDegraded; CompleteWindowObserved = [bool]$completeWindowObserved }
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
    if ($DenyScreenshot -and $FullWindowObserved) { return [pscustomobject]@{ result = 'pass'; reason = 'deny-screenshot-and-ACK-plus-60-without-B-create' } }
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
            $nextProbeAt = ([DateTimeOffset]::Parse([string]$Context.LastProbeAt)).AddSeconds($spacingSeconds)
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
            $Context.Terminal = $true
            $Context.Finished = $true
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

function Assert-ScenarioCaptureCanContinue {
    param([Parameter(Mandatory)]$Results, [Parameter(Mandatory)]$Observation)
    $script:PartialScenarios = @($Results)
    if (-not $Observation.CaptureDegraded) { return }
    $scenarioNumber = [int]$Observation.Observation.scenario
    $entries = @($script:CaptureDegraded | Where-Object {
        [int]$_.scenario -in @($scenarioNumber, 0) -and [string]$_.component -match 'raw-hilog'
    })
    $infraEntries = @($entries | Where-Object {
        [string]$_.category -eq 'infrastructure' -or [string]$_.infrastructure_reason -eq 'hdc-usb-interruption'
    })
    if ($infraEntries.Count -gt 0) {
        $detail = Protect-SensitiveText ([string]$infraEntries[0].reason)
        $script:InfrastructureReasonObserved = 'hdc-usb-interruption'
        throw "scenario-$scenarioNumber continuous capture infrastructure failure: $detail"
    }
    $detail = if ($entries.Count -gt 0) { Protect-SensitiveText ([string]$entries[0].reason) } else { 'continuous capture degraded' }
    throw "scenario-$scenarioNumber continuous capture non-infrastructure blocked: $detail"
}

function Invoke-Capture {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Scenario, [switch]$ObservationOnly)
    $operations = @('ScreenCap', 'DumpLayout', 'ReceiveScreen', 'ReceiveLayout')
    $failures = [Collections.Generic.List[string]]::new()
    if ($null -ne $script:CampaignCapture -and $script:CampaignCapture.Degraded) {
        $failures.Add('continuous-hilog-capture-degraded')
    } else {
        foreach ($operation in $operations) {
            $result = Invoke-HdcOperation $operation @{ Name = $Name } -AllowFailure
            if ($result.ExitCode -ne 0) { $failures.Add("$operation-exit-$($result.ExitCode)") }
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
    $script:CaptureArtifacts.Add($artifact)
    if ($status -eq 'degraded') {
        if ($ObservationOnly) {
            # Observation-only captures (e.g. the Settings>VPN page in scenario 5) never enter the
            # global CaptureDegraded list: their loss is recorded as an independent diagnostic and
            # must not block the scenario or the final overall aggregation. The failure is still
            # visible in CaptureArtifacts (status=degraded) and in observation_only_degraded.
            $script:ObservationOnlyDegraded.Add([ordered]@{ scenario = $Scenario; name = $Name; status = 'degraded'; failures = @($failures); screen_path = $screenPath; layout_path = $layoutPath })
        } else {
            # Screen/layout degradation is non-infrastructure evidence loss; do not mark continuous Capture.Degraded.
            Add-CaptureDegradation $script:CampaignCapture 'screen-layout-capture' ($failures -join ',') -Scenario $Scenario -Category 'non-infrastructure' -MarkContinuousDegraded $false
        }
    }
    return $status
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

function Invoke-LiveCampaign {
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
    if ($campaignCapture.Degraded) {
        $infraPrep = @($script:CaptureDegraded | Where-Object {
            [string]$_.category -eq 'infrastructure' -or [string]$_.infrastructure_reason -eq 'hdc-usb-interruption'
        }).Count -gt 0
        if ($infraPrep) { $script:InfrastructureReasonObserved = 'hdc-usb-interruption' }
        throw 'collection preparation blocked: continuous capture unavailable before scenario-1 installation'
    }
    $scenario1State = [pscustomobject]@{ FirstBaselineQueryAt = $null; InstallCompletedAt = $null }
    $scenario1Action = {
        $scenario1State.FirstBaselineQueryAt = Get-Now
        foreach ($bundle in @($script:BundleA, $script:BundleB)) {
            $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
            if ((Get-HdcCombinedText $dumpResult) -notmatch 'failed to get information|not exist|not found') { throw "cleanup baseline failed: bundle already installed or query unavailable: $bundle" }
            $processResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
            if ($processResult.ExitCode -ne 0 -and -not [string]::IsNullOrWhiteSpace($processResult.Stderr)) { throw "cleanup baseline pid query failed: $bundle" }
            if (-not [string]::IsNullOrWhiteSpace($processResult.Stdout)) { throw "cleanup baseline failed: process exists: $bundle" }
        }
        [void](Invoke-HdcOperation 'RemoveStaging' -AllowFailure)
        $baselineProbe = Invoke-HdcOperation 'StagingProbe' -AllowFailure
        if (-not (Test-StagingAbsent $baselineProbe)) { throw 'cleanup baseline failed: staging residual still present after RemoveStaging' }
        # Set before MkdirStaging so any later failure still runs fixed staging cleanup + probe in finally.
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
        $scenario1State.InstallCompletedAt = Get-Now
    }
    $observation1 = Invoke-ScenarioObservation 1 '场景1：确认冻结清理基线；runner 将自动查询/暂存并安装测试 App A 与 B（A/B 为两个测试 App），你无需在手机上手动安装；完成后 ACK' $scenario1Action
    $capture1 = Invoke-Capture 'scenario-1-baseline' 1
    $window1StartedAt = [DateTimeOffset]::Parse([string]$observation1.Observation.window_started_at)
    $queryCovered = $null -ne $scenario1State.FirstBaselineQueryAt -and $window1StartedAt -le $scenario1State.FirstBaselineQueryAt
    $installSeconds = if ($null -ne $scenario1State.InstallCompletedAt) { ($scenario1State.InstallCompletedAt - $window1StartedAt).TotalSeconds } else { [double]::PositiveInfinity }
    $installWithin60 = $installSeconds -le 60.0
    $scenario1Result = if ($observation1.ReadyValid -and $observation1.AckValid -and $observation1.CompleteWindowObserved -and -not $observation1.CaptureDegraded -and $queryCovered -and $installWithin60 -and $script:InstalledA -and $script:InstalledB -and $capture1 -eq 'collected') { 'pass' } else { 'blocked' }
    $results.Add([ordered]@{ sequence_index = 1; scenario = 1; result = $scenario1Result; reason = 'cleanup-baseline-and-install'; first_baseline_query_covered = [bool]$queryCovered; install_elapsed_seconds = $installSeconds; install_completed_within_60_seconds = [bool]$installWithin60; observation = $observation1.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation1

    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $authCaptureState = [pscustomobject]@{ AuthUiVisible = $false; Status = 'not-run'; Name = 'scenario-2-authorization' }
    $scenario2Action = {
        $authCaptureState.AuthUiVisible = Confirm-VisibleFact 2 'AUTH-UI-VISIBLE' '仅当系统授权界面在选择 Allow 前已可见时确认为真。'
        if ($authCaptureState.AuthUiVisible) {
            $authCaptureState.Status = Invoke-Capture $authCaptureState.Name 2
        } else {
            $authCaptureState.Status = 'degraded'
        }
    }
    $observation2 = Invoke-ScenarioObservation 2 '场景2：在测试 App A 点 Start；出现系统授权界面后，先按 runner 提示完成 AUTH-UI-VISIBLE 事实确认并等待 runner 截取完成，再点 Allow，然后 ACK' $scenario2Action
    $capture2 = Invoke-Capture 'scenario-2-allow' 2
    $request2 = Get-RequestIdFromEvents $observation2.Events $script:BundleA
    $authCaptureAssertion = if ($authCaptureState.AuthUiVisible -and $authCaptureState.Status -eq 'collected') { 'pass' } else { 'blocked' }
    $allowAssertion = if ($observation2.AckValid) { 'pass' } else { 'blocked' }
    $onCreateAssertion = if ($request2 -and (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_ONCREATE')) { 'pass' } else { 'blocked' }
    $fdAssertion = if ($request2 -and (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_RESOLVED') -and
        (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'CREATE_ACCEPTED') -and
        (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_FD_SNAPSHOT')) { 'pass' } else { 'blocked' }
    # An observed create rejection/invalid-fd on the S2-bound request is an explicit functional fail
    # (create invalid) and outranks capture/window/operator degradation; it is never downgraded to
    # blocked. Missing evidence stays blocked and is never promoted to fail.
    $createRejected2 = $request2 -and ((Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_REJECTED') -or
        (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'VPN_CREATE_INVALID_FD') -or
        (Test-CorrelatedMarker $observation2.Events $script:BundleA $request2 'START_PROMISE_REJECTED'))
    $scenario2Result = if ($createRejected2 -or 'fail' -in @($allowAssertion, $onCreateAssertion, $fdAssertion)) { 'fail' } elseif ($observation2.CaptureDegraded -or -not $observation2.CompleteWindowObserved -or $capture2 -ne 'collected' -or $authCaptureAssertion -eq 'blocked') { 'blocked' } elseif ('blocked' -in @($allowAssertion, $onCreateAssertion, $fdAssertion)) { 'blocked' } else { 'pass' }
    $s2Reason = if ($createRejected2) { 'create-rejected-after-allow' } elseif ('fail' -in @($allowAssertion, $onCreateAssertion, $fdAssertion)) { 'allow-onCreate-create-fd-fail' } else { 'allow-onCreate-create-fd' }
    $results.Add([ordered]@{
        sequence_index = 2; scenario = 2; result = $scenario2Result; reason = $s2Reason; bundle = $script:BundleA; request_id = $request2
        assertions = [ordered]@{ allow = $allowAssertion; vpn_on_create = $onCreateAssertion; vpn_connection_create_fd = $fdAssertion }
        authorization_capture = [ordered]@{ name = $authCaptureState.Name; status = $authCaptureState.Status; auth_ui_visible = [bool]$authCaptureState.AuthUiVisible; result = $authCaptureAssertion }
        observation = $observation2.Observation
    })
    Assert-ScenarioCaptureCanContinue $results $observation2

    $script:ProbeContexts[3] = New-ProcessProbeContext -Scenario 3 -Bundle $script:BundleA -RequireBundlePresent $true -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
    $duringScenario3 = {
        param([object[]]$CurrentEvents)
        # Start the strict-process-boundary probe series only when the fallback marker prerequisites
        # are already visible for the S2-bound request (unique stop + onDestroy + destroy-begin/pre snapshot).
        $probeCtx = $script:ProbeContexts[3]
        if ([bool]$probeCtx.Finished -or [bool]$probeCtx.Aborted) { return }
        if ([string]::IsNullOrWhiteSpace([string]$request2)) { return }
        $prereq = Test-StrictFallbackPrerequisites -Events $CurrentEvents -Bundle $script:BundleA -RequestId $request2
        if (-not $prereq.Met) { return }
        [void](Invoke-ProcessFinalStateProbeSeries $probeCtx $script:CurrentWindowEnd)
    }
    $observation3 = Invoke-ScenarioObservation 3 '场景3：在当前已激活的测试 App A 的 Entry 界面点 Stop，然后 ACK' $null $duringScenario3
    $capture3 = Invoke-Capture 'scenario-3-stop' 3
    # S3 is hard-bound to S2 request2; missing binding is blocked (no window-event inference substitute).
    $final3 = if ([string]::IsNullOrWhiteSpace([string]$request2)) {
        [pscustomobject]@{ result = 'blocked'; reason = 'active-request-unresolved'; terminal_mode = $null; callback = $null; strict = $null }
    } else {
        Get-VpnFinalState -Events $observation3.Events -Bundle $script:BundleA -RequestId $request2 -ProbeState $script:ProbeContexts[3] -RequireBundlePresent $true -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
    }
    $scenario3Result = if ($final3.result -eq 'fail') { 'fail' } elseif ($observation3.CaptureDegraded -or -not $observation3.CompleteWindowObserved -or $capture3 -ne 'collected') { 'blocked' } elseif (-not $observation3.AckValid -or $final3.result -ne 'pass') { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{
        sequence_index = 3; scenario = 3; result = $scenario3Result; reason = $final3.reason; bundle = $script:BundleA; request_id = $request2
        terminal_mode = $final3.terminal_mode
        process_final_state_probes = @($script:ProbeContexts[3].Probes)
        bundle_present_during_probe = [bool]$script:ProbeContexts[3].BundlePresent
        clean_reactivation_proof = $false
        observation = $observation3.Observation
    })
    Assert-ScenarioCaptureCanContinue $results $observation3

    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleB })
    $observation4 = Invoke-ScenarioObservation 4 '场景4：在测试 App B 点 Start，在可见的系统授权界面选择 Deny，保持拒绝画面不要关闭，然后 ACK'
    $capture4 = Invoke-Capture 'scenario-4-deny' 4
    $denyScreen = Confirm-VisibleFact 4 'DENY-SCREEN-CAPTURED' '仅当拒绝画面截图清晰可见时确认为真。'
    $request4 = Get-RequestIdFromEvents $observation4.Events $script:BundleB
    $fullDenyWindow = [bool]$observation4.CompleteWindowObserved
    $deny4 = Get-DenyAssessment $observation4.Events $script:BundleB $request4 $denyScreen $fullDenyWindow
    # An observed B create after deny is an explicit functional fail (deny-then-create / dual-active
    # class) and outranks capture/window/operator degradation; it is never downgraded to blocked.
    $scenario4Result = if ($deny4.result -eq 'fail') { 'fail' } elseif ($observation4.CaptureDegraded -or -not $fullDenyWindow -or $capture4 -ne 'collected') { 'blocked' } elseif (-not $observation4.AckValid) { 'blocked' } else { $deny4.result }
    $results.Add([ordered]@{ sequence_index = 4; scenario = 4; result = $scenario4Result; reason = $deny4.reason; bundle = $script:BundleB; request_id = $request4; deny_screen = [bool]$denyScreen; full_window_after_ack = [bool]$fullDenyWindow; observation = $observation4.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation4

    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $s5State = [pscustomobject]@{
        PathActualDirect = $false
        PathActualReauth = $false
        VpnPageVisible = $false
        VpnPageCaptureStatus = 'not-run'
        ForceStopConfirmed = $false
        ForceStopCaptureStatus = 'not-run'
    }
    $scenario5Action = {
        $s5State.PathActualDirect = Confirm-VisibleFact 5 'PATH-ACTUAL-DIRECT-SYSTEM-ACTIVATION' '仅当实际 re-allow 路径为 direct-system-activation 时确认为真。'
        $s5State.PathActualReauth = Confirm-VisibleFact 5 'PATH-ACTUAL-SYSTEM-REAUTHORIZATION-UI' '仅当实际 re-allow 路径为 system-reauthorization-UI 时确认为真。'
        # Settings>VPN page is observation-only: screenshot + fields, never a pass/blocked gate.
        $s5State.VpnPageVisible = Confirm-VisibleFact 5 'SETTINGS-VPN-PAGE-VISIBLE' '仅当手机系统设置 → 更多连接 → VPN 页面已可见时确认为真（仅观察，不影响结果）。'
        if ($s5State.VpnPageVisible) { $s5State.VpnPageCaptureStatus = Invoke-Capture 'scenario-5-settings-vpn-page' 5 -ObservationOnly } else { $s5State.VpnPageCaptureStatus = 'degraded' }
        # Manual Settings>app info>A>force stop is the revoke mechanism; separate screenshot + confirmation.
        $s5State.ForceStopConfirmed = Confirm-VisibleFact 5 'SETTINGS-APP-INFO-FORCE-STOP-CAPTURED' '仅当已在系统设置 → 应用 → 测试 App A 的应用信息页执行强制停止且画面可见时确认为真。'
        if ($s5State.ForceStopConfirmed) { $s5State.ForceStopCaptureStatus = Invoke-Capture 'scenario-5-app-info-force-stop' 5 } else { $s5State.ForceStopCaptureStatus = 'degraded' }
    }
    $script:ProbeContexts[5] = New-ProcessProbeContext -Scenario 5 -Bundle $script:BundleA -RequireBundlePresent $true -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
    $duringScenario5 = {
        param([object[]]$CurrentEvents)
        # After the manual force-stop is confirmed, probe the bundle: pidof/bundledump observation only,
        # never HDC force-stop. No UI_STOP is expected or required on the settings-app-info-force-stop path.
        if (-not $s5State.ForceStopConfirmed) { return }
        [void](Invoke-ProcessFinalStateProbeSeries $script:ProbeContexts[5] $script:CurrentWindowEnd)
    }
    $observation5 = Invoke-ScenarioObservation 5 "场景5：先在测试 App A 点 Start 重新激活（预测 re-allow 路径 '$($Freeze.settings_reallow_expected_path)'，路径偏差仅观察、不改判定）；随后打开手机系统设置 App（齿轮）→ 更多连接 → VPN 页并保持可见，等 runner 截取；再进入 设置 → 应用 → 测试 App A 的应用信息页执行强制停止并保持画面，等 runner 截取；连续采集保持至 ACK 后再 60 秒" $scenario5Action $duringScenario5
    $actualReallowPath = if ($s5State.PathActualDirect -and -not $s5State.PathActualReauth) {
        'direct-system-activation'
    } elseif ($s5State.PathActualReauth -and -not $s5State.PathActualDirect) {
        'system-reauthorization-UI'
    } elseif ($s5State.PathActualDirect -and $s5State.PathActualReauth) {
        'ambiguous'
    } else {
        'unobserved'
    }
    $pathMatch = $actualReallowPath -eq [string]$Freeze.settings_reallow_expected_path
    $pathObservation = if ($pathMatch) {
        'actual-path-matched-expected'
    } elseif ($actualReallowPath -in @('direct-system-activation', 'system-reauthorization-UI')) {
        'actual-path-deviated-from-expected-observation-only'
    } else {
        'actual-path-not-confirmed'
    }
    $settingsReallowPath = [ordered]@{
        expected = [string]$Freeze.settings_reallow_expected_path
        actual = $actualReallowPath
        match = [bool]$pathMatch
        observation = $pathObservation
        policy = [string]$Freeze.settings_reallow_path_policy
    }
    $request5 = Get-RequestIdFromEvents $observation5.Events $script:BundleA
    $onCreate5 = $request5 -and (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_ONCREATE')
    $create5 = $request5 -and (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_CREATE_RESOLVED') -and
        (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'CREATE_ACCEPTED')
    $freshCreateProof = $request5 -and (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'CREATE_ACCEPTED') -and
        (Test-PostCreateOpen $observation5.Events $script:BundleA $request5)
    $s5ProbeCtx = $script:ProbeContexts[5]
    $bundlePresentDuringProbe = [bool]$s5ProbeCtx.BundlePresent
    # Post-destroy FD_STILL_OPEN on the current request is a hard fail and can never be overridden
    # by consecutive-absent process probes; pre-destroy open snapshots never count as fail.
    $s5FdStillOpen = Test-S5PostDestroyStillOpen $observation5.Events $script:BundleA $request5
    # Force-stop assessment re-checks recorded probe timestamps (>=2 consecutive absent, first-to-last
    # spacing >= freeze spacing); execution-time Wait/Terminal flags alone are never trusted, so a
    # probe_spacing_override below the freeze spacing stays blocked.
    $absentEvidence = Test-ProcessAbsentEvidence $s5ProbeCtx ([int]$freeze.process_absent_required_count) ([double]$freeze.process_absent_probe_spacing_seconds)
    $s5Reason = 'settings-app-info-force-stop'
    $scenario5Result = 'blocked'
    # A hard FD_STILL_OPEN fail outranks capture/window/operator degradation and is never downgraded.
    if ($s5FdStillOpen) { $scenario5Result = 'fail'; $s5Reason = 'FD_STILL_OPEN' }
    elseif ($observation5.CaptureDegraded -or -not $observation5.CompleteWindowObserved) { $s5Reason = 'observation-incomplete' }
    elseif ($s5ProbeCtx.Aborted) { $s5Reason = 'probe-unknown-or-error' }
    elseif (-not $observation5.AckValid) { $s5Reason = 'ack-invalid' }
    elseif (-not $s5State.ForceStopConfirmed) { $s5Reason = 'force-stop-not-confirmed' }
    elseif ($s5State.ForceStopCaptureStatus -ne 'collected') { $s5Reason = 'force-stop-capture-degraded' }
    elseif (-not $onCreate5) { $s5Reason = 'vpn-on-create-missing' }
    elseif (-not $create5) { $s5Reason = 'vpn-create-fd-missing' }
    elseif (-not $freshCreateProof) { $s5Reason = 'fresh-create-proof-missing' }
    elseif (-not $absentEvidence.Met) { $s5Reason = $absentEvidence.Reason }
    elseif (-not $bundlePresentDuringProbe) { $s5Reason = 'bundle-absent-during-probe' }
    else { $scenario5Result = 'pass'; $s5Reason = 'settings-app-info-force-stop-terminal' }
    $results.Add([ordered]@{
        sequence_index = 5; scenario = 5; result = $scenario5Result; reason = $s5Reason; bundle = $script:BundleA; request_id = $request5
        settings_revoke_mechanism = [string]$Freeze.settings_revoke_mechanism
        settings_vpn_page_policy = [string]$Freeze.settings_vpn_page_policy
        settings_vpn_page_observation_only = $true
        settings_vpn_page_capture = [ordered]@{ name = 'scenario-5-settings-vpn-page'; status = $s5State.VpnPageCaptureStatus; visible = [bool]$s5State.VpnPageVisible }
        app_info_force_stop_capture = [ordered]@{ name = 'scenario-5-app-info-force-stop'; status = $s5State.ForceStopCaptureStatus; confirmed = [bool]$s5State.ForceStopConfirmed }
        force_stop_confirmed = [bool]$s5State.ForceStopConfirmed
        terminal_mode = 'settings-app-info-force-stop'
        fd_still_open = [bool]$s5FdStillOpen
        process_final_state_probes = @($s5ProbeCtx.Probes)
        process_absent_evidence = [ordered]@{ met = [bool]$absentEvidence.Met; reason = $absentEvidence.Reason; required_count = [int]$freeze.process_absent_required_count; required_spacing_seconds = [double]$freeze.process_absent_probe_spacing_seconds; measured_spacing_seconds = $absentEvidence.SpacingSeconds }
        bundle_present_during_probe = [bool]$bundlePresentDuringProbe
        settings_reallow_path = $settingsReallowPath
        assertions = [ordered]@{ vpn_on_create = $(if ($onCreate5) { 'pass' } else { 'blocked' }); vpn_connection_create_fd = $(if ($create5) { 'pass' } else { 'blocked' }); fresh_create_proof = $(if ($freshCreateProof) { 'pass' } else { 'blocked' }); force_stop = $(if ($s5State.ForceStopConfirmed) { 'pass' } else { 'blocked' }) }
        observation = $observation5.Observation
    })
    $s3Entry = @($results | Where-Object { [int]$_.scenario -eq 3 })[0]
    if ($null -ne $s3Entry) { $s3Entry.clean_reactivation_proof = [bool]$freshCreateProof }
    Assert-ScenarioCaptureCanContinue $results $observation5

    $observation6 = Invoke-ScenarioObservation 6 '场景6：先在测试 App A 点 Start 激活；再在测试 App B 点 Start；若系统画面出现替换/取消等选择，不要提前点选，先保留当前画面并按 runner 推进；连续采集保持至 ACK 后再 60 秒'
    $capture6 = Invoke-Capture 'scenario-6-conflict' 6
    # S6 operator three-state: NO-DUAL-ACTIVE-CAPTURED is asked first; only when it is false is the
    # independent DUAL-ACTIVE-CAPTURED confirmation asked (true only when A and B are clearly both
    # active on screen). dual=true && noDual=false is the only operator fail; noDual=true &&
    # dual=false is normal; both false is blocked dual-active-observation-unresolved; both true is
    # blocked inconsistent-operator-confirmation. An empty/false answer alone never fails S6.
    $noDual = Confirm-VisibleFact 6 'NO-DUAL-ACTIVE-CAPTURED' '仅当最终可见状态为画面上未同时出现 A 与 B 两个 active VPN 时确认为真。'
    $dualActive = $false
    if ($LiveSimulation) {
        # Simulation fixtures may pre-set both confirmations so the full four-state matrix is
        # exercised (including the defensive both-true inconsistent state). Live only asks
        # DUAL-ACTIVE-CAPTURED when NO-DUAL-ACTIVE-CAPTURED is false.
        $dualActive = Get-SimulationConfirmation 'DUAL-ACTIVE-CAPTURED'
    } elseif (-not $noDual) {
        $dualActive = Confirm-VisibleFact 6 'DUAL-ACTIVE-CAPTURED' '仅当明确看到 A 与 B 两个 active VPN 同时出现在画面上时确认为真。'
    }
    $operatorState = if ($noDual -and -not $dualActive) { 'normal' }
        elseif (-not $noDual -and $dualActive) { 'dual-active-observed' }
        elseif (-not $noDual -and -not $dualActive) { 'dual-active-observation-unresolved' }
        else { 'inconsistent-operator-confirmation' }
    $request6A = Get-RequestIdFromEvents $observation6.Events $script:BundleA
    $request6B = Get-RequestIdFromEvents $observation6.Events $script:BundleB
    $bUiStartObserved = $null -ne $request6B
    $aAccepted = $request6A -and (Test-CorrelatedMarker $observation6.Events $script:BundleA $request6A 'CREATE_ACCEPTED')
    $bRejected = $request6B -and ((Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'START_PROMISE_REJECTED') -or (Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'VPN_CREATE_REJECTED'))
    $bAccepted = $request6B -and (Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'CREATE_ACCEPTED')
    $replacementDestroy = if ($bAccepted) { Get-DestroyAssessment $observation6.Events $script:BundleA $request6A } else { [pscustomobject]@{ result = 'pass'; reason = 'B-rejected-no-replacement-destroy-required' } }
    # Explicit functional fails outrank capture/window/operator degradation and are never downgraded
    # to blocked: a failed replacement destroy (A still holds an open fd after B was accepted) and an
    # operator-confirmed dual-active visible state (dual=true && noDual=false) are both observed
    # fails. They must be evaluated before operator unresolved/inconsistent and capture/window blocked
    # reasons (including no-new-B-UI_START / observation-incomplete). Missing evidence stays blocked
    # and is never promoted to fail; an unresolved or inconsistent operator confirmation without a
    # functional fail is its own blocked reason, never a fail.
    $scenario6Result = 'blocked'
    $s6Reason = 'scenario-6-evidence-incomplete'
    if ($replacementDestroy.result -eq 'fail' -or $operatorState -eq 'dual-active-observed') {
        $scenario6Result = 'fail'
        $s6Reason = if ($replacementDestroy.result -eq 'fail') { [string]$replacementDestroy.reason } else { 'dual-active-observed' }
    } elseif ($operatorState -eq 'dual-active-observation-unresolved') {
        $s6Reason = 'dual-active-observation-unresolved'
    } elseif ($operatorState -eq 'inconsistent-operator-confirmation') {
        $s6Reason = 'inconsistent-operator-confirmation'
    } elseif (-not $bUiStartObserved) {
        $s6Reason = 'no-new-B-UI_START'
    } elseif ($observation6.CaptureDegraded -or -not $observation6.CompleteWindowObserved -or $capture6 -ne 'collected') {
        $s6Reason = 'observation-incomplete'
    } elseif (-not $observation6.AckValid -or -not $aAccepted -or -not $request6B -or (-not $bRejected -and -not $bAccepted) -or $replacementDestroy.result -ne 'pass') {
        $s6Reason = [string]$replacementDestroy.reason
    } else {
        $scenario6Result = 'pass'
        $s6Reason = [string]$replacementDestroy.reason
    }
    $results.Add([ordered]@{ sequence_index = 6; scenario = 6; result = $scenario6Result; reason = $s6Reason; request_id_a = $request6A; request_id_b = $request6B; a_accepted = [bool]$aAccepted; b_rejected = [bool]$bRejected; b_accepted = [bool]$bAccepted; no_dual_active_confirmed = [bool]$noDual; dual_active_confirmed = [bool]$dualActive; operator_state = $operatorState; observation = $observation6.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation6

    $activeBundle = if ($bAccepted) { $script:BundleB } else { $script:BundleA }
    $activeRequest = if ($bAccepted) { $request6B } else { $request6A }
    $script:ProbeContexts[7] = New-ProcessProbeContext -Scenario 7 -Bundle $activeBundle -RequireBundlePresent $false -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
    $cleanupState = [pscustomobject]@{ Done = $false; Verified = $false; CompletedAt = $null; FaultDegraded = $false; TerminalAssessed = $false; TerminalMode = $null }
    $duringScenario7 = {
        param([object[]]$CurrentEvents)
        if ($cleanupState.Done) { return }
        $probeCtx = $script:ProbeContexts[7]
        # Terminal assessment completes before any uninstall cleanup is allowed. Get-VpnFinalState is
        # the single terminal evaluator: callback terminal + post-destroy fd snapshot first,
        # FD_STILL_OPEN is a hard fail that never falls back, otherwise the strict-process-boundary
        # route (unique stop + onDestroy + begin/pre snapshot + consecutive absent pre-uninstall
        # probes). The callback/FD/fallback ladder is never re-implemented here;
        # Test-StrictFallbackPrerequisites only gates whether to adopt the probe series.
        # Finally-absent must never backfill these probes.
        # RequestId is bound to the actual active A/B request from S6; null inference is forbidden.
        if ([string]::IsNullOrWhiteSpace([string]$activeRequest)) { return }
        $stopCandidate = Get-StopRequestFromEvents -Events $CurrentEvents -ExpectedBundle $activeBundle
        if ($null -eq $stopCandidate -or [string]$stopCandidate.RequestId -ne [string]$activeRequest) { return }
        $rid = [string]$activeRequest
        $final = Get-VpnFinalState -Events $CurrentEvents -Bundle $activeBundle -RequestId $rid -ProbeState $probeCtx -RequireBundlePresent $false -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
        if ($final.result -eq 'pass') {
            $cleanupState.TerminalMode = [string]$final.terminal_mode
        } elseif ($final.result -eq 'fail') {
            # FD_STILL_OPEN is a hard fail: never uninstall over a leaked fd; bundles stay untouched.
            return
        } elseif (-not [bool]$probeCtx.Started -and (Test-StrictFallbackPrerequisites -Events $CurrentEvents -Bundle $activeBundle -RequestId $rid).Met) {
            # Strict fallback eligible: adopt the window-bound probe series, then re-evaluate with
            # the same unique evaluator. Only a pass authorizes uninstall; fail/blocked leave
            # everything untouched.
            [void](Invoke-ProcessFinalStateProbeSeries $probeCtx $script:CurrentWindowEnd)
            $final = Get-VpnFinalState -Events $CurrentEvents -Bundle $activeBundle -RequestId $rid -ProbeState $probeCtx -RequireBundlePresent $false -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
            if ($final.result -ne 'pass') { return }
            $cleanupState.TerminalMode = [string]$final.terminal_mode
        } else {
            # blocked: prerequisites missing, or probes already run but insufficient/aborted. Terminal
            # assessment is not complete; leave bundles untouched and keep waiting for later events.
            return
        }
        $cleanupState.TerminalAssessed = $true
        $preStatus = Invoke-Capture 'scenario-7-pre-uninstall' 7
        foreach ($faultOperation in @('FaultA', 'FaultB')) {
            if ((Invoke-FaultArtifact $faultOperation 7) -ne 'collected') { $cleanupState.FaultDegraded = $true }
        }
        $verified = $preStatus -eq 'collected'
        if ($script:InstalledB) {
            $uninstallBResult = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleB } -AllowFailure
            $script:CleanupActions.Add([ordered]@{ operation = 'Uninstall'; bundle = $script:BundleB; exit_code = $uninstallBResult.ExitCode })
            if ($uninstallBResult.ExitCode -eq 0) { $script:InstalledB = $false } else { $verified = $false }
        }
        if ($script:InstalledA) {
            $uninstallAResult = Invoke-HdcOperation 'Uninstall' @{ Bundle = $script:BundleA } -AllowFailure
            $script:CleanupActions.Add([ordered]@{ operation = 'Uninstall'; bundle = $script:BundleA; exit_code = $uninstallAResult.ExitCode })
            if ($uninstallAResult.ExitCode -eq 0) { $script:InstalledA = $false } else { $verified = $false }
        }
        if ($script:StagingSent -or $script:StagingMayExist) {
            if (-not (Invoke-RemoveStagingVerified 'RemoveStaging')) { $verified = $false }
        }
        foreach ($bundle in @($script:BundleA, $script:BundleB)) {
            $dumpResult = Invoke-HdcOperation 'BundleDump' @{ Bundle = $bundle } -AllowFailure
            $processResult = Invoke-HdcOperation 'PidOf' @{ Bundle = $bundle } -AllowFailure
            if ((Get-HdcCombinedText $dumpResult) -notmatch 'failed to get information|not exist|not found' -or -not [string]::IsNullOrWhiteSpace($processResult.Stdout)) { $verified = $false }
        }
        $cleanupState.Done = $true
        $cleanupState.Verified = $verified
        $cleanupState.CompletedAt = (Get-Now).ToString('o')
    }
    $observation7 = Invoke-ScenarioObservation 7 '场景7：在当前仍 active 的测试 App（A 或 B）界面点 Stop；不要手工强停或卸载；runner 负责后续清理' $null $duringScenario7
    # No post-window DuringWait/probe rerun: late stop is conservatively blocked; probes stay inside the window only.
    # S7 is hard-bound to the calculated active A/B request; null must not fall back to window-event inference.
    $final7 = if ([string]::IsNullOrWhiteSpace([string]$activeRequest)) {
        [pscustomobject]@{ result = 'blocked'; reason = 'active-request-unresolved'; terminal_mode = $null; callback = $null; strict = $null }
    } else {
        Get-VpnFinalState -Events $observation7.Events -Bundle $activeBundle -RequestId $activeRequest -ProbeState $script:ProbeContexts[7] -RequireBundlePresent $false -RequiredCount ([int]$freeze.process_absent_required_count) -SpacingSeconds ([double]$freeze.process_absent_probe_spacing_seconds)
    }
    $capture7Name = if ($cleanupState.Done) { 'scenario-7-post-cleanup' } else { 'scenario-7-final-state' }
    $capture7 = Invoke-Capture $capture7Name 7
    $cleanupVisible = Confirm-VisibleFact 7 'FINAL-CLEANUP-CAPTURED' '仅当已无 active VPN 或测试配置残留时确认为真。'
    $scenario7Result = if ($final7.result -eq 'fail') { 'fail' } elseif ($observation7.CaptureDegraded -or -not $observation7.CompleteWindowObserved -or $capture7 -ne 'collected') { 'blocked' } elseif (-not $observation7.AckValid -or -not $cleanupState.Done -or -not $cleanupState.Verified -or -not $cleanupVisible -or $cleanupState.FaultDegraded -or $final7.result -ne 'pass') { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{
        sequence_index = 7; scenario = 7; result = $scenario7Result; reason = $final7.reason; active_bundle = $activeBundle; request_id = $activeRequest
        terminal_mode = $final7.terminal_mode
        terminal_assessed = [bool]$cleanupState.TerminalAssessed
        terminal_mode_at_cleanup = $cleanupState.TerminalMode
        process_final_state_probes = @($script:ProbeContexts[7].Probes)
        bundle_present_during_probe = [bool]$script:ProbeContexts[7].BundlePresent
        cleanup_completed_at = $cleanupState.CompletedAt
        post_cleanup_capture = [bool]$cleanupState.Done
        post_cleanup_capture_name = $capture7Name
        bundle_process_cleanup_verified = [bool]$cleanupState.Verified
        visible_cleanup_confirmed = [bool]$cleanupVisible
        fault_capture_degraded = [bool]$cleanupState.FaultDegraded
        observation = $observation7.Observation
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
        [Parameter(Mandatory)][string]$ManifestSha256
    )
    $scenario2 = @($Scenarios | Where-Object { [int]$_.scenario -eq 2 })[0]
    $s3Record = @($Scenarios | Where-Object { [int]$_.scenario -eq 3 })[0]
    $isEvidence = $script:ExecutionMode -eq 'live'
    # Non-evidence modes (dry-run/live-simulation) stay blocked unless the measured aggregation is an
    # explicit fail: a hard fail (e.g. post-destroy FD_STILL_OPEN) must never be downgraded to blocked.
    if (-not $isEvidence -and $Overall -ne 'fail') { $Overall = 'blocked'; $RecordStatus = 'blocked' }
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
        freeze_contract_sha256 = $FreezeContractSha256
        preflight_inputs_frozen_at = $Freeze.preflight_inputs_frozen_at
        scenario_window_seconds = 60
        observation_semantics = 'one continuous campaign HiLog capture; pre-scenario byte anchors exclude prior buffer; device_observed_at bounds action prompt through measured ACK plus at least 60 seconds; frozen CST=>+08:00 zone map; device clock skew tolerance 3s; READY latency excluded from scenario-1 60s install window; scenario 3/7 terminal prefers callback destroy terminal plus post-destroy fd snapshot, otherwise strict-process-boundary needs unique stop/onDestroy/destroy-begin plus consecutive absent host process probes (>=2, >=3s apart, bundle present for scenario 3); scenario 5 revokes via manual Settings app-info force-stop with confirmation and consecutive absent probes; scenario 5 Settings>VPN page capture is observation-only (its degradation is recorded in observation_only_degraded and never blocks the scenario or overall); scenario 6 operator dual-active confirmation is three-state (no_dual_active_confirmed/dual_active_confirmed: only dual=true && noDual=false fails, both false is blocked dual-active-observation-unresolved, both true is blocked inconsistent-operator-confirmation, empty/false alone never fails); probe results are recorded before any cleanup and never backfilled from finally'
        settings_reallow_expected_path = $Freeze.settings_reallow_expected_path
        settings_reallow_path_policy = $Freeze.settings_reallow_path_policy
        settings_revoke_mechanism = $Freeze.settings_revoke_mechanism
        settings_vpn_page_policy = $Freeze.settings_vpn_page_policy
        settings_vpn_page_observation_only = $true
        destroy_terminal_policy = $Freeze.destroy_terminal_policy
        process_absent_required_count = [int]$Freeze.process_absent_required_count
        process_absent_probe_spacing_seconds = [double]$Freeze.process_absent_probe_spacing_seconds
        cleanup_baseline = 'A/B absent; no A/B process; no active VPN; unrelated VPN isolated; staging removed before send'
        scenarios = @($Scenarios)
        scenario_aggregation = [ordered]@{
            mapping = '1=cleanup_and_install; 2=allow_and_fd; 3=active_stop; 4=deny; 5=settings_revoke; 6=second_vpn_conflict; 7=final_cleanup'
            scenario_2_rule = 'overall is pass only when allow, vpn_on_create, and vpn_connection_create_fd are all pass; fail dominates blocked'
            scenario_2_assertions = $(if ($null -ne $scenario2) { $scenario2.assertions } else { $null })
            scenario_5_rule = 'settings-app-info-force-stop revoke: fresh create/open plus manual force-stop confirmation plus bundle present plus consecutive absent probes; Settings VPN page is observation-only and never blocks'
            scenario_6_rule = 'explicit functional fails first: replacementDestroy fail or dual-active-observed outrank operator unresolved/inconsistent and capture/window blocked; operator dual-active confirmation is three-state: only dual_active_confirmed=true && no_dual_active_confirmed=false fails (dual-active-observed); noDual=true && dual=false is normal; both false is blocked dual-active-observation-unresolved; both true is blocked inconsistent-operator-confirmation; empty/false alone never fails'
            scenario_7_rule = 'uninstall cleanup is allowed only after the scenario terminal assessment completes (callback or strict-process-boundary with pre-uninstall probes); finally-absent never backfills terminal probes'
            s3_strict_process_boundary_gate = 'scenario 3 strict-process-boundary fallback pass additionally requires scenario 5 same-bundle fresh request CREATE_ACCEPTED plus post-create open (clean_reactivation_proof); without it overall stays blocked'
            s3_clean_reactivation_proof = $(if ($null -ne $s3Record) { [bool](Get-OptionalProperty $s3Record 'clean_reactivation_proof' $false) } else { $false })
            overall_rule = 'any scenario fail => fail; else any scenario blocked => blocked; all seven scenarios pass => pass; scenario 3 strict-process-boundary without clean reactivation proof => blocked; evidence integrity violation => invalid'
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
    }
    if (-not [string]::IsNullOrEmpty($Failure)) { $record['failure'] = $Failure }
    if (-not [string]::IsNullOrEmpty($InfrastructureReason)) { $record['infrastructure_reason'] = $InfrastructureReason }
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
    foreach ($artifact in $script:CaptureArtifacts) {
        if ($artifact.status -eq 'collected' -and (-not (Test-Path -LiteralPath $artifact.screen_path -PathType Leaf) -or -not (Test-Path -LiteralPath $artifact.layout_path -PathType Leaf))) {
            $violations.Add("capture-reference-missing:$($artifact.name)")
        }
    }
    return @($violations | Select-Object -Unique)
}

function Get-FailureClassification {
    param([Parameter(Mandatory)][string]$Message)
    if ($Message.StartsWith('FUNCTIONAL_FAIL', [StringComparison]::Ordinal)) {
        return [pscustomobject]@{ Overall = 'fail'; RecordStatus = 'collected'; InfrastructureReason = $null; RetryAuthorized = $false }
    }
    if ($Message.StartsWith('RUNNER_HOST_FAILURE', [StringComparison]::Ordinal)) {
        return [pscustomobject]@{ Overall = 'blocked'; RecordStatus = 'blocked'; InfrastructureReason = 'runner-host-failure'; RetryAuthorized = $true }
    }
    # Generic capture_degraded / time-parse / missing artifacts are non-infrastructure. Only real capture
    # process exit/stderr/start/timeout and HDC transport failures authorize USB retry.
    if ($Message -match '(?i)exit\s*=\s*(124|125)|\bHDC(?:\s+operation)?\s+timeout\b|HDC infrastructure interruption|HDC Process\.Start|\bUSB\b|\boffline\b|\bdisconnect(?:ed)?\b|transport (?:offline|error|fail)|target.+not found|connect(?:ion)?.+fail|channel.+fail|continuous capture infrastructure failure|raw-hilog-(?:start|process|stderr)|capture process exited|unable to start continuous campaign capture') {
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
        if (($globalDegradation -or [int]$scenario.scenario -in $affected) -and [string]$scenario.result -ne 'fail') {
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
    $script:PublicVersionLiterals = @('PLA-AL10 7.0.0.100(SP8C00E32R7P2)')
    $versionRedaction = Protect-SensitiveText 'build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=192.0.2.44|port=8710'
    Check ($versionRedaction.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $versionRedaction.Contains('api=26') -and $versionRedaction -notmatch '192\.0\.2\.44|8710') 'redaction-preserves-build-api-and-removes-ip-port'
    $api26IpLike = Protect-SensitiveText 'full_system_build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=198.51.100.77|port=8710|bare=7.0.0.100'
    Check ($api26IpLike.Contains('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $api26IpLike.Contains('api=26') -and $api26IpLike -match 'bare=<REDACTED_IPV4>' -and $api26IpLike -notmatch '198\.51\.100\.77|8710') 'api26-build-ip-like-literal-preserved-real-ip-redacted'
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
    Check ($script:HdcProcessStartCount -eq 0) 'SelfTest-zero-HDC-processes'
    if ($failures.Count -gt 0) { throw "self-test failures: $($failures -join ', ')" }
    Write-Host 'SELFTEST_RESULT=pass HDC_PROCESSES=0'
}

if ($SelfTest) {
    Invoke-RunnerSelfTest
    exit 0
}

if ($DryRun -and $LiveSimulation) { throw 'DryRun and LiveSimulation are mutually exclusive' }
if ([string]::IsNullOrWhiteSpace($FreezeManifest) -or [string]::IsNullOrWhiteSpace($EvidenceRoot) -or [string]::IsNullOrWhiteSpace($HapA) -or [string]::IsNullOrWhiteSpace($HapB) -or [string]::IsNullOrWhiteSpace($HdcPath)) {
    throw 'FreezeManifest, EvidenceRoot, HapA, HapB, and HdcPath are required unless SelfTest is used'
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
$freezeSha256 = Get-FileSha256 $freezePath
$script:PublicVersionLiterals = @([string]$freeze.target_tuple.full_system_build, [string]$freeze.sdk.version)
Assert-FreezeManifest $freeze $freezePath
$freezeContractSha256 = Get-FreezeContractSha256 $freeze
$repositoryBefore = Get-RepositoryState
if ([string]$freeze.code_sha -ne $repositoryBefore.Head) { throw 'freeze code_sha does not match repository HEAD' }
if (-not $DryRun -and -not $repositoryBefore.Clean) { throw 'Live and LiveSimulation require a clean repository state' }
if (-not $script:NoDeviceMode) { Assert-TargetEnvironment }
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
    Initialize-PriorBlockedBinding $freeze
    if ($DryRun) {
        $scenarios = Invoke-DryRunCampaign
        $overall = 'blocked'
        $recordStatus = 'blocked'
    } else {
        $scenarios = Invoke-LiveCampaign $freeze
        $measuredOverall = Get-ScenarioAggregation $scenarios
        if ($LiveSimulation) {
            $overall = 'blocked'
            $recordStatus = 'blocked'
        } else {
            $overall = $measuredOverall
            $recordStatus = 'collected'
        }
    }
} catch {
    $rawException = [string]$_.Exception.Message
    $phase = [string]$script:CampaignPhase
    if ($phase -match '^scenario-([1-7])$' -and $rawException -notmatch 'scenario-[1-7]') {
        $rawException = "scenario-$($Matches[1]) $rawException"
    } elseif ($phase -eq 'preflight' -and $rawException -notmatch '(?i)^preflight\b|collection preparation blocked|scenario-[1-7]') {
        $rawException = "preflight: $rawException"
    }
    $fatalMessage = Protect-SensitiveText $rawException
    $classification = Get-FailureClassification $fatalMessage
    $overall = if ($script:ExecutionMode -eq 'live') { $classification.Overall } else { 'blocked' }
    $recordStatus = if ($script:ExecutionMode -eq 'live') { $classification.RecordStatus } else { 'blocked' }
    $infrastructureReason = $classification.InfrastructureReason
    $failedScenario = if ($fatalMessage -match 'scenario-([1-7])') { [int]$Matches[1] } else { $null }
    $scenarios = New-BlockedScenarios $(if ($phase -eq 'preflight' -or $fatalMessage -match '(?i)^preflight\b|collection preparation blocked') { 'preflight-or-collection-preparation-blocked' } else { 'not-run-after-runner-failure' })
    foreach ($partialScenario in @($script:PartialScenarios)) {
        # Preserve already-measured scenario results; preflight/exception must not overwrite them.
        $scenarios[[int]$partialScenario.scenario - 1] = $partialScenario
    }
    if ($null -ne $failedScenario) {
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
    if ($script:CaptureDegraded.Count -gt 0 -or (-not $DryRun -and -not [bool]$script:CleanupVerification.verified_absent)) {
        # Capture degradation / cleanup uncertainty never downgrades an explicit scenario fail: only
        # non-fail aggregation collapses to blocked, so a leaked fd (FD_STILL_OPEN) still fails overall.
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
        schema_version = 1
        evidence_id = $freeze.evidence_id
        campaign_id = $freeze.campaign_id
        attempt = $freeze.attempt
        execution_mode = $script:ExecutionMode
        operator_role = $freeze.operator_role
        attested = [bool]($script:ExecutionMode -eq 'live' -and $null -eq $fatalMessage)
        statement = 'All device UI actions were manual and visible; no automated device input or privileged bypass was used.'
        record_status = $(if ($script:ExecutionMode -eq 'live') { 'collected' } else { 'blocked' })
        reviewer = 'pending'
        reviewed_at = 'pending'
    }
    Write-JsonFile (Join-Path $script:EvidencePath 'operator-attestation.json') $attestation
    $manifestPath = Write-CollectionManifest $script:EvidencePath
    $manifestSha256 = Get-FileSha256 $manifestPath
    $record = New-CompleteRecord $freeze $scenarios $overall $recordStatus $startedAt $endedAt $fatalMessage $infrastructureReason $repositoryBefore $freezeSha256 $freezeContractSha256 $manifestSha256
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
        if ($script:ExecutionMode -eq 'live') {
            $record.record_status = 'invalidated'
            $record.overall = 'invalid'
            $record.verdict = 'invalid'
            $record.scenario_aggregation.overall = 'invalid'
            $overall = 'invalid'
            $recordStatus = 'invalidated'
        } else {
            $record.record_status = 'blocked'
            $record.overall = 'blocked'
            $record.verdict = 'blocked'
            $record.scenario_aggregation.overall = 'blocked'
            $overall = 'blocked'
            $recordStatus = 'blocked'
        }
        Write-JsonFile $recordPath $record
        Write-CampaignSeal $script:EvidencePath
    }
}

if ($script:NoDeviceMode -and $script:HdcProcessStartCount -ne 0) { throw 'host-only safety invariant violated: HDC process count is nonzero' }
Write-Host "RUNNER_RESULT=$overall RECORD_STATUS=$recordStatus MODE=$($script:ExecutionMode) EVIDENCE_ROOT=$($script:EvidencePath) RAW_ROOT_HASH=$(Get-TextSha256 $script:RawPath) HDC_PROCESSES=$($script:HdcProcessStartCount)"
if ($null -ne $fatalMessage -or $overall -eq 'invalid') { exit 2 }
exit 0
