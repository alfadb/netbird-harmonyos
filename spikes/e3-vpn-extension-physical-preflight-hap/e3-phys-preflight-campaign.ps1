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
    if ($LiveSimulation) {
        Add-SimulationScenarioOutput $capture $Scenario $actionPromptAt
        Update-CampaignCapture $capture
        $currentEvents = @(Get-ScenarioWindowEvents $capture $anchorByte $actionPromptAt $requiredObservationEnd)
        if ($null -ne $DuringWait) { & $DuringWait -CurrentEvents $currentEvents }
        if (-not $capture.Degraded) { Wait-Until $requiredObservationEnd }
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

function Get-ScenarioAggregation {
    param([Parameter(Mandatory)][object[]]$Scenarios, [bool]$IntegrityViolation = $false)
    if ($IntegrityViolation) { return 'invalid' }
    $results = @($Scenarios | ForEach-Object { [string]$_.result })
    if ($results -contains 'fail') { return 'fail' }
    if ($results -contains 'blocked') { return 'blocked' }
    if ($results.Count -ne 7 -or @($results | Where-Object { $_ -ne 'pass' }).Count -gt 0) { return 'invalid' }
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
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][int]$Scenario)
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
        # Screen/layout degradation is non-infrastructure evidence loss; do not mark continuous Capture.Degraded.
        Add-CaptureDegradation $script:CampaignCapture 'screen-layout-capture' ($failures -join ',') -Scenario $Scenario -Category 'non-infrastructure' -MarkContinuousDegraded $false
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
    $scenario2Result = if ($observation2.CaptureDegraded -or -not $observation2.CompleteWindowObserved -or $capture2 -ne 'collected' -or $authCaptureAssertion -eq 'blocked') { 'blocked' } elseif ('fail' -in @($allowAssertion, $onCreateAssertion, $fdAssertion)) { 'fail' } elseif ('blocked' -in @($allowAssertion, $onCreateAssertion, $fdAssertion)) { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{
        sequence_index = 2; scenario = 2; result = $scenario2Result; reason = 'allow-onCreate-create-fd'; bundle = $script:BundleA; request_id = $request2
        assertions = [ordered]@{ allow = $allowAssertion; vpn_on_create = $onCreateAssertion; vpn_connection_create_fd = $fdAssertion }
        authorization_capture = [ordered]@{ name = $authCaptureState.Name; status = $authCaptureState.Status; auth_ui_visible = [bool]$authCaptureState.AuthUiVisible; result = $authCaptureAssertion }
        observation = $observation2.Observation
    })
    Assert-ScenarioCaptureCanContinue $results $observation2

    $observation3 = Invoke-ScenarioObservation 3 '场景3：在当前已激活的测试 App A 的 Entry 界面点 Stop，然后 ACK'
    $capture3 = Invoke-Capture 'scenario-3-stop' 3
    $destroy3 = Get-DestroyAssessment $observation3.Events $script:BundleA $request2
    $stop3 = $request2 -and (Test-CorrelatedMarker $observation3.Events $script:BundleA $request2 'STOP_PROMISE_RESOLVED')
    $onDestroy3 = $request2 -and (Test-CorrelatedMarker $observation3.Events $script:BundleA $request2 'VPN_ONDESTROY')
    $scenario3Result = if ($observation3.CaptureDegraded -or -not $observation3.CompleteWindowObserved -or $capture3 -ne 'collected') { 'blocked' } elseif ($destroy3.result -eq 'fail') { 'fail' } elseif (-not $observation3.AckValid -or -not $stop3 -or -not $onDestroy3 -or $destroy3.result -ne 'pass') { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{ sequence_index = 3; scenario = 3; result = $scenario3Result; reason = $destroy3.reason; bundle = $script:BundleA; request_id = $request2; observation = $observation3.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation3

    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleB })
    $observation4 = Invoke-ScenarioObservation 4 '场景4：在测试 App B 点 Start，在可见的系统授权界面选择 Deny，保持拒绝画面不要关闭，然后 ACK'
    $capture4 = Invoke-Capture 'scenario-4-deny' 4
    $denyScreen = Confirm-VisibleFact 4 'DENY-SCREEN-CAPTURED' '仅当拒绝画面截图清晰可见时确认为真。'
    $request4 = Get-RequestIdFromEvents $observation4.Events $script:BundleB
    $fullDenyWindow = [bool]$observation4.CompleteWindowObserved
    $deny4 = Get-DenyAssessment $observation4.Events $script:BundleB $request4 $denyScreen $fullDenyWindow
    $scenario4Result = if ($observation4.CaptureDegraded -or -not $fullDenyWindow -or $capture4 -ne 'collected') { 'blocked' } elseif ($deny4.result -eq 'fail') { 'fail' } elseif (-not $observation4.AckValid) { 'blocked' } else { $deny4.result }
    $results.Add([ordered]@{ sequence_index = 4; scenario = 4; result = $scenario4Result; reason = $deny4.reason; bundle = $script:BundleB; request_id = $request4; deny_screen = [bool]$denyScreen; full_window_after_ack = [bool]$fullDenyWindow; observation = $observation4.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation4

    [void](Invoke-HdcOperation 'StartEntry' @{ Bundle = $script:BundleA })
    $observation5 = Invoke-ScenarioObservation 5 "场景5：先在测试 App A 点 Start 重新激活（预测 re-allow 路径 '$($Freeze.settings_reallow_expected_path)'，路径偏差仅观察、不改判定）；再打开手机系统设置 App（齿轮）→ 更多连接 → VPN，在其中断开/删除测试 VPN；连续采集保持至 ACK 后再 60 秒"
    $capture5 = Invoke-Capture 'scenario-5-settings' 5
    $pathActualDirect = Confirm-VisibleFact 5 'PATH-ACTUAL-DIRECT-SYSTEM-ACTIVATION' '仅当实际 re-allow 路径为 direct-system-activation 时确认为真。'
    $pathActualReauth = Confirm-VisibleFact 5 'PATH-ACTUAL-SYSTEM-REAUTHORIZATION-UI' '仅当实际 re-allow 路径为 system-reauthorization-UI 时确认为真。'
    $actualReallowPath = if ($pathActualDirect -and -not $pathActualReauth) {
        'direct-system-activation'
    } elseif ($pathActualReauth -and -not $pathActualDirect) {
        'system-reauthorization-UI'
    } elseif ($pathActualDirect -and $pathActualReauth) {
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
    $settingsConfirmed = Confirm-VisibleFact 5 'SETTINGS-REVOKE-CAPTURED' '仅当已在普通系统设置中可见地完成 VPN 断开/删除时确认为真。'
    $request5 = Get-RequestIdFromEvents $observation5.Events $script:BundleA
    $onCreate5 = $request5 -and (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_ONCREATE')
    $create5 = $request5 -and (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_CREATE_RESOLVED') -and
        (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'CREATE_ACCEPTED') -and
        (Test-CorrelatedMarker $observation5.Events $script:BundleA $request5 'VPN_FD_SNAPSHOT')
    $destroy5 = Get-DestroyAssessment $observation5.Events $script:BundleA $request5
    # Path match/mismatch is observation-only under settings_reallow_path_policy=observation-only; functional gates alone decide pass/blocked/fail.
    $scenario5Result = if ($observation5.CaptureDegraded -or -not $observation5.CompleteWindowObserved -or $capture5 -ne 'collected') { 'blocked' } elseif ($destroy5.result -eq 'fail') { 'fail' } elseif (-not $observation5.AckValid -or -not $settingsConfirmed -or -not $onCreate5 -or -not $create5 -or $destroy5.result -ne 'pass') { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{
        sequence_index = 5; scenario = 5; result = $scenario5Result; reason = $destroy5.reason; bundle = $script:BundleA; request_id = $request5
        settings_reallow_path = $settingsReallowPath
        settings_revoke_captured = [bool]$settingsConfirmed
        assertions = [ordered]@{ vpn_on_create = $(if ($onCreate5) { 'pass' } else { 'blocked' }); vpn_connection_create_fd = $(if ($create5) { 'pass' } else { 'blocked' }); settings_revoke = $(if ($settingsConfirmed) { 'pass' } else { 'blocked' }); destroy_cleanup = $destroy5.result }
        observation = $observation5.Observation
    })
    Assert-ScenarioCaptureCanContinue $results $observation5

    $observation6 = Invoke-ScenarioObservation 6 '场景6：先在测试 App A 点 Start 激活；再在测试 App B 点 Start；若系统画面出现替换/取消等选择，不要提前点选，先保留当前画面并按 runner 推进；连续采集保持至 ACK 后再 60 秒'
    $capture6 = Invoke-Capture 'scenario-6-conflict' 6
    $noDual = Confirm-VisibleFact 6 'NO-DUAL-ACTIVE-CAPTURED' '仅当最终可见状态为画面上未同时出现 A 与 B 两个 active VPN 时确认为真。'
    $request6A = Get-RequestIdFromEvents $observation6.Events $script:BundleA
    $request6B = Get-RequestIdFromEvents $observation6.Events $script:BundleB
    $bUiStartObserved = $null -ne $request6B
    $aAccepted = $request6A -and (Test-CorrelatedMarker $observation6.Events $script:BundleA $request6A 'CREATE_ACCEPTED')
    $bRejected = $request6B -and ((Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'START_PROMISE_REJECTED') -or (Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'VPN_CREATE_REJECTED'))
    $bAccepted = $request6B -and (Test-CorrelatedMarker $observation6.Events $script:BundleB $request6B 'CREATE_ACCEPTED')
    $replacementDestroy = if ($bAccepted) { Get-DestroyAssessment $observation6.Events $script:BundleA $request6A } else { [pscustomobject]@{ result = 'pass'; reason = 'B-rejected-no-replacement-destroy-required' } }
    if (-not $bUiStartObserved) {
        $scenario6Result = 'blocked'
        $s6Reason = 'no-new-B-UI_START'
    } else {
        $scenario6Result = if ($observation6.CaptureDegraded -or -not $observation6.CompleteWindowObserved -or $capture6 -ne 'collected') { 'blocked' } elseif ($replacementDestroy.result -eq 'fail') { 'fail' } elseif (-not $observation6.AckValid -or -not $noDual -or -not $aAccepted -or -not $request6B -or (-not $bRejected -and -not $bAccepted) -or $replacementDestroy.result -ne 'pass') { 'blocked' } else { 'pass' }
        $s6Reason = $replacementDestroy.reason
    }
    $results.Add([ordered]@{ sequence_index = 6; scenario = 6; result = $scenario6Result; reason = $s6Reason; request_id_a = $request6A; request_id_b = $request6B; a_accepted = [bool]$aAccepted; b_rejected = [bool]$bRejected; b_accepted = [bool]$bAccepted; no_dual_active = [bool]$noDual; observation = $observation6.Observation })
    Assert-ScenarioCaptureCanContinue $results $observation6

    $activeBundle = if ($bAccepted) { $script:BundleB } else { $script:BundleA }
    $activeRequest = if ($bAccepted) { $request6B } else { $request6A }
    $cleanupState = [pscustomobject]@{ Done = $false; Verified = $false; CompletedAt = $null; FaultDegraded = $false }
    $duringScenario7 = {
        param([object[]]$CurrentEvents)
        if ($cleanupState.Done) { return }
        # Require a unique current-window stop marker for the expected bundle consistent with active request.
        # Old destroy-only chains must not start cleanup; multi-id conflict falls through to finally.
        $stopCandidate = Get-StopRequestFromEvents -Events $CurrentEvents -ExpectedBundle $activeBundle
        if ($null -eq $stopCandidate) { return }
        if (-not [string]::IsNullOrWhiteSpace([string]$activeRequest) -and [string]$stopCandidate.RequestId -ne [string]$activeRequest) { return }
        $assessment = Get-DestroyAssessment $CurrentEvents $activeBundle ([string]$stopCandidate.RequestId)
        if ($assessment.result -ne 'pass') { return }
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
    if (-not $cleanupState.Done) { & $duringScenario7 -CurrentEvents $observation7.Events }
    $stopInfo = Get-StopRequestFromEvents -Events $observation7.Events -ExpectedBundle $activeBundle
    if ($null -ne $stopInfo -and -not [string]::IsNullOrWhiteSpace([string]$activeRequest) -and [string]$stopInfo.RequestId -ne [string]$activeRequest) {
        $stopInfo = $null
    }
    $s7Request = if ($null -ne $stopInfo) { [string]$stopInfo.RequestId } else { $null }
    $s7Bundle = if ($null -ne $stopInfo -and -not [string]::IsNullOrWhiteSpace([string]$stopInfo.Bundle)) { [string]$stopInfo.Bundle } else { $activeBundle }
    $capture7Name = if ($cleanupState.Done) { 'scenario-7-post-cleanup' } else { 'scenario-7-final-state' }
    $capture7 = Invoke-Capture $capture7Name 7
    $cleanupVisible = Confirm-VisibleFact 7 'FINAL-CLEANUP-CAPTURED' '仅当已无 active VPN 或测试配置残留时确认为真。'
    $destroy7 = Get-DestroyAssessment $observation7.Events $s7Bundle $s7Request
    $scenario7Result = if ($observation7.CaptureDegraded -or -not $observation7.CompleteWindowObserved -or $capture7 -ne 'collected') { 'blocked' } elseif ($destroy7.result -eq 'fail') { 'fail' } elseif (-not $observation7.AckValid -or -not $cleanupState.Done -or -not $cleanupState.Verified -or -not $cleanupVisible -or $cleanupState.FaultDegraded -or $destroy7.result -ne 'pass') { 'blocked' } else { 'pass' }
    $results.Add([ordered]@{ sequence_index = 7; scenario = 7; result = $scenario7Result; reason = $destroy7.reason; active_bundle = $s7Bundle; request_id = $s7Request; cleanup_completed_at = $cleanupState.CompletedAt; post_cleanup_capture = [bool]$cleanupState.Done; post_cleanup_capture_name = $capture7Name; bundle_process_cleanup_verified = [bool]$cleanupState.Verified; visible_cleanup_confirmed = [bool]$cleanupVisible; fault_capture_degraded = [bool]$cleanupState.FaultDegraded; observation = $observation7.Observation })
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
    $isEvidence = $script:ExecutionMode -eq 'live'
    if (-not $isEvidence) { $Overall = 'blocked'; $RecordStatus = 'blocked' }
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
        observation_semantics = 'one continuous campaign HiLog capture; pre-scenario byte anchors exclude prior buffer; device_observed_at bounds action prompt through measured ACK plus at least 60 seconds; frozen CST=>+08:00 zone map; device clock skew tolerance 3s; READY latency excluded from scenario-1 60s install window'
        settings_reallow_expected_path = $Freeze.settings_reallow_expected_path
        settings_reallow_path_policy = $Freeze.settings_reallow_path_policy
        cleanup_baseline = 'A/B absent; no A/B process; no active VPN; unrelated VPN isolated; staging removed before send'
        scenarios = @($Scenarios)
        scenario_aggregation = [ordered]@{
            mapping = '1=cleanup_and_install; 2=allow_and_fd; 3=active_stop; 4=deny; 5=settings_revoke; 6=second_vpn_conflict; 7=final_cleanup'
            scenario_2_rule = 'overall is pass only when allow, vpn_on_create, and vpn_connection_create_fd are all pass; fail dominates blocked'
            scenario_2_assertions = $scenario2.assertions
            overall_rule = 'any scenario fail => fail; else any scenario blocked => blocked; all seven scenarios pass => pass; evidence integrity violation => invalid'
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
    foreach ($scenario in $Scenarios) {
        if ($globalDegradation -or [int]$scenario.scenario -in $affected) {
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
    if ($script:CaptureDegraded.Count -gt 0 -or (-not $DryRun -and -not [bool]$script:CleanupVerification.verified_absent)) {
        $overall = 'blocked'
        $recordStatus = 'blocked'
    } elseif ($script:ExecutionMode -eq 'live') {
        $overall = Get-ScenarioAggregation $scenarios
        $recordStatus = 'collected'
    } else {
        $overall = 'blocked'
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
