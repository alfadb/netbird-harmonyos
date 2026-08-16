#requires -Version 7.0
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$project = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$sourceRunner = Join-Path $project 'e3-phys-preflight-campaign.ps1'
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('e3-phys-preflight-selftest-' + [guid]::NewGuid().ToString('N'))
$repo = Join-Path $tempRoot 'runner-repository'
[IO.Directory]::CreateDirectory($repo) | Out-Null
$runner = Join-Path $repo 'e3-phys-preflight-campaign.ps1'
Copy-Item -LiteralPath $sourceRunner -Destination $runner
& git -C $repo init --quiet | Out-Null
& git -C $repo config user.email 'e3-selftest@example.invalid'
& git -C $repo config user.name 'E3 Runner Selftest'
& git -C $repo add -- 'e3-phys-preflight-campaign.ps1'
& git -C $repo commit --quiet -m 'selftest runner snapshot'
if ($LASTEXITCODE -ne 0) { throw 'unable to create clean temporary runner repository' }
$script:CaseIndex = 0
$script:HdcLaunchMarker = Join-Path $tempRoot 'HDC-PROCESS-WAS-STARTED.txt'
$script:SentinelHdc = Join-Path $tempRoot 'HDC-MUST-NOT-START.cmd'
[IO.File]::WriteAllText($script:SentinelHdc, "@echo launched>$($script:HdcLaunchMarker)`r`n", [Text.ASCIIEncoding]::new())

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw "ASSERTION FAILED: $Message" }
}

function Write-FixtureFile {
    param([string]$Name, [string]$Content)
    $path = Join-Path $tempRoot $Name
    [IO.File]::WriteAllText($path, $Content, [Text.UTF8Encoding]::new($false))
    return $path
}

function Write-JsonFixture {
    param([string]$Name, $Value)
    $path = Join-Path $tempRoot $Name
    [IO.File]::WriteAllText($path, (($Value | ConvertTo-Json -Depth 60) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
    return $path
}

function Get-Sha256 {
    param([string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Copy-JsonObject {
    param($Value)
    return (($Value | ConvertTo-Json -Depth 60) | ConvertFrom-Json -Depth 60)
}

function New-CasePaths {
    param([string]$Name)
    $script:CaseIndex++
    $base = Join-Path $tempRoot ('case-' + $script:CaseIndex.ToString('00') + '-' + $Name)
    return [pscustomobject]@{ Evidence = $base + '-evidence'; Raw = $base + '-raw' }
}

function Invoke-Runner {
    param(
        [string]$FreezePath,
        [string]$EvidencePath,
        [string]$RawPath,
        [switch]$AsDryRun,
        [string]$FixturePath,
        [switch]$AsConfirm,
        [string]$ConfirmationRecordPath,
        [switch]$AsSelfTest,
        [switch]$IncludeRoots
    )
    $arguments = @(
        '-NoProfile', '-File', $runner,
        '-FreezeManifest', $FreezePath,
        '-HapA', $script:HapA,
        '-HapB', $script:HapB,
        '-HdcPath', $script:SentinelHdc
    )
    if ($AsSelfTest) { $arguments += '-SelfTest' }
    if ($AsConfirm) {
        $arguments += '-TargetBindingConfirm'
        if (-not [string]::IsNullOrWhiteSpace($ConfirmationRecordPath)) { $arguments += @('-ConfirmationRecord', $ConfirmationRecordPath) }
    }
    if ($AsDryRun) { $arguments += '-DryRun' }
    if (-not $AsSelfTest -and -not $AsDryRun) {
        if (-not [string]::IsNullOrWhiteSpace($FixturePath)) { $arguments += @('-LiveSimulation', '-SimulationFixture', $FixturePath) }
    }
    if (-not $AsSelfTest -and ($IncludeRoots -or -not $AsConfirm)) {
        if (-not [string]::IsNullOrWhiteSpace($EvidencePath)) { $arguments += @('-EvidenceRoot', $EvidencePath) }
        if (-not [string]::IsNullOrWhiteSpace($RawPath)) { $arguments += @('-RawRoot', $RawPath) }
    }
    $output = & pwsh @arguments 2>&1
    return [pscustomobject]@{ ExitCode = $LASTEXITCODE; Text = ($output -join "`n"); Lines = @($output) }
}

function New-Freeze {
    param([string]$PlanStatus = 'ready', [string]$EvidenceId = 'EV-E3-SELFTEST-20990101-0001', [string]$CampaignId = 'E3-PHYS-PREFLIGHT-SELFTEST')
    $head = (& git -C $repo rev-parse HEAD 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-f]{40}$') { throw 'unable to get repository HEAD' }
    return [ordered]@{
        schema_version = 2
        plan_status = $PlanStatus
        exception = 'E3-PHYS-PREFLIGHT'
        evidence_id = $EvidenceId
        campaign_id = $CampaignId
        attempt = 'initial'
        prior_blocked_binding = 'N/A'
        retry = [ordered]@{ basis = 'N/A'; infrastructure_reason = 'N/A'; prior_record_path = 'N/A'; prior_record_sha256 = 'N/A' }
        scenario_window_seconds = 60
        device_alias = 'PHYS-1'
        target_tuple = [ordered]@{
            distribution = 'HarmonyOS'
            device_model = 'PLA-AL10'
            full_system_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
            api = '26'
            kernel_arch = 'aarch64'
            app_abi = 'arm64-v8a'
        }
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
        artifact_sha256 = [ordered]@{ hap_a = Get-Sha256 $script:HapA; hap_b = Get-Sha256 $script:HapB }
        source = [ordered]@{
            archive_path = $script:SourceArchive
            archive_sha256 = Get-Sha256 $script:SourceArchive
            manifest_path = $script:SourceManifest
            manifest_sha256 = Get-Sha256 $script:SourceManifest
        }
        sdk = [ordered]@{
            version = 'synthetic-6.1.1'
            api = '24'
            syscap_basis = 'synthetic public VPN SysCap basis'
            files = @([ordered]@{ path = $script:SdkInput; sha256 = Get-Sha256 $script:SdkInput })
        }
        hdc = [ordered]@{ version = 'SELFTEST-HDC-1.0'; sha256 = Get-Sha256 $script:SentinelHdc }
        runner_sha256 = Get-Sha256 $runner
        code_sha = $head
        preflight_inputs_frozen_at = '2099-01-01T00:00:00+00:00'
        cleanup_baseline_frozen = $true
        collection_ready = $true
        independent_review_ready = $true
        independent_review_record = [ordered]@{ status = 'pending'; record_path = 'N/A'; record_sha256 = 'N/A'; reviewer_role = 'selftest-independent-reviewer' }
        operator_role = 'selftest-operator'
        independent_reviewer_role = 'selftest-independent-reviewer'
    }
}

function New-SimulationFixture {
    param(
        [object[]]$HdcFailures = @(),
        [string[]]$CaptureFailures = @(),
        [int]$CaptureDieScenario = 0,
        [bool]$TamperTranscript = $false,
        [bool]$TamperPayload = $false
    )
    $a = 'cn.alfadb.netbird.e3physvpna'
    $b = 'cn.alfadb.netbird.e3physvpnb'
    $stamp = '<DEVICE_OBSERVED_AT>'
    return [ordered]@{
        hdc_version = 'SELFTEST-HDC-1.0'
        capture_die_scenario = $CaptureDieScenario
        capture_initial_lines = @(
            '2098-12-31 23:59:58.000+00:00 UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4',
            '2098-12-31 23:59:59.000+00:00 VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4',
            '2098-12-31 23:59:59.500+00:00 UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5'
        )
        operator = [ordered]@{
            action_delay_seconds = 1
            no_effect_steps = @()
        }
        layout_reviews = [ordered]@{}
        layout_profiles = [ordered]@{}
        hdc_failures = @($HdcFailures)
        capture_failures = @($CaptureFailures)
        tamper_transcript_after_manifest = $TamperTranscript
        tamper_payload_after_manifest = $TamperPayload
        scenario_events = [ordered]@{
            '1' = @()
            '2' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_START|bundle=$a|requestId=a2" },
                [ordered]@{ offset_seconds = 2; text = "$stamp VPN_ONCREATE|bundle=$a|requestId=a2" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_CREATE_RESOLVED|requestId=a2|fd=42|accepted=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_FD_SNAPSHOT|requestId=a2|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 5; text = "$stamp CANARY|target=target-canary.example.test:8710|ipv4=10.23.45.67:8710|ipv6=[2001:db8::1234]:8710|host=device-canary.example.test:9911|mac=00:11:22:33:44:55|serial=SN-CANARY12345678" }
            )
            '3' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_STOP|bundle=$a|requestId=a2|basis=last-known-request" },
                [ordered]@{ offset_seconds = 2; text = "$stamp STOP_PROMISE_RESOLVED|bundle=$a|requestId=a2" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_ONDESTROY|requestId=a2" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy" },
                [ordered]@{ offset_seconds = 5; text = "$stamp VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_CLOSED_CONFIRMED" },
                [ordered]@{ offset_seconds = 6; text = "$stamp VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
            )
            '4' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_START|bundle=$b|requestId=b4" },
                [ordered]@{ offset_seconds = 2; text = "$stamp START_PROMISE_REJECTED|bundle=$b|requestId=b4|summary=denied" }
            )
            '5' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_START|bundle=$a|requestId=a5" },
                [ordered]@{ offset_seconds = 2; text = "$stamp VPN_ONCREATE|bundle=$a|requestId=a5" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 8; text = "$stamp VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
                [ordered]@{ offset_seconds = 9; text = "$stamp VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
            )
            '6' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_START|bundle=$a|requestId=a6" },
                [ordered]@{ offset_seconds = 2; text = "$stamp VPN_ONCREATE|bundle=$a|requestId=a6" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
                [ordered]@{ offset_seconds = 8; text = "$stamp UI_START|bundle=$b|requestId=b6" },
                [ordered]@{ offset_seconds = 9; text = "$stamp VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN" }
            )
            '7' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_STOP|bundle=$a|requestId=a6|basis=active-request" },
                [ordered]@{ offset_seconds = 2; text = "$stamp STOP_PROMISE_RESOLVED|bundle=$a|requestId=a6" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_ONDESTROY|requestId=a6" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy" },
                [ordered]@{ offset_seconds = 5; text = "$stamp VPN_DESTROY_RESOLVED|requestId=a6|fdMarker=FD_CLOSED_CONFIRMED" },
                [ordered]@{ offset_seconds = 6; text = "$stamp VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
            )
        }
    }
}

function New-S6BAuthorizationFixture {
    param([ValidateSet('frozen', 'accepted', 'none')][string]$Terminal = 'frozen')
    $fixture = New-SimulationFixture
    $fixture.layout_profiles['scenario-6-conflict'] = Get-Content -LiteralPath (Join-Path $project 'tests\fixtures\s6-b-authorization-production-0005.json') -Raw | ConvertFrom-Json -Depth 60
    $events = [Collections.Generic.List[object]]::new()
    $events.Add([ordered]@{ offset_seconds = 1; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" })
    $events.Add([ordered]@{ offset_seconds = 2; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" })
    $events.Add([ordered]@{ offset_seconds = 3; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" })
    $events.Add([ordered]@{ offset_seconds = 4; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" })
    $events.Add([ordered]@{ offset_seconds = 8; step_index = 3; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" })
    if ($Terminal -eq 'frozen') {
        $events.Add([ordered]@{ offset_seconds = 9; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict" })
    } elseif ($Terminal -eq 'accepted') {
        $events.Add([ordered]@{ offset_seconds = 9; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=b6|accepted=true|marker=CREATE_ACCEPTED" })
    }
    $fixture.scenario_events.'6' = @($events)
    return $fixture
}

function Assert-ManifestAndSeal {
    param([string]$EvidencePath)
    $manifestPath = Join-Path $EvidencePath 'hash-manifest.json'
    $recordPath = Join-Path $EvidencePath 'scenario-results.json'
    $sealPath = Join-Path $EvidencePath 'campaign-seal.json'
    foreach ($path in @($manifestPath, $recordPath, $sealPath, (Join-Path $EvidencePath 'projection\transcript.redacted.jsonl'))) {
        Assert-True (Test-Path -LiteralPath $path -PathType Leaf) "missing sealed output $path"
    }
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json -Depth 60
    foreach ($entry in @($manifest.files)) {
        $path = Join-Path $EvidencePath ([string]$entry.path).Replace('/', [IO.Path]::DirectorySeparatorChar)
        Assert-True ((Get-Sha256 $path) -eq [string]$entry.sha256) "manifest hash mismatch $($entry.path)"
    }
    $seal = Get-Content -LiteralPath $sealPath -Raw | ConvertFrom-Json -Depth 20
    Assert-True ((Get-Sha256 $recordPath) -eq [string]$seal.record.sha256) 'record seal mismatch'
    Assert-True ((Get-Sha256 $manifestPath) -eq [string]$seal.manifest.sha256) 'manifest seal mismatch'
}

function Assert-ProjectionChain {
    param([string]$EvidencePath)
    $path = Join-Path $EvidencePath 'projection\transcript.redacted.jsonl'
    $previousHash = ('0' * 64)
    $index = 1
    foreach ($line in (Get-Content -LiteralPath $path)) {
        $entry = $line | ConvertFrom-Json -Depth 60
        Assert-True ([int]$entry.payload.index -eq $index) 'projection index ordering mismatch'
        Assert-True ([string]$entry.payload.previous_hash -eq $previousHash) 'projection previous hash mismatch'
        $actual = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes([string]$entry.payload_canonical))).ToLowerInvariant()
        Assert-True ($actual -eq [string]$entry.entry_hash) 'projection entry hash mismatch'
        $previousHash = $actual
        $index++
    }
}

try {
    $script:HapA = Write-FixtureFile 'final-a.hap' 'synthetic signed HAP A fixture'
    $script:HapB = Write-FixtureFile 'final-b.hap' 'synthetic signed HAP B fixture distinct'
    $script:SourceArchive = Write-FixtureFile 'source.tar' 'synthetic source archive fixture'
    $script:SourceManifest = Write-FixtureFile 'source-manifest.json' '{"fixture":true}'
    $script:SdkInput = Write-FixtureFile 'sdk-input.bin' 'synthetic SDK fixture'

    Write-Host 'SELFTEST_PHASE=parser'
    $tokens = $null
    $parseErrors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($runner, [ref]$tokens, [ref]$parseErrors)
    Assert-True ($parseErrors.Count -eq 0) ('runner parser errors: ' + ($parseErrors -join '; '))
    $runnerText = Get-Content -LiteralPath $runner -Raw
    Assert-True ($runnerText -notmatch '(?i)\$(pid|args)\b') 'runner uses a PowerShell automatic variable name'

    Write-Host 'SELFTEST_PHASE=pure-fixtures'
    $pureOutput = & pwsh -NoProfile -File $runner -SelfTest 2>&1
    Assert-True ($LASTEXITCODE -eq 0 -and ($pureOutput -join "`n") -match 'SELFTEST_RESULT=pass HDC_PROCESSES=0') 'runner pure selftest failed'

    Write-Host 'SELFTEST_PHASE=source-marker-fixture-m1-m3'
    $uiSourcePath = Join-Path $project 'entry/src/main/ets/pages/Index.ets'
    $extensionSourcePath = Join-Path $project 'entry/src/main/ets/vpnextensionability/E3PhysicalVpnExtensionAbility.ets'
    Assert-True (Test-Path -LiteralPath $uiSourcePath -PathType Leaf) "Index.ets source missing: $uiSourcePath"
    Assert-True (Test-Path -LiteralPath $extensionSourcePath -PathType Leaf) "extension source missing: $extensionSourcePath"
    $uiSource = Get-Content -LiteralPath $uiSourcePath -Raw
    $extensionSource = Get-Content -LiteralPath $extensionSourcePath -Raw
    function Assert-SourceMarker {
        param([string]$Text, [string]$Marker, [string]$Message)
        Assert-True ($Text.Contains($Marker, [StringComparison]::Ordinal)) "$Message (missing marker: $Marker)"
    }
    # M1 UI ledger: API24 openSync+writeSync+closeSync before startVpnExtensionAbility; explicit error markers.
    Assert-SourceMarker $uiSource "import fs from '@ohos.file.fs';" 'UI lacks @ohos.file.fs import'
    Assert-SourceMarker $uiSource 'writeRequestLedger' 'UI lacks ledger writer'
    Assert-SourceMarker $uiSource 'LEDGER_PERSISTED' 'UI lacks LEDGER_PERSISTED marker'
    Assert-SourceMarker $uiSource 'LEDGER_WRITE_REJECTED' 'UI lacks LEDGER_WRITE_REJECTED marker'
    Assert-SourceMarker $uiSource 'isValidRequestId' 'UI lacks requestId validation'
    Assert-SourceMarker $uiSource 'MAX_REQUEST_ID_LENGTH' 'UI lacks requestId length cap'
    Assert-SourceMarker $uiSource 'fs.openSync' 'UI lacks fs.openSync'
    Assert-SourceMarker $uiSource 'fs.writeSync' 'UI lacks fs.writeSync'
    Assert-SourceMarker $uiSource 'fs.closeSync' 'UI lacks fs.closeSync'
    Assert-SourceMarker $uiSource 'finally' 'UI ledger write lacks finally close'
    $uiLedgerIndex = $uiSource.IndexOf('writeRequestLedger', [StringComparison]::Ordinal)
    $uiStartIndex = $uiSource.IndexOf('startVpnExtensionAbility', [StringComparison]::Ordinal)
    Assert-True ($uiLedgerIndex -ge 0 -and $uiStartIndex -ge 0 -and $uiLedgerIndex -lt $uiStartIndex) 'ledger write must precede startVpnExtensionAbility in source order'
    # M1 extension: always consume, want-first, age window, mismatch marker, missing-file info.
    Assert-SourceMarker $extensionSource "import fs from '@ohos.file.fs';" 'extension lacks @ohos.file.fs import'
    Assert-SourceMarker $extensionSource 'consumeRequestLedger' 'extension lacks ledger consumer'
    Assert-SourceMarker $extensionSource 'requestSource' 'extension lacks requestSource field'
    Assert-SourceMarker $extensionSource 'LEDGER_READ_RESOLVED' 'extension lacks LEDGER_READ_RESOLVED marker'
    Assert-SourceMarker $extensionSource 'LEDGER_READ_REJECTED' 'extension lacks LEDGER_READ_REJECTED marker'
    Assert-SourceMarker $extensionSource 'LEDGER_CONSUME_RESOLVED' 'extension lacks LEDGER_CONSUME_RESOLVED marker'
    Assert-SourceMarker $extensionSource 'LEDGER_CONSUME_REJECTED' 'extension lacks LEDGER_CONSUME_REJECTED marker'
    Assert-SourceMarker $extensionSource 'LEDGER_MISSING' 'extension lacks LEDGER_MISSING info marker'
    Assert-SourceMarker $extensionSource 'LEDGER_REQUESTID_MISMATCH' 'extension lacks LEDGER_REQUESTID_MISMATCH marker'
    Assert-SourceMarker $extensionSource 'LEDGER_AGE_REJECTED' 'extension lacks LEDGER_AGE_REJECTED marker'
    Assert-SourceMarker $extensionSource 'LEDGER_MAX_FUTURE_MS' 'extension lacks future skew bound'
    Assert-SourceMarker $extensionSource 'LEDGER_MAX_AGE_MS' 'extension lacks max age bound'
    Assert-SourceMarker $extensionSource 'VPN_REQUESTID_INVALID' 'extension lacks VPN_REQUESTID_INVALID marker'
    Assert-SourceMarker $extensionSource 'fs.openSync' 'extension lacks fs.openSync'
    Assert-SourceMarker $extensionSource 'fs.readSync' 'extension lacks fs.readSync'
    Assert-SourceMarker $extensionSource 'fs.closeSync' 'extension lacks fs.closeSync'
    Assert-SourceMarker $extensionSource 'fs.unlinkSync' 'extension lacks fs.unlinkSync'
    Assert-True (($extensionSource.Split('consumeRequestLedger').Count - 1) -eq 2) 'consumeRequestLedger must be defined and called exactly once'
    Assert-True ($extensionSource -notmatch 'setTimeout|clearTimeout') 'extension must not contain a timer'
    Assert-True ($extensionSource.Contains('Always read+consume', [StringComparison]::Ordinal)) 'onCreate must always consume ledger'
    $consumeCallIndex = $extensionSource.IndexOf('this.consumeRequestLedger()', [StringComparison]::Ordinal)
    $wantAssignIndex = $extensionSource.IndexOf("requestSource = 'want'", [StringComparison]::Ordinal)
    Assert-True ($consumeCallIndex -ge 0 -and $wantAssignIndex -ge 0 -and $consumeCallIndex -lt $wantAssignIndex) 'ledger consume must precede want-priority assignment'
    # M1 negative: persist/consume markers must not be used as create-success markers.
    Assert-True ($uiSource -notmatch 'CREATE_ACCEPTED') 'UI must never emit a create-accepted marker'
    Assert-True ($extensionSource -notmatch 'LEDGER.*CREATE_ACCEPTED|CREATE_ACCEPTED.*LEDGER') 'ledger markers must not gate create acceptance'
    # M3 UI: exactly one bounded setTimeout release, late then/catch markers, generation advanced on timeout.
    Assert-SourceMarker $uiSource 'setTimeout(' 'UI lacks pending release timer'
    Assert-SourceMarker $uiSource 'clearTimeout(' 'UI lacks timer clear'
    Assert-SourceMarker $uiSource 'START_PENDING_RELEASED' 'UI lacks START_PENDING_RELEASED marker'
    Assert-SourceMarker $uiSource 'reason=bounded-timeout' 'UI lacks bounded-timeout reason'
    Assert-SourceMarker $uiSource 'START_PROMISE_LATE_RESOLVED' 'UI lacks late resolved marker'
    Assert-SourceMarker $uiSource 'START_PROMISE_LATE_REJECTED' 'UI lacks late rejected marker'
    Assert-SourceMarker $uiSource 'startGeneration' 'UI lacks generation guard'
    Assert-SourceMarker $uiSource 'aboutToDisappear' 'UI lacks timer cleanup lifecycle'
    Assert-SourceMarker $uiSource '65000' 'UI lacks 65000ms bounded release threshold'
    Assert-True (($uiSource.Split('setTimeout(').Count - 1) -eq 1) 'UI must contain exactly one setTimeout bounded release'
    Assert-True (($uiSource.Split('clearTimeout(').Count - 1) -eq 1) 'UI must contain exactly one clearTimeout'
    Assert-True (($uiSource.Split('this.startGeneration++').Count - 1) -eq 2) 'timeout release must advance generation so late only goes LATE'
    Assert-True ($uiSource -notmatch 'Promise\.race|Atomics\.wait|while\s*\(|for\s*\(\s*;|setInterval') 'UI contains forbidden race or busy-wait'
    # C7: last-known request id survives the bounded pending release; Stop falls back to it with an
    # explicit basis; new Start overwrites it; the resolve copy never implies an active VPN.
    Assert-SourceMarker $uiSource 'lastRequestId' 'UI lacks last-known request id'
    Assert-SourceMarker $uiSource '|basis=%{public}s' 'UI lacks last-known stop basis marker'
    Assert-SourceMarker $uiSource "'last-known-request'" 'UI lacks last-known basis value'
    Assert-SourceMarker $uiSource 'STOP_SESSION_RELEASED_LAST_KNOWN' 'UI lacks last-known terminal rejection cleanup marker'
    Assert-SourceMarker $uiSource 'Start request resolved' 'UI lacks accurate start-request-resolved copy'
    Assert-SourceMarker $uiSource 'VPN create follows extension events' 'UI lacks accurate VPN-create-extension-events copy'
    Assert-True ($uiSource -notmatch [regex]::Escape("'Start resolved'")) 'UI still displays the misleading Start resolved copy'
    Assert-True (($uiSource.Split('lastRequestId = requestId').Count - 1) -eq 1) 'UI must overwrite last-known exactly once per Start'
    # M2 negative: no active tun-fd close/dup/read/write and no destroy-issued terminal marker anywhere.
    Assert-True ($uiSource -notmatch '\.close\(|\.dup\(|\.write\(|\.read\(') 'UI must not actively close/dup/read/write an fd'
    Assert-True ($extensionSource -notmatch '\.close\(|\.dup\(|\.write\(|\.read\(') 'extension must not actively close/dup/read/write an fd'
    Assert-True ($extensionSource -notmatch 'VPN_DESTROY_ISSUED') 'extension must not emit a destroy-issued terminal marker'
    Assert-True ($uiSource -notmatch 'VPN_DESTROY_ISSUED') 'UI must not emit a destroy-issued terminal marker'

    $baseFixture = New-SimulationFixture
    $baseFixturePath = Write-JsonFixture 'simulation-base.json' $baseFixture

    Write-Host 'SELFTEST_PHASE=blocked-plan-dry-run'
    $dryFreeze = New-Freeze 'blocked' 'EV-E3-SELFTEST-20990101-0001'
    $dryFreezePath = Write-JsonFixture 'freeze-dry.json' $dryFreeze
    $dryPaths = New-CasePaths 'dry'
    $dryResult = Invoke-Runner $dryFreezePath $dryPaths.Evidence $dryPaths.Raw -AsDryRun
    Assert-True ($dryResult.ExitCode -eq 0) "dry-run failed: $($dryResult.Text)"
    $dryRecord = Get-Content -LiteralPath (Join-Path $dryPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($dryRecord.plan_status -eq 'blocked' -and -not $dryRecord.is_evidence -and $dryRecord.record_status -eq 'blocked' -and $dryRecord.hdc_processes_started -eq 0) 'dry-run non-evidence contract mismatch'
    Assert-True (@($dryRecord.integrity_violations).Count -eq 0) 'normal dry-run produced false transcript integrity violations'
    Assert-True ($null -eq $dryRecord.scenario_aggregation.s3_clean_reactivation_proof) 'aggregation s3_clean_reactivation_proof masqueraded not-probed as false'
    Assert-ManifestAndSeal $dryPaths.Evidence
    Assert-ProjectionChain $dryPaths.Evidence

    Write-Host 'SELFTEST_PHASE=target-binding-confirm-mode-gates'
    # ADJ-20260810-0001 host-governed TargetBindingConfirm: mode exclusivity (specific messages),
    # required ConfirmationRecord, out-of-repository record path, and explicit EvidenceRoot/RawRoot
    # rejection are all enforced before any HDC call. The runner also enforces mode exclusivity
    # before the SelfTest early exit, so combined switches are rejected even with -SelfTest.
    $confirmConflictRun = Invoke-Runner $dryFreezePath '' '' -AsDryRun -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json'
    Assert-True ($confirmConflictRun.ExitCode -ne 0 -and $confirmConflictRun.Text -match 'mutually exclusive') 'TargetBindingConfirm+DryRun was not rejected as mutually exclusive'
    $confirmLivesimRun = Invoke-Runner $dryFreezePath '' '' -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json' -FixturePath $baseFixturePath
    Assert-True ($confirmLivesimRun.ExitCode -ne 0 -and $confirmLivesimRun.Text -match 'mutually exclusive') 'TargetBindingConfirm+LiveSimulation was not rejected as mutually exclusive'
    $confirmSelfTestRun = Invoke-Runner $dryFreezePath '' '' -AsSelfTest -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json'
    Assert-True ($confirmSelfTestRun.ExitCode -ne 0 -and $confirmSelfTestRun.Text -match 'mutually exclusive') 'TargetBindingConfirm+SelfTest was not rejected as mutually exclusive'
    $confirmNoRecordRun = Invoke-Runner $dryFreezePath '' '' -AsConfirm
    Assert-True ($confirmNoRecordRun.ExitCode -ne 0 -and $confirmNoRecordRun.Text -match 'requires ConfirmationRecord') 'TargetBindingConfirm without ConfirmationRecord was not rejected'
    $confirmInRepoPath = Join-Path $repo 'in-repo-confirmation.json'
    # ADJ-20260810-0001 (C6): the in-repo ConfirmationRecord negative must use a FIXED-candidate
    # blocked confirmation freeze so the failure is the out-of-repo path gate, never the earlier
    # candidate-pair gate (a wrong-pair freeze would be rejected before the path is even checked).
    $confirmBlockedFreeze = New-Freeze 'blocked' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $confirmBlockedPath = Write-JsonFixture 'freeze-confirm-blocked-candidate.json' $confirmBlockedFreeze
    $confirmInRepoRun = Invoke-Runner $confirmBlockedPath '' '' -AsConfirm -ConfirmationRecordPath $confirmInRepoPath
    Assert-True ($confirmInRepoRun.ExitCode -ne 0 -and $confirmInRepoRun.Text -match 'outside the git repository') 'in-repo ConfirmationRecord was not rejected'
    Assert-True (-not (Test-Path -LiteralPath $confirmInRepoPath)) 'in-repo ConfirmationRecord was written despite rejection'
    $confirmRootsRun = Invoke-Runner $dryFreezePath $dryPaths.Evidence $dryPaths.Raw -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json' -IncludeRoots
    Assert-True ($confirmRootsRun.ExitCode -ne 0 -and $confirmRootsRun.Text -match 'EvidenceRoot is not allowed') 'confirm mode did not explicitly reject EvidenceRoot/RawRoot'
    # ADJ-20260810-0001 (C6): TargetBindingConfirm freeze-level negatives - the fixed candidate
    # pair and attempt=initial are enforced before any HDC call, so a wrong-pair or retry-attempt
    # freeze is rejected with the specific gate message and the HDC sentinel never starts.
    $confirmWrongPairFreeze = New-Freeze 'blocked' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-WRONG'
    $confirmWrongPairPath = Write-JsonFixture 'freeze-confirm-wrong-pair.json' $confirmWrongPairFreeze
    $confirmWrongPairRun = Invoke-Runner $confirmWrongPairPath '' '' -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json'
    Assert-True ($confirmWrongPairRun.ExitCode -ne 0 -and $confirmWrongPairRun.Text -match 'fixed candidate pair') 'TargetBindingConfirm with wrong candidate pair was not rejected'
    $confirmRetryFreeze = New-Freeze 'blocked' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $confirmRetryFreeze.attempt = 'infrastructure-blocked-retry-1'
    $confirmRetryPath = Write-JsonFixture 'freeze-confirm-retry-attempt.json' $confirmRetryFreeze
    $confirmRetryRun = Invoke-Runner $confirmRetryPath '' '' -AsConfirm -ConfirmationRecordPath 'C:/outside/confirm.json'
    Assert-True ($confirmRetryRun.ExitCode -ne 0 -and $confirmRetryRun.Text -match 'attempt|retry') 'TargetBindingConfirm with retry attempt was not rejected'
    Assert-True (-not (Test-Path -LiteralPath $script:HdcLaunchMarker)) 'confirm mode gate cases launched the HDC sentinel'

    Write-Host 'SELFTEST_PHASE=target-binding-confirm-ready-freeze-binding'
    function Get-ContractSha256 {
        param($Freeze)
        # ADJ-20260810-0001 (C6): mirrors the runner's Get-FreezeContract field list (full contract,
        # including preflight_inputs_frozen_at). Used only to assert that the blocked and ready
        # phase full-contract hashes DIFFER when frozen_at advances; confirmation/review fixtures
        # bind Get-ConfirmationContractSha256 instead.
        $contract = [ordered]@{
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
            process_probe_target = $Freeze.process_probe_target
            operator_trust_model = $Freeze.operator_trust_model
            scenario_invalid_policy = $Freeze.scenario_invalid_policy
            layout_verification_profile = $Freeze.layout_verification_profile
            vpn_conflict_rejection_codes = $Freeze.vpn_conflict_rejection_codes
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
        $json = $contract | ConvertTo-Json -Depth 30 -Compress
        return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($json))).ToLowerInvariant()
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
    function Get-ConfirmationContractSha256 {
        param($Freeze)
        # ADJ-20260810-0001 (C6): mirrors the runner's Get-ConfirmationContract stable two-phase
        # projection (execution core + exact candidate pair + external inputs + code/runner/HDC +
        # roles; excludes plan_status / preflight_inputs_frozen_at / machine_fresh_confirmation /
        # independent_review_record / independent_review_ready). Confirmation/review records bind
        # THIS hash so they survive the blocked -> ready transition; if the runner's projection
        # ever drifts, the bound-pass cases fail and catch it. Field access goes through the same
        # Get-OptionalProperty helper the runner uses: PowerShell unrolls a single-element array
        # returned from a function (vpn_conflict_rejection_codes is a frozen one-element list), so
        # direct property access would serialize [2203002] while the runner serializes 2203002 and
        # the bound-pass cases would fail on a contract hash mismatch.
        $contract = [ordered]@{
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
        $json = $contract | ConvertTo-Json -Depth 30 -Compress
        return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($json))).ToLowerInvariant()
    }
    function Write-ConfirmationRecordFixture {
        param([string]$Name, $Freeze, [string]$OverrideAuthId = 'AUTH-E3-PHYS1API26-20260816-0003', [string]$OverrideVerdict = 'pass')
        $record = [ordered]@{
            schema_version = 1
            record_kind = 'target-binding-confirmation'
            is_evidence = $false
            authorization_id = $OverrideAuthId
            exception = 'E3-PHYS-PREFLIGHT'
            campaign_id = $Freeze.campaign_id
            evidence_id = $Freeze.evidence_id
            attempt = 'initial'
            retry = [ordered]@{ basis = 'N/A'; infrastructure_reason = 'N/A' }
            plan_status = 'ready'
            device_alias = 'PHYS-1'
            target_redacted = $true
            code_sha = $Freeze.code_sha
            runner_sha256 = $Freeze.runner_sha256
            freeze_manifest_sha256 = ('e' * 64)
            confirmation_contract_sha256 = Get-ConfirmationContractSha256 $Freeze
            hdc_sha256 = $Freeze.hdc.sha256
            hdc_version = $Freeze.hdc.version
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
            verdict = $OverrideVerdict
            reason = 'N/A'
        }
        $path = Write-JsonFixture $Name $record
        $sha = Get-Sha256 $path
        [IO.File]::WriteAllText($path + '.sha256', $sha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        return [pscustomobject]@{ Path = $path; Sha256 = $sha }
    }
    function Add-ConfirmationBinding {
        param($Freeze, [string]$RecordPath, [string]$RecordSha256, [string]$Status = 'pass', [string]$AuthId = 'AUTH-E3-PHYS1API26-20260816-0003')
        $copy = Copy-JsonObject $Freeze
        # ADJ-20260810-0001 (C6): machine_fresh_confirmation does not exist on a New-Freeze object;
        # under Set-StrictMode a direct assignment to the non-existent property would throw, so it
        # must be added with Add-Member -Force.
        Add-Member -InputObject $copy -NotePropertyName 'machine_fresh_confirmation' -NotePropertyValue ([ordered]@{ status = $Status; authorization_id = $AuthId; record_path = $RecordPath; record_sha256 = $RecordSha256 }) -Force
        return $copy
    }
    function Write-ReviewRecordFixture {
        param([string]$Name, $Freeze, [string]$MachineSha, [string]$ReviewerRole = 'selftest-independent-reviewer', [string]$StartedAt = '2098-12-31T23:59:59+00:00', [string]$EndedAt = '2098-12-31T23:59:59+00:00')
        # ADJ-20260810-0001 (C6): default review times are anchored AFTER the machine confirmation
        # ended_at (23:59:59) and at/before the final freeze preflight_inputs_frozen_at
        # (2099-01-01T00:00:00); the consumer enforces machine ended <= review started <= review
        # ended <= final freeze frozen_at, so a review can never be pre-filled before the machine
        # confirmation completes.
        $record = [ordered]@{
            schema_version = 1
            record_kind = 'e3-ready-freeze-review'
            is_evidence = $false
            exception = 'E3-PHYS-PREFLIGHT'
            campaign_id = $Freeze.campaign_id
            evidence_id = $Freeze.evidence_id
            code_sha = $Freeze.code_sha
            runner_sha256 = $Freeze.runner_sha256
            confirmation_contract_sha256 = Get-ConfirmationContractSha256 $Freeze
            machine_confirmation_sha256 = $MachineSha
            reviewer_role = $ReviewerRole
            operator_role = $Freeze.operator_role
            verdict = 'pass'
            blockers = 0
            majors = 0
            started_at = $StartedAt
            ended_at = $EndedAt
        }
        $path = Write-JsonFixture $Name $record
        $sha = Get-Sha256 $path
        [IO.File]::WriteAllText($path + '.sha256', $sha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        return [pscustomobject]@{ Path = $path; Sha256 = $sha }
    }
    function Add-ReviewBinding {
        param($Freeze, [string]$RecordPath, [string]$RecordSha256, [string]$Status = 'pass', [string]$ReviewerRole = 'selftest-independent-reviewer')
        $copy = Copy-JsonObject $Freeze
        $copy.independent_review_record = [ordered]@{ status = $Status; record_path = $RecordPath; record_sha256 = $RecordSha256; reviewer_role = $ReviewerRole }
        return $copy
    }
    # The ready freeze consumed by this AUTH path fixes the candidate pair and attempt=initial; the
    # generic infrastructure retry branch never applies.
    $readyNoConfirmFreeze = New-Freeze 'ready' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $readyNoConfirmPath = Write-JsonFixture 'freeze-ready-no-confirm.json' $readyNoConfirmFreeze
    $readyNoConfirmPaths = New-CasePaths 'ready-no-confirm-dryrun'
    $readyNoConfirmRun = Invoke-Runner $readyNoConfirmPath $readyNoConfirmPaths.Evidence $readyNoConfirmPaths.Raw -AsDryRun
    Assert-True ($readyNoConfirmRun.ExitCode -ne 0 -and $readyNoConfirmRun.Text -match 'machine_fresh_confirmation') 'DryRun ready freeze without confirmation was not rejected'
    Assert-True (-not (Test-Path -LiteralPath $readyNoConfirmPaths.Evidence)) 'rejected ready freeze still created an evidence root'
    $readyConfirmRecord = Write-ConfirmationRecordFixture 'confirmation-record-pass.json' $readyNoConfirmFreeze
    $readyReviewRecord = Write-ReviewRecordFixture 'ready-freeze-review-pass.json' $readyNoConfirmFreeze $readyConfirmRecord.Sha256
    $readyWrongAuthFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256 -AuthId 'AUTH-E3-PHYS1API26-20260815-9999') $readyReviewRecord.Path $readyReviewRecord.Sha256
    $readyWrongAuthPath = Write-JsonFixture 'freeze-ready-wrong-auth.json' $readyWrongAuthFreeze
    $readyWrongAuthPaths = New-CasePaths 'ready-wrong-auth-dryrun'
    $readyWrongAuthRun = Invoke-Runner $readyWrongAuthPath $readyWrongAuthPaths.Evidence $readyWrongAuthPaths.Raw -AsDryRun
    Assert-True ($readyWrongAuthRun.ExitCode -ne 0 -and $readyWrongAuthRun.Text -match 'authorization_id') 'DryRun ready freeze with wrong authorization_id was not rejected'
    $readyWrongShaFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path ('d' * 64)) $readyReviewRecord.Path $readyReviewRecord.Sha256
    $readyWrongShaPath = Write-JsonFixture 'freeze-ready-wrong-sha.json' $readyWrongShaFreeze
    $readyWrongShaPaths = New-CasePaths 'ready-wrong-sha-dryrun'
    $readyWrongShaRun = Invoke-Runner $readyWrongShaPath $readyWrongShaPaths.Evidence $readyWrongShaPaths.Raw -AsDryRun
    Assert-True ($readyWrongShaRun.ExitCode -ne 0 -and $readyWrongShaRun.Text -match 'SHA-256') 'DryRun ready freeze with wrong record sha was not rejected'
    $readyBoundFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $readyReviewRecord.Path $readyReviewRecord.Sha256
    $readyBoundPath = Write-JsonFixture 'freeze-ready-bound.json' $readyBoundFreeze
    $readyBoundPaths = New-CasePaths 'ready-bound-dryrun'
    $readyBoundRun = Invoke-Runner $readyBoundPath $readyBoundPaths.Evidence $readyBoundPaths.Raw -AsDryRun
    Assert-True ($readyBoundRun.ExitCode -eq 0) "DryRun ready freeze with bound confirmation failed: $($readyBoundRun.Text)"
    $readyBoundRecord = Get-Content -LiteralPath (Join-Path $readyBoundPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True (-not $readyBoundRecord.is_evidence -and $readyBoundRecord.record_status -eq 'blocked' -and $readyBoundRecord.hdc_processes_started -eq 0) 'ready-bound DryRun non-evidence contract mismatch'
    Assert-True ([string]$readyBoundRecord.machine_fresh_confirmation.status -eq 'pass' -and [string]$readyBoundRecord.machine_fresh_confirmation.authorization_id -eq 'AUTH-E3-PHYS1API26-20260816-0003' -and [string]$readyBoundRecord.machine_fresh_confirmation.record_sha256 -eq $readyConfirmRecord.Sha256 -and $null -eq $readyBoundRecord.machine_fresh_confirmation.PSObject.Properties['record_path']) 'sealed record machine_fresh_confirmation projection mismatch or leaked path'
    Assert-True ([string]$readyBoundRecord.independent_review_record.status -eq 'pass' -and [string]$readyBoundRecord.independent_review_record.record_sha256 -eq $readyReviewRecord.Sha256 -and $null -eq $readyBoundRecord.independent_review_record.PSObject.Properties['record_path']) 'sealed record independent_review_record projection mismatch or leaked path'
    Assert-ManifestAndSeal $readyBoundPaths.Evidence
    Assert-ProjectionChain $readyBoundPaths.Evidence
    $pendingBoundFreeze = Add-ConfirmationBinding $dryFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256 -Status 'pending'
    $pendingBoundPath = Write-JsonFixture 'freeze-blocked-pending-confirmation.json' $pendingBoundFreeze
    $pendingBoundPaths = New-CasePaths 'blocked-pending-dryrun'
    $pendingBoundRun = Invoke-Runner $pendingBoundPath $pendingBoundPaths.Evidence $pendingBoundPaths.Raw -AsDryRun
    Assert-True ($pendingBoundRun.ExitCode -eq 0) "blocked DryRun with pending confirmation was rejected: $($pendingBoundRun.Text)"
    # ADJ-20260810-0001 (C6): a blocked DryRun that declares status=pass is FULLY validated too;
    # pending is allowed and skipped, but a broken pass binding is never hidden by plan_status.
    $blockedPassFreeze = New-Freeze 'blocked' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $blockedPassBound = Add-ConfirmationBinding $blockedPassFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256
    $blockedPassPath = Write-JsonFixture 'freeze-blocked-machine-pass.json' $blockedPassBound
    $blockedPassPaths = New-CasePaths 'blocked-machine-pass-dryrun'
    $blockedPassRun = Invoke-Runner $blockedPassPath $blockedPassPaths.Evidence $blockedPassPaths.Raw -AsDryRun
    Assert-True ($blockedPassRun.ExitCode -eq 0) "blocked DryRun with status=pass was not fully validated: $($blockedPassRun.Text)"
    $brokenConfirmRecord = Write-ConfirmationRecordFixture 'confirmation-record-broken.json' $readyNoConfirmFreeze -OverrideVerdict 'blocked'
    $blockedBrokenBound = Add-ConfirmationBinding $blockedPassFreeze $brokenConfirmRecord.Path $brokenConfirmRecord.Sha256
    $blockedBrokenPath = Write-JsonFixture 'freeze-blocked-machine-broken.json' $blockedBrokenBound
    $blockedBrokenPaths = New-CasePaths 'blocked-machine-broken-dryrun'
    $blockedBrokenRun = Invoke-Runner $blockedBrokenPath $blockedBrokenPaths.Evidence $blockedBrokenPaths.Raw -AsDryRun
    Assert-True ($blockedBrokenRun.ExitCode -ne 0 -and $blockedBrokenRun.Text -match 'verdict') 'blocked DryRun with status=pass did not fully validate the record content'
    # ADJ-20260810-0001 (C6): blocked DryRun review consistency - a declared-pass review can never
    # ride on a pending/absent machine confirmation, and a machine pass + review pass pair is fully
    # validated (ValidateDeclaredPass) exactly like a ready one; pending review stays allowed.
    $blockedPendingMachineReviewPassFreeze = Add-ReviewBinding (Add-ConfirmationBinding $blockedPassFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256 -Status 'pending') $readyReviewRecord.Path $readyReviewRecord.Sha256
    $blockedPendingMachineReviewPassPath = Write-JsonFixture 'freeze-blocked-pending-machine-review-pass.json' $blockedPendingMachineReviewPassFreeze
    $blockedPendingMachineReviewPassPaths = New-CasePaths 'blocked-pending-machine-review-pass-dryrun'
    $blockedPendingMachineReviewPassRun = Invoke-Runner $blockedPendingMachineReviewPassPath $blockedPendingMachineReviewPassPaths.Evidence $blockedPendingMachineReviewPassPaths.Raw -AsDryRun
    Assert-True ($blockedPendingMachineReviewPassRun.ExitCode -ne 0 -and $blockedPendingMachineReviewPassRun.Text -match 'machine_fresh_confirmation.status=pass') 'blocked DryRun with pending machine confirmation and declared-pass review was not rejected'
    $blockedMachineReviewPassBound = Add-ReviewBinding (Add-ConfirmationBinding $blockedPassFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $readyReviewRecord.Path $readyReviewRecord.Sha256
    $blockedMachineReviewPassPath = Write-JsonFixture 'freeze-blocked-machine-review-pass.json' $blockedMachineReviewPassBound
    $blockedMachineReviewPassPaths = New-CasePaths 'blocked-machine-review-pass-dryrun'
    $blockedMachineReviewPassRun = Invoke-Runner $blockedMachineReviewPassPath $blockedMachineReviewPassPaths.Evidence $blockedMachineReviewPassPaths.Raw -AsDryRun
    Assert-True ($blockedMachineReviewPassRun.ExitCode -eq 0) "blocked DryRun with machine pass + review pass was not fully validated: $($blockedMachineReviewPassRun.Text)"
    $blockedPendingReviewBound = Add-ReviewBinding (Add-ConfirmationBinding $blockedPassFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $readyReviewRecord.Path $readyReviewRecord.Sha256 -Status 'pending'
    $blockedPendingReviewPath = Write-JsonFixture 'freeze-blocked-pending-review.json' $blockedPendingReviewBound
    $blockedPendingReviewPaths = New-CasePaths 'blocked-pending-review-dryrun'
    $blockedPendingReviewRun = Invoke-Runner $blockedPendingReviewPath $blockedPendingReviewPaths.Evidence $blockedPendingReviewPaths.Raw -AsDryRun
    Assert-True ($blockedPendingReviewRun.ExitCode -eq 0) "blocked DryRun with pending review was rejected: $($blockedPendingReviewRun.Text)"
    $brokenReviewRecordPath = Write-JsonFixture 'ready-freeze-review-broken-verdict.json' ([ordered]@{
        schema_version = 1
        record_kind = 'e3-ready-freeze-review'
        is_evidence = $false
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $readyNoConfirmFreeze.campaign_id
        evidence_id = $readyNoConfirmFreeze.evidence_id
        code_sha = $readyNoConfirmFreeze.code_sha
        runner_sha256 = $readyNoConfirmFreeze.runner_sha256
        confirmation_contract_sha256 = Get-ConfirmationContractSha256 $readyNoConfirmFreeze
        machine_confirmation_sha256 = $readyConfirmRecord.Sha256
        reviewer_role = 'selftest-independent-reviewer'
        operator_role = 'selftest-operator'
        verdict = 'blocked'
        blockers = 0
        majors = 0
        started_at = '2098-12-31T23:59:59+00:00'
        ended_at = '2098-12-31T23:59:59+00:00'
    })
    $brokenReviewRecordSha = Get-Sha256 $brokenReviewRecordPath
    [IO.File]::WriteAllText($brokenReviewRecordPath + '.sha256', $brokenReviewRecordSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $blockedBrokenReviewBound = Add-ReviewBinding (Add-ConfirmationBinding $blockedPassFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $brokenReviewRecordPath $brokenReviewRecordSha
    $blockedBrokenReviewPath = Write-JsonFixture 'freeze-blocked-machine-review-broken.json' $blockedBrokenReviewBound
    $blockedBrokenReviewPaths = New-CasePaths 'blocked-machine-review-broken-dryrun'
    $blockedBrokenReviewRun = Invoke-Runner $blockedBrokenReviewPath $blockedBrokenReviewPaths.Evidence $blockedBrokenReviewPaths.Raw -AsDryRun
    Assert-True ($blockedBrokenReviewRun.ExitCode -ne 0 -and $blockedBrokenReviewRun.Text -match 'verdict') 'blocked DryRun with machine pass + broken review record was not fully validated'
    # ADJ-20260810-0001 (C6): independent review record mechanical gate negatives. The self-declared
    # independent_review_ready=true boolean is never an execution gate for a ready plan_status.
    $readyNoReviewFreeze = Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256
    $readyNoReviewPath = Write-JsonFixture 'freeze-ready-no-review.json' $readyNoReviewFreeze
    $readyNoReviewPaths = New-CasePaths 'ready-no-review-dryrun'
    $readyNoReviewRun = Invoke-Runner $readyNoReviewPath $readyNoReviewPaths.Evidence $readyNoReviewPaths.Raw -AsDryRun
    Assert-True ($readyNoReviewRun.ExitCode -ne 0 -and $readyNoReviewRun.Text -match 'independent_review_record') 'DryRun ready freeze without pass review record was not rejected'
    $readyPendingReviewFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $readyReviewRecord.Path $readyReviewRecord.Sha256 -Status 'pending'
    $readyPendingReviewPath = Write-JsonFixture 'freeze-ready-pending-review.json' $readyPendingReviewFreeze
    $readyPendingReviewPaths = New-CasePaths 'ready-pending-review-dryrun'
    $readyPendingReviewRun = Invoke-Runner $readyPendingReviewPath $readyPendingReviewPaths.Evidence $readyPendingReviewPaths.Raw -AsDryRun
    Assert-True ($readyPendingReviewRun.ExitCode -ne 0 -and $readyPendingReviewRun.Text -match 'status must be pass') 'DryRun ready freeze with pending review record was not rejected'
    $wrongReviewRecord = Write-ReviewRecordFixture 'ready-freeze-review-wrong-machine.json' $readyNoConfirmFreeze ('0' * 64)
    $wrongReviewFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $wrongReviewRecord.Path $wrongReviewRecord.Sha256
    $wrongReviewPath = Write-JsonFixture 'freeze-ready-wrong-review-machine.json' $wrongReviewFreeze
    $wrongReviewPaths = New-CasePaths 'ready-wrong-review-machine-dryrun'
    $wrongReviewRun = Invoke-Runner $wrongReviewPath $wrongReviewPaths.Evidence $wrongReviewPaths.Raw -AsDryRun
    Assert-True ($wrongReviewRun.ExitCode -ne 0 -and $wrongReviewRun.Text -match 'machine_confirmation_sha256') 'review record with wrong machine_confirmation_sha256 was not rejected'
    $wrongRoleReviewRecord = Write-ReviewRecordFixture 'ready-freeze-review-wrong-role.json' $readyNoConfirmFreeze $readyConfirmRecord.Sha256 -ReviewerRole 'some-other-reviewer'
    $wrongRoleReviewFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $wrongRoleReviewRecord.Path $wrongRoleReviewRecord.Sha256
    $wrongRoleReviewPath = Write-JsonFixture 'freeze-ready-wrong-review-role.json' $wrongRoleReviewFreeze
    $wrongRoleReviewPaths = New-CasePaths 'ready-wrong-review-role-dryrun'
    $wrongRoleReviewRun = Invoke-Runner $wrongRoleReviewPath $wrongRoleReviewPaths.Evidence $wrongRoleReviewPaths.Raw -AsDryRun
    Assert-True ($wrongRoleReviewRun.ExitCode -ne 0 -and $wrongRoleReviewRun.Text -match 'reviewer_role') 'review record with wrong reviewer_role was not rejected'
    # ADJ-20260810-0001 (C6): the review cannot be pre-filled before the machine confirmation
    # completes: review started_at before the machine confirmation ended_at must be rejected.
    $earlyReviewRecord = Write-ReviewRecordFixture 'ready-freeze-review-early-start.json' $readyNoConfirmFreeze $readyConfirmRecord.Sha256 -StartedAt '2098-12-31T23:59:30+00:00' -EndedAt '2098-12-31T23:59:59+00:00'
    $earlyReviewFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $earlyReviewRecord.Path $earlyReviewRecord.Sha256
    $earlyReviewPath = Write-JsonFixture 'freeze-ready-early-review.json' $earlyReviewFreeze
    $earlyReviewPaths = New-CasePaths 'ready-early-review-dryrun'
    $earlyReviewRun = Invoke-Runner $earlyReviewPath $earlyReviewPaths.Evidence $earlyReviewPaths.Raw -AsDryRun
    # pwsh may wrap this fixed error phrase across a line boundary and insert ANSI SGR codes.
    # Strip only bounded SGR sequences, then normalize the three newline encodings; never scan
    # arbitrary output with an unbounded wildcard regex.
    $earlyReviewTextPlain = [regex]::Replace($earlyReviewRun.Text, "$([char]27)\[[0-9;]*m", '')
    $earlyReviewTextUnwrapped = $earlyReviewTextPlain.Replace("`r`n     | ", ' ').Replace("`n     | ", ' ').Replace("`r     | ", ' ')
    $earlyReviewTextSingleLine = $earlyReviewTextUnwrapped.Replace("`r`n", ' ').Replace("`n", ' ').Replace("`r", ' ')
    Assert-True ($earlyReviewRun.ExitCode -ne 0 -and $earlyReviewTextSingleLine -match 'machine confirmation ended_at') 'review record starting before the machine confirmation ended_at was not rejected'
    $loneReviewRecordPath = Write-JsonFixture 'ready-freeze-review-lone.json' ([ordered]@{
        schema_version = 1
        record_kind = 'e3-ready-freeze-review'
        is_evidence = $false
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = 'E3-PHYS-PREFLIGHT-20260816-0003'
        evidence_id = 'EV-E3-PHYS1API26-20260816-0003'
        code_sha = $readyNoConfirmFreeze.code_sha
        runner_sha256 = $readyNoConfirmFreeze.runner_sha256
        confirmation_contract_sha256 = Get-ConfirmationContractSha256 $readyNoConfirmFreeze
        machine_confirmation_sha256 = $readyConfirmRecord.Sha256
        reviewer_role = 'selftest-independent-reviewer'
        operator_role = 'selftest-operator'
        verdict = 'pass'
        blockers = 0
        majors = 0
        started_at = '2098-12-31T23:59:59+00:00'
        ended_at = '2098-12-31T23:59:59+00:00'
    })
    $loneReviewFreeze = Add-ReviewBinding (Add-ConfirmationBinding $readyNoConfirmFreeze $readyConfirmRecord.Path $readyConfirmRecord.Sha256) $loneReviewRecordPath (Get-Sha256 $loneReviewRecordPath)
    $loneReviewPath = Write-JsonFixture 'freeze-ready-lone-review.json' $loneReviewFreeze
    $loneReviewPaths = New-CasePaths 'ready-lone-review-dryrun'
    $loneReviewRun = Invoke-Runner $loneReviewPath $loneReviewPaths.Evidence $loneReviewPaths.Raw -AsDryRun
    Assert-True ($loneReviewRun.ExitCode -ne 0 -and $loneReviewRun.Text -match 'companion missing') 'review record without companion was not rejected'
    Assert-True (-not (Test-Path -LiteralPath $script:HdcLaunchMarker)) 'ready-freeze-binding cases launched the HDC sentinel'

    Write-Host 'SELFTEST_PHASE=two-phase-confirmation-contract'
    # ADJ-20260810-0001 (C6): two-phase positive - the blocked confirmation freeze freezes at T1
    # BEFORE the machine confirmation runs, and the final ready freeze freezes at T2 AFTER the
    # confirmation/review end times. The full Get-FreezeContract hash differs (preflight_inputs_
    # frozen_at advanced), but the stable confirmation contract is byte-identical, so the
    # confirmation/review records bound on the blocked phase are consumable by the ready phase;
    # the time gate (started<=ended<=frozen_at) is checked against the FINAL ready freeze.
    $twoPhaseBlockedFreeze = New-Freeze 'blocked' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $twoPhaseBlockedFreeze.preflight_inputs_frozen_at = '2099-01-01T00:00:00+00:00'
    $twoPhaseReadyFreeze = New-Freeze 'ready' 'EV-E3-PHYS1API26-20260816-0003' 'E3-PHYS-PREFLIGHT-20260816-0003'
    $twoPhaseReadyFreeze.preflight_inputs_frozen_at = '2099-01-01T00:00:10+00:00'
    Assert-True ((Get-ContractSha256 $twoPhaseBlockedFreeze) -ne (Get-ContractSha256 $twoPhaseReadyFreeze)) 'two-phase full freeze contract hashes must differ'
    Assert-True ((Get-ConfirmationContractSha256 $twoPhaseBlockedFreeze) -eq (Get-ConfirmationContractSha256 $twoPhaseReadyFreeze)) 'two-phase confirmation contract hashes must be identical'
    $twoPhaseRecord = [ordered]@{
        schema_version = 1
        record_kind = 'target-binding-confirmation'
        is_evidence = $false
        authorization_id = 'AUTH-E3-PHYS1API26-20260816-0003'
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $twoPhaseBlockedFreeze.campaign_id
        evidence_id = $twoPhaseBlockedFreeze.evidence_id
        attempt = 'initial'
        retry = [ordered]@{ basis = 'N/A'; infrastructure_reason = 'N/A' }
        plan_status = 'blocked'
        device_alias = 'PHYS-1'
        target_redacted = $true
        code_sha = $twoPhaseBlockedFreeze.code_sha
        runner_sha256 = $twoPhaseBlockedFreeze.runner_sha256
        freeze_manifest_sha256 = ('e' * 64)
        confirmation_contract_sha256 = Get-ConfirmationContractSha256 $twoPhaseBlockedFreeze
        hdc_sha256 = $twoPhaseBlockedFreeze.hdc.sha256
        hdc_version = $twoPhaseBlockedFreeze.hdc.version
        expected_model = 'PLA-AL10'
        expected_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
        observed_model = 'PLA-AL10'
        observed_build = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
        started_at = '2099-01-01T00:00:01+00:00'
        ended_at = '2099-01-01T00:00:05+00:00'
        command_attempted = 3
        command_completed = 3
        command_count = 3
        repository_fingerprint = ('g' * 64)
        verdict = 'pass'
        reason = 'N/A'
    }
    $twoPhaseRecordPath = Write-JsonFixture 'two-phase-confirmation.json' $twoPhaseRecord
    $twoPhaseRecordSha = Get-Sha256 $twoPhaseRecordPath
    [IO.File]::WriteAllText($twoPhaseRecordPath + '.sha256', $twoPhaseRecordSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $twoPhaseReview = [ordered]@{
        schema_version = 1
        record_kind = 'e3-ready-freeze-review'
        is_evidence = $false
        exception = 'E3-PHYS-PREFLIGHT'
        campaign_id = $twoPhaseBlockedFreeze.campaign_id
        evidence_id = $twoPhaseBlockedFreeze.evidence_id
        code_sha = $twoPhaseBlockedFreeze.code_sha
        runner_sha256 = $twoPhaseBlockedFreeze.runner_sha256
        confirmation_contract_sha256 = Get-ConfirmationContractSha256 $twoPhaseBlockedFreeze
        machine_confirmation_sha256 = $twoPhaseRecordSha
        reviewer_role = 'selftest-independent-reviewer'
        operator_role = 'selftest-operator'
        verdict = 'pass'
        blockers = 0
        majors = 0
        started_at = '2099-01-01T00:00:06+00:00'
        ended_at = '2099-01-01T00:00:09+00:00'
    }
    $twoPhaseReviewPath = Write-JsonFixture 'two-phase-review.json' $twoPhaseReview
    $twoPhaseReviewSha = Get-Sha256 $twoPhaseReviewPath
    [IO.File]::WriteAllText($twoPhaseReviewPath + '.sha256', $twoPhaseReviewSha + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    $twoPhaseBoundFreeze = Add-ReviewBinding (Add-ConfirmationBinding $twoPhaseReadyFreeze $twoPhaseRecordPath $twoPhaseRecordSha) $twoPhaseReviewPath $twoPhaseReviewSha
    $twoPhaseBoundPath = Write-JsonFixture 'freeze-two-phase-ready.json' $twoPhaseBoundFreeze
    $twoPhaseBoundPaths = New-CasePaths 'two-phase-ready-dryrun'
    $twoPhaseBoundRun = Invoke-Runner $twoPhaseBoundPath $twoPhaseBoundPaths.Evidence $twoPhaseBoundPaths.Raw -AsDryRun
    Assert-True ($twoPhaseBoundRun.ExitCode -eq 0) "two-phase ready DryRun failed: $($twoPhaseBoundRun.Text)"
    $twoPhaseBoundRecord = Get-Content -LiteralPath (Join-Path $twoPhaseBoundPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    # ADJ-20260810-0001 (C6): only assert the runner's sealed projections carry the final full
    # freeze contract and the stable confirmation contract as final SHA-256 values, that the two
    # are distinct from each other, and that the stable confirmation contract matches the
    # same-side mirror. Full-contract byte-equality against a re-derivation is representation-
    # sensitive (governance/time fields) and is never asserted across representations/timezones;
    # the two-phase full-hash-differ / stable-identical checks above are the same-side mirror proof.
    Assert-True ([string]$twoPhaseBoundRecord.freeze_contract_sha256 -match '^[0-9a-f]{64}$' -and [string]$twoPhaseBoundRecord.confirmation_contract_sha256 -match '^[0-9a-f]{64}$') 'sealed record dual contract projection missing final SHA-256 format'
    Assert-True ([string]$twoPhaseBoundRecord.freeze_contract_sha256 -ne [string]$twoPhaseBoundRecord.confirmation_contract_sha256) 'sealed record full freeze contract must differ from the stable confirmation contract'
    Assert-True ([string]$twoPhaseBoundRecord.confirmation_contract_sha256 -eq (Get-ConfirmationContractSha256 $twoPhaseReadyFreeze)) 'sealed record stable confirmation contract does not match the same-side mirror'
    Assert-True ([string]$twoPhaseBoundRecord.machine_fresh_confirmation.confirmation_contract_sha256 -eq (Get-ConfirmationContractSha256 $twoPhaseReadyFreeze) -and [string]$twoPhaseBoundRecord.independent_review_record.confirmation_contract_sha256 -eq (Get-ConfirmationContractSha256 $twoPhaseReadyFreeze)) 'sealed binding projections do not anchor the confirmation contract'
    # negative: mutating a stable contract core field that passes the freeze static value gate
    # (operator_role is non-empty, contains no '<', and still differs from the reviewer role) but
    # IS part of the stable confirmation contract changes the contract hash, so the records bound
    # on the blocked phase must be rejected by the ready-phase consumer. settings_revoke_mechanism
    # is NOT used here: it has a dedicated static freeze gate, so it would be rejected with a
    # settings_revoke_mechanism error before the contract check ever runs.
    $twoPhaseMutatedFreeze = Copy-JsonObject $twoPhaseBoundFreeze
    $twoPhaseMutatedFreeze.operator_role = 'some-other-operator'
    $twoPhaseMutatedPath = Write-JsonFixture 'freeze-two-phase-mutated.json' $twoPhaseMutatedFreeze
    $twoPhaseMutatedPaths = New-CasePaths 'two-phase-mutated-dryrun'
    $twoPhaseMutatedRun = Invoke-Runner $twoPhaseMutatedPath $twoPhaseMutatedPaths.Evidence $twoPhaseMutatedPaths.Raw -AsDryRun
    Assert-True ($twoPhaseMutatedRun.ExitCode -ne 0 -and $twoPhaseMutatedRun.Text -match 'confirmation_contract_sha256') 'two-phase stable contract core mutation was not rejected'
    Assert-True (-not (Test-Path -LiteralPath $script:HdcLaunchMarker)) 'two-phase cases launched the HDC sentinel'

    Write-Host 'SELFTEST_PHASE=complete-live-simulation-seven-scenarios'
    $liveFreeze = New-Freeze 'ready' 'EV-E3-SELFTEST-20990101-0002'
    $liveFreezePath = Write-JsonFixture 'freeze-live.json' $liveFreeze
    $livePaths = New-CasePaths 'live-simulation'
    $liveResult = Invoke-Runner $liveFreezePath $livePaths.Evidence $livePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($liveResult.ExitCode -eq 0) "live simulation failed: $($liveResult.Text)"
    Assert-True ($liveResult.Text -match 'HDC_PROCESSES=0') 'live simulation did not report zero HDC processes'
    $liveRecordPath = Join-Path $livePaths.Evidence 'scenario-results.json'
    $liveRecord = Get-Content -LiteralPath $liveRecordPath -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($liveRecord.execution_mode -eq 'live-simulation' -and -not $liveRecord.is_evidence -and $liveRecord.record_status -eq 'blocked' -and $liveRecord.verdict -eq 'blocked' -and $liveRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'live simulation top-level contract mismatch'
    Assert-True (@($liveRecord.integrity_violations).Count -eq 0) 'normal live simulation produced false transcript integrity violations (must not treat blocked mode as a free pass)'
    Assert-True (@($liveRecord.scenarios).Count -eq 7 -and (@($liveRecord.scenarios | Where-Object { $_.result -ne 'pass' }).Count -eq 0)) 'not all seven simulated scenarios passed'
    Assert-True ($liveRecord.scenarios[1].assertions.allow -eq 'pass' -and $liveRecord.scenarios[1].assertions.vpn_on_create -eq 'pass' -and $liveRecord.scenarios[1].assertions.vpn_connection_create_fd -eq 'pass') 'scenario 2 three assertions mismatch'
    Assert-True ($liveRecord.scenarios[1].authorization_capture.status -eq 'collected' -and $liveRecord.scenarios[1].authorization_capture.result -eq 'pass' -and $liveRecord.scenarios[1].authorization_capture.name -eq 'scenario-2-authorization') 'scenario 2 authorization capture missing or failed'
    Assert-True ([double]$liveRecord.scenarios[3].observation.measured_coverage_after_action_seconds -ge 60 -and $liveRecord.scenarios[3].observation.complete_window_observed) 'deny did not measure healthy coverage through action plus 60 seconds'
    Assert-True ($liveRecord.scenarios[3].reason -eq 'observable-B-request-rejection') 'cross-bundle pollution changed deny result'
    Assert-True ($liveRecord.scenarios[3].deny_screen -eq $true -and $liveRecord.scenarios[3].deny_screen_capture.status -eq 'collected' -and $liveRecord.scenarios[3].deny_screen_capture.visible -eq $true -and $liveRecord.scenarios[3].deny_screen_capture.result -eq 'pass') 'S4 deny pre-capture fields mismatch'
    Assert-True (($liveRecord.scenarios[3].observation.events.text -join "`n") -notmatch '2098-12-31|VPN_ONCREATE.*requestId=b4') 'pre-anchor history entered the deny scenario'
    Assert-True (($liveRecord.scenarios[4].observation.events.text -join "`n") -notmatch '2098-12-31') 'pre-anchor requestId history entered scenario 5'
    Assert-True ($liveRecord.scenarios[4].settings_reallow_path.match -eq $true -and $liveRecord.scenarios[4].settings_reallow_path.actual -eq 'direct-system-activation' -and $liveRecord.scenarios[4].settings_reallow_path.policy -eq 'observation-only') 'scenario 5 path observation contract mismatch on matched path'
    Assert-True ($liveRecord.scenarios[0].first_baseline_query_covered -and $liveRecord.scenarios[0].install_completed_within_60_seconds) 'scenario 1 did not cover baseline query and installation within 60 seconds'
    Assert-True ([double]$liveRecord.scenarios[0].observation.measured_coverage_before_action_prompt_seconds -lt 0.5) 'scenario 1 counted pre-action setup latency into the action window'
    Assert-True ([double]$liveRecord.scenarios[0].install_elapsed_seconds -le 60) 'scenario 1 install window exceeded 60s after the action prompt'
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$liveRecord.scenarios[1].observation.events[0].host_observed_at)) 'host observed time missing'
    Assert-True (-not [string]::IsNullOrWhiteSpace([string]$liveRecord.scenarios[1].observation.events[0].device_observed_at)) 'parsed device time missing'
    Assert-True ($liveRecord.scenarios[3].observation.capture_health.measured -eq $true) 'capture health was not actually measured'
    Assert-True ($null -eq $liveRecord.PSObject.Properties['failure'] -or [string]::IsNullOrEmpty([string]$liveRecord.failure)) 'successful pass record wrote empty failure'
    Assert-True ($null -eq $liveRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$liveRecord.infrastructure_reason)) 'successful pass record wrote empty infrastructure_reason'
    Assert-True ($liveRecord.actual -notmatch 'Runner stopped:\s*$') 'successful record wrote empty Runner stopped failure text'
    $screenRefs = @($liveRecord.screenshot_reference)
    $layoutRefs = @($liveRecord.layout_state_reference)
    Assert-True ($screenRefs.Count -gt 0 -and $layoutRefs.Count -gt 0) 'screenshot/layout references missing'
    Assert-True (@($screenRefs | Where-Object { $null -ne $_.PSObject.Properties['layout'] }).Count -eq 0) 'screenshot_reference leaked layout artifacts'
    Assert-True (@($layoutRefs | Where-Object { $null -ne $_.PSObject.Properties['screen'] }).Count -eq 0) 'layout_state_reference leaked screenshot artifacts'
    Assert-True (@($screenRefs | Where-Object { $_.status -eq 'collected' -and $null -ne $_.screen }).Count -gt 0) 'screenshot_reference missing screen artifacts'
    Assert-True (@($layoutRefs | Where-Object { $_.status -eq 'collected' -and $null -ne $_.layout }).Count -gt 0) 'layout_state_reference missing layout artifacts'
    Assert-True ([int]$liveRecord.clock_source.device_clock_skew_tolerance_seconds -eq 3) 'device clock skew tolerance not frozen at 3 seconds'
    Assert-True ([string]$liveRecord.target_tuple.distribution -eq [string]$liveFreeze.target_tuple.distribution) 'record distribution does not match freeze target_tuple.distribution'
    Assert-True ([string]$liveRecord.target_tuple.full_system_build -eq 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' -and [string]$liveRecord.target_tuple.api -eq '26' -and [string]$liveRecord.target_tuple.device_model -eq 'PLA-AL10' -and [string]$liveRecord.target_tuple.kernel_arch -eq 'aarch64' -and [string]$liveRecord.target_tuple.app_abi -eq 'arm64-v8a') 'new frozen target tuple did not pass through live simulation'

    $requiredFields = @(
        'information_status', 'stage_or_gate', 'related_stages_or_gates', 'target_tuple', 'signing', 'code_sha', 'source_archive_sha256',
        'source_manifest_sha256', 'sdk_sha256', 'runner_sha256', 'artifact_sha256', 'freeze_manifest_sha256', 'started_at', 'ended_at',
        'clock_source', 'cleanup_result', 'raw_hilog_reference', 'transcript_reference', 'screenshot_reference', 'layout_state_reference',
        'fault_reference', 'hash_manifest_reference', 'forbidden_capabilities_audit', 'operator', 'reviewer', 'reviewed_at', 'review_record',
        'prior_blocked_binding'
    )
    foreach ($field in $requiredFields) { Assert-True ($null -ne $liveRecord.PSObject.Properties[$field]) "schema field missing: $field" }
    Assert-True ($liveRecord.clock_source.host_observed_time_recorded.GetType() -eq [bool] -and $liveRecord.forbidden_capabilities_audit.no_go.GetType() -eq [bool]) 'JSON Boolean fields were stringified'
    Assert-True ([string]$liveRecord.freeze_manifest_sha256 -eq (Get-Sha256 $liveFreezePath)) 'freeze self hash missing or wrong'
    Assert-True ([string]$liveRecord.prior_blocked_binding -eq 'N/A') 'unbound record must project prior_blocked_binding N/A'
    Assert-True ($liveRecord.scenarios[5].a_accepted -eq $true -and $liveRecord.scenarios[5].reason -eq 'B-explicit-conflict-rejection' -and [int]$liveRecord.scenarios[5].b_rejection_code -eq 2203002) 'scenario 6 machine conflict result mismatch'
    Assert-True ([int]$liveRecord.scenarios[5].accepted_session_count_in_window -eq 2) 'scenario 6 accepted marker count did not use verified A/B request semantics'
    Assert-True ((@($liveRecord.scenarios[5].observation.operator_steps | ForEach-Object { [int]$_.step_index }) -join ',') -eq '1,3') 'scenario 6 direct-entry path unexpectedly added Allow or changed step numbering'
    Assert-True ($null -eq $liveRecord.scenarios[5].PSObject.Properties['no_dual_active_confirmed'] -and $null -eq $liveRecord.scenarios[5].PSObject.Properties['dual_active_confirmed'] -and $null -eq $liveRecord.scenarios[5].PSObject.Properties['operator_state']) 'scenario 6 retained semantic operator fields'
    Assert-True ($liveRecord.scenarios[6].post_cleanup_capture -eq $true -and $liveRecord.scenarios[6].post_cleanup_capture_name -eq 'scenario-7-post-cleanup') 'scenario 7 post-cleanup screenshot naming mismatch'
    Assert-True ($liveRecord.reviewer -eq 'pending' -and $liveRecord.reviewed_at -eq 'pending' -and $liveRecord.record_status -notmatch '^reviewed') 'runner wrote reviewed state'
    Assert-True ([string]$liveRecord.settings_revoke_mechanism -eq 'settings-app-info-force-stop' -and [string]$liveRecord.settings_vpn_page_policy -eq 'observation-only' -and [string]$liveRecord.destroy_terminal_policy -eq 'callback-or-strict-process-boundary') 'record did not project ADJ-20260807-0003 decision fields'
    Assert-True ([int]$liveRecord.process_absent_required_count -eq 2 -and [double]$liveRecord.process_absent_probe_spacing_seconds -eq 3) 'record did not project probe count/spacing'
    Assert-True ($liveRecord.scenarios[2].terminal_mode -eq 'callback-post-fd' -and $liveRecord.scenarios[6].terminal_mode -eq 'callback-post-fd') 'base S3/S7 did not prefer callback terminal mode'
    Assert-True ($liveRecord.scenarios[4].terminal_mode -eq 'settings-app-info-force-stop' -and -not $liveRecord.scenarios[4].app_info_force_stop_capture.machine_verified -and $liveRecord.scenarios[4].app_info_force_stop_capture.observation_only -and $liveRecord.scenarios[4].settings_vpn_page_observation_only -and $liveRecord.scenarios[4].bundle_present_during_probe) 'base S5 force-stop flow fields mismatch'
    Assert-True ([string]$liveRecord.scenarios[4].observation.operator_steps[-1].expected_action -eq '点击强行停止，并完成随后出现的确认（如有）') 'base S5 did not use the exact production force-stop operator action'
    Assert-True (@($liveRecord.scenarios[4].process_final_state_probes).Count -ge 2) 'base S5 did not record consecutive absent probes'
    Assert-True ($liveRecord.scenarios[2].clean_reactivation_proof -eq $true) 'base S3 clean reactivation proof not recorded from S5 fresh create'
    Assert-True ($liveRecord.scenario_aggregation.s3_clean_reactivation_proof -eq $true) 'aggregation s3_clean_reactivation_proof not true when S5 fresh create proves it'
    Assert-True ($liveRecord.scenarios[2].process_target -eq 'cn.alfadb.netbird.e3physvpna:vpn' -and $liveRecord.scenarios[4].process_target -eq 'cn.alfadb.netbird.e3physvpna:vpn' -and $liveRecord.scenarios[6].process_target -eq 'cn.alfadb.netbird.e3physvpna:vpn') 'S3/S5/S7 process_target not the <bundle>:vpn Extension process'
    Assert-True (@($liveRecord.scenarios[4].process_final_state_probes | Where-Object { $_.process_target -ne 'cn.alfadb.netbird.e3physvpna:vpn' }).Count -eq 0) 'S5 probe records missing process_target'
    Assert-True ([double]$liveRecord.scenarios[4].process_absent_evidence.measured_spacing_seconds -ge 3.0) 'S5 default probe spacing below frozen 3.0s'
    Assert-True ([double]$liveRecord.scenarios[4].process_final_state_probes[1].spacing_seconds_since_previous -ge 3.0) 'S5 second probe spacing below frozen 3.0s'
    Assert-True ($null -ne $liveRecord.scenario_aggregation.scenario_2_assertions -and [string]$liveRecord.scenario_aggregation.scenario_2_assertions.allow -eq 'pass') 'scenario_2_assertions missing or not restored'
    Assert-True ([string]$liveRecord.scenarios[2].request_id -eq 'a2' -and [string]$liveRecord.scenarios[6].request_id -eq 'a6') 'S3/S7 request ids not bound to active create requests'
    $liveTranscriptLines = @(Get-Content -LiteralPath (Join-Path $livePaths.Evidence 'projection\transcript.redacted.jsonl'))
    $liveTranscriptEntries = @($liveTranscriptLines | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    $liveHdcLogicalTranscript = @($liveTranscriptEntries | Where-Object { [string]$_.payload.kind -in @('hdc-command', 'hdc-capture-start') })
    Assert-True ([int]$liveRecord.hdc_logical_calls -eq $liveHdcLogicalTranscript.Count) 'hdc_logical_calls does not match transcript hdc-command/hdc-capture-start count'
    $liveProbePairs = @($liveRecord.scenarios[4].process_final_state_probes).Count
    Assert-True ($liveProbePairs -ge 2) 'base S5 probe pair count below required consecutive absent'
    $liveProbeHdcCommands = @($liveTranscriptEntries | Where-Object {
        [string]$_.payload.kind -eq 'hdc-command' -and [string]$_.payload.data.operation -in @('PidOf', 'BundleDump')
    })
    # Each probe pair is PidOf+BundleDump (+2). Baseline/install/cleanup also use the same ops; probe pairs alone must contribute even counts.
    Assert-True (($liveProbeHdcCommands.Count % 2) -eq 0 -and $liveProbeHdcCommands.Count -ge (2 * $liveProbePairs)) 'PidOf+BundleDump transcript count not aligned with probe pairs (+2 each)'
    Assert-ManifestAndSeal $livePaths.Evidence
    Assert-ProjectionChain $livePaths.Evidence

    # Pollable operator-wait state: stable path in EvidenceRoot, complete at the end, no sensitive
    # fields, sealed by the final manifest, and bound by the record reference.
    $waitStatePath = Join-Path $livePaths.Evidence 'operator-wait-state.json'
    Assert-True (Test-Path -LiteralPath $waitStatePath -PathType Leaf) 'operator-wait-state.json missing from evidence root'
    $waitState = Get-Content -LiteralPath $waitStatePath -Raw | ConvertFrom-Json -Depth 20
    Assert-True ($waitState.phase -eq 'complete' -and $waitState.complete -eq $true -and $null -ne $waitState.completed_at -and [int]$waitState.schema_version -eq 2) 'operator-wait-state not completed'
    Assert-True ($waitState.execution_mode -eq 'live-simulation' -and $null -ne $waitState.campaign_id -and $null -ne $waitState.evidence_id) 'operator-wait-state identity fields missing'
    $waitStateText = Get-Content -LiteralPath $waitStatePath -Raw
    Assert-True ($waitStateText -notmatch 'target-canary|10\.23\.45\.67|2001:db8|device-canary|00:11:22:33:44:55|SN-CANARY|HDC-MUST-NOT-START|final-a\.hap|final-b\.hap|usb-target') 'operator-wait-state leaked sensitive fields'
    Assert-True ($null -eq $waitState.PSObject.Properties['target'] -and $null -eq $waitState.PSObject.Properties['udid'] -and $null -eq $waitState.PSObject.Properties['hap_a'] -and $null -eq $waitState.PSObject.Properties['hap_b'] -and $null -eq $waitState.PSObject.Properties['endpoint']) 'operator-wait-state has forbidden fields'
    $waitPhases = @($waitState.history | ForEach-Object { [string]$_.phase } | Select-Object -Unique)
    Assert-True ('waiting' -in $waitPhases -and 'operator-complete' -in $waitPhases -and 'verifying' -in $waitPhases -and 'captured' -in $waitPhases -and 'complete' -in $waitPhases) 'operator-wait-state history misses a state-machine phase'
    Assert-True ($waitStateText -notmatch '(?i)READY|ACK\s+[0-9a-f]|NO-DUAL|DUAL-ACTIVE|FINAL-CLEANUP|semantic.confirm|nonce') 'operator-wait-state retained semantic token protocol'
    $liveWaitManifest = Get-Content -LiteralPath (Join-Path $livePaths.Evidence 'hash-manifest.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True (@($liveWaitManifest.files | Where-Object { $_.path -eq 'operator-wait-state.json' }).Count -eq 1) 'operator-wait-state.json not sealed in final manifest'
    Assert-True ($liveRecord.operator_wait_state_reference.path -eq 'operator-wait-state.json' -and [string]$liveRecord.operator_wait_state_reference.sha256 -eq (Get-Sha256 $waitStatePath) -and $liveRecord.operator_wait_state_reference.sealed_by -eq 'hash-manifest.json') 'record operator_wait_state_reference mismatch'

    $rawText = (Get-ChildItem -LiteralPath $livePaths.Raw -File -Recurse | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw -ErrorAction SilentlyContinue }) -join "`n"
    Assert-True ($rawText -match 'target-canary\.example\.test:8710' -and $rawText -match '10\.23\.45\.67:8710' -and $rawText -match '2001:db8::1234' -and $rawText -match '00:11:22:33:44:55' -and $rawText -match 'SN-CANARY12345678') 'raw HiLog did not preserve sensitive canaries'
    $evidenceText = (Get-ChildItem -LiteralPath $livePaths.Evidence -File -Recurse | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
    Assert-True ($evidenceText -notmatch 'target-canary|10\.23\.45\.67|2001:db8|device-canary|00:11:22:33:44:55|SN-CANARY|HDC-MUST-NOT-START') 'sensitive target or host canary leaked into projected evidence'
    Assert-True ($evidenceText -match [regex]::Escape('PLA-AL10 7.0.0.100(SP8C00E32R7P2)') -and $evidenceText -match '"api"\s*:\s*"26"' -and $evidenceText -notmatch '192\.0\.2\.') 'public API26 build IP-like literal was redacted or real IPs leaked into evidence'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $livePaths.Evidence 'raw\transcript.jsonl'))) 'raw transcript was incorrectly created in evidence root'
    Assert-True ($liveRecord.transcript_reference.projection_only -and -not $liveRecord.transcript_reference.raw_transcript_exists) 'projection/raw transcript contract mismatch'
    Assert-True (@($liveRecord.raw_hilog_reference).Count -eq 2 -and @($liveRecord.raw_hilog_reference | Where-Object { $_.reference -eq 'RAW-HILOG-CAMPAIGN' }).Count -eq 1) 'campaign did not produce one continuous HiLog stream'
    Assert-True ($liveRecord.cleanup_result.verified_absent -and $liveRecord.cleanup_result.status -eq 'verified-clean') 'final targeted cleanup verification did not pass'
    Assert-True (@($liveRecord.fault_reference.artifacts).Count -eq 2) 'targeted fault artifact references are incomplete'
    foreach ($faultArtifact in @($liveRecord.fault_reference.artifacts)) {
        $suffix = if ($faultArtifact.operation -eq 'FaultA') { 'a' } else { 'b' }
        $faultPath = Join-Path $livePaths.Raw "fault-scenario-7-$suffix.txt"
        Assert-True (Test-Path -LiteralPath $faultPath -PathType Leaf) "fault artifact missing: $suffix"
        Assert-True ((Get-Sha256 $faultPath) -eq [string]$faultArtifact.sha256) "fault artifact hash mismatch: $suffix"
    }

    Write-Host 'SELFTEST_PHASE=s6-b-production-authorization-allow-frozen-pass'
    $s6BProductionFixture = New-S6BAuthorizationFixture
    $s6BProductionPath = Write-JsonFixture 'simulation-s6-b-production-authorization.json' $s6BProductionFixture
    $s6BProductionPaths = New-CasePaths 's6-b-production-authorization'
    $s6BProductionRun = Invoke-Runner $liveFreezePath $s6BProductionPaths.Evidence $s6BProductionPaths.Raw -FixturePath $s6BProductionPath
    Assert-True ($s6BProductionRun.ExitCode -eq 0) "S6 B production authorization path failed: $($s6BProductionRun.Text)"
    Assert-ManifestAndSeal $s6BProductionPaths.Evidence
    Assert-ProjectionChain $s6BProductionPaths.Evidence
    $s6BProductionRecord = Get-Content -LiteralPath (Join-Path $s6BProductionPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    $s6BProductionS6 = $s6BProductionRecord.scenarios[5]
    Assert-True ($s6BProductionS6.result -eq 'pass' -and $s6BProductionS6.reason -eq 'B-explicit-conflict-rejection' -and [int]$s6BProductionS6.b_rejection_code -eq 2203002) 'S6 production authorization frozen conflict did not pass'
    Assert-True ([int]$s6BProductionS6.accepted_session_count_in_window -eq 2) 'S6 production authorization accepted marker count mismatch'
    $s6BLayoutRefs = @($s6BProductionRecord.layout_state_reference | ForEach-Object { [string]$_.name })
    Assert-True ('scenario-6-conflict' -in $s6BLayoutRefs -and 'scenario-6-after-allow-b' -in $s6BLayoutRefs) 'S6 production authorization record omitted the decisive/new capture references'
    Assert-True ((@($s6BProductionS6.observation.operator_steps | ForEach-Object { [int]$_.step_index }) -join ',') -eq '1,3,4') 'S6 authorization path step numbering is not A Start=1, B Start=3, B Allow=4'
    $s6BProductionWait = Get-Content -LiteralPath (Join-Path $s6BProductionPaths.Evidence 'operator-wait-state.json') -Raw | ConvertFrom-Json -Depth 60
    $s6BAllowHistory = @($s6BProductionWait.history | Where-Object { [int]$_.scenario -eq 6 -and [int]$_.step_index -eq 4 })
    Assert-True (@($s6BAllowHistory | Where-Object { [string](Get-OptionalProperty $_.capture_before 'selected_profile' '') -eq 'authorization' }).Count -gt 0) 'S6 B Allow capture_before did not bind the authorization choice checkpoint'
    Assert-True (@($s6BAllowHistory | Where-Object { [string](Get-OptionalProperty $_.capture_after 'profile' '') -eq 'authorization-dismissed' }).Count -gt 0) 'S6 B Allow capture_after did not bind authorization-dismissed'
    Assert-True (@($s6BAllowHistory | Where-Object { [string](Get-OptionalProperty $_.machine_precondition 'reason' '') -eq 'B-authorization-layout-and-request-machine-verified' }).Count -gt 0) 'S6 B Allow did not use the literal layout/request machine precondition'
    Assert-True (@($s6BAllowHistory | Where-Object { $null -ne $_.machine_precondition.PSObject.Properties['states'] }).Count -eq 0) 'S6 B Allow reused a stale process checkpoint as its precondition'
    $s6ProductionText = Get-Content -LiteralPath (Join-Path $project 'tests\fixtures\s6-b-authorization-production-0005.json') -Raw
    Assert-True ($s6ProductionText -match 'E3 Preflight B' -and $s6ProductionText -notmatch 'PLA-AL10|PHYS-1|192\.168\.|中国电信|中国联通|session31') 'S6 production-derived fixture lost B label or contains forbidden source fields'

    Write-Host 'SELFTEST_PHASE=s6-b-authorization-resample'
    $s6BResampleFixture = New-S6BAuthorizationFixture
    $s6BResampleFixture.layout_profiles['scenario-6-conflict'] = 'authorization'
    $s6BResampleFixture | Add-Member -NotePropertyName layout_ready_delays -NotePropertyValue ([ordered]@{ 'scenario-6-conflict' = 3 }) -Force
    $s6BResamplePath = Write-JsonFixture 'simulation-s6-b-authorization-resample.json' $s6BResampleFixture
    $s6BResamplePaths = New-CasePaths 's6-b-authorization-resample'
    $s6BResampleRun = Invoke-Runner $liveFreezePath $s6BResamplePaths.Evidence $s6BResamplePaths.Raw -FixturePath $s6BResamplePath
    Assert-True ($s6BResampleRun.ExitCode -eq 0) "S6 B authorization resample failed: $($s6BResampleRun.Text)"
    $s6BResampleTranscript = @(Get-Content -LiteralPath (Join-Path $s6BResamplePaths.Evidence 'projection\transcript.redacted.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 60 })
    Assert-True (@($s6BResampleTranscript | Where-Object { [string]$_.payload.kind -eq 'machine-layout-choice-resample' -and [string]$_.payload.data.name -eq 'scenario-6-conflict' }).Count -gt 0) 'S6 B authorization choice did not resample'

    Write-Host 'SELFTEST_PHASE=s6-b-allow-no-terminal-blocked'
    $s6BNoTerminalFixture = New-S6BAuthorizationFixture 'none'
    $s6BNoTerminalPath = Write-JsonFixture 'simulation-s6-b-allow-no-terminal.json' $s6BNoTerminalFixture
    $s6BNoTerminalPaths = New-CasePaths 's6-b-allow-no-terminal'
    $s6BNoTerminalRun = Invoke-Runner $liveFreezePath $s6BNoTerminalPaths.Evidence $s6BNoTerminalPaths.Raw -FixturePath $s6BNoTerminalPath
    $s6BNoTerminalRecord = Get-Content -LiteralPath (Join-Path $s6BNoTerminalPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6BNoTerminalRun.ExitCode -ne 0 -and $s6BNoTerminalRecord.overall -eq 'blocked' -and $null -eq $s6BNoTerminalRecord.PSObject.Properties['scenario_invalid'] -and $s6BNoTerminalRun.Text -match 'B-create-terminal-missing|platform-marker-missing') 'S6 B Allow without terminal was not blocked'

    Write-Host 'SELFTEST_PHASE=s6-b-authorization-not-dismissed-blocked'
    $s6BNotDismissedFixture = New-S6BAuthorizationFixture
    $s6BNotDismissedFixture.layout_profiles['scenario-6-after-allow-b'] = 'authorization'
    $s6BNotDismissedPath = Write-JsonFixture 'simulation-s6-b-authorization-not-dismissed.json' $s6BNotDismissedFixture
    $s6BNotDismissedPaths = New-CasePaths 's6-b-authorization-not-dismissed'
    $s6BNotDismissedRun = Invoke-Runner $liveFreezePath $s6BNotDismissedPaths.Evidence $s6BNotDismissedPaths.Raw -FixturePath $s6BNotDismissedPath
    $s6BNotDismissedRecord = Get-Content -LiteralPath (Join-Path $s6BNotDismissedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6BNotDismissedRun.ExitCode -ne 0 -and $s6BNotDismissedRecord.overall -eq 'blocked' -and $null -eq $s6BNotDismissedRecord.PSObject.Properties['scenario_invalid'] -and $s6BNotDismissedRun.Text -match 'authorization-not-dismissed:layout-fields-missing:authorization-controls-absent') 'S6 B authorization cancel/not-dismissed was not blocked with the mismatch reason'

    Write-Host 'SELFTEST_PHASE=s6-b-authorization-dismissal-unverifiable-blocked'
    $s6BUnverifiableFixture = New-S6BAuthorizationFixture
    $s6BUnverifiableFixture | Add-Member -NotePropertyName invalid_layout_json -NotePropertyValue @('scenario-6-after-allow-b') -Force
    $s6BUnverifiablePath = Write-JsonFixture 'simulation-s6-b-authorization-dismissal-unverifiable.json' $s6BUnverifiableFixture
    $s6BUnverifiablePaths = New-CasePaths 's6-b-authorization-dismissal-unverifiable'
    $s6BUnverifiableRun = Invoke-Runner $liveFreezePath $s6BUnverifiablePaths.Evidence $s6BUnverifiablePaths.Raw -FixturePath $s6BUnverifiablePath
    $s6BUnverifiableRecord = Get-Content -LiteralPath (Join-Path $s6BUnverifiablePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6BUnverifiableRun.ExitCode -ne 0 -and $s6BUnverifiableRecord.overall -eq 'blocked' -and $null -eq $s6BUnverifiableRecord.PSObject.Properties['scenario_invalid'] -and $s6BUnverifiableRun.Text -match 'authorization-dismissal-unverifiable:layout-json-invalid') "S6 B invalid dismissal JSON did not preserve the unverifiable reason: $($s6BUnverifiableRun.Text)"

    Write-Host 'SELFTEST_PHASE=s6-b-post-terminal-a-unverifiable-blocked'
    $s6BAUnverifiableFixture = New-S6BAuthorizationFixture
    $s6BAUnverifiableFixture.hdc_failures = @([ordered]@{ operation = 'PidOf'; occurrence = 16; exit_code = 1; stdout = ''; stderr = '' })
    $s6BAUnverifiablePath = Write-JsonFixture 'simulation-s6-b-post-terminal-a-unverifiable.json' $s6BAUnverifiableFixture
    $s6BAUnverifiablePaths = New-CasePaths 's6-b-post-terminal-a-unverifiable'
    $s6BAUnverifiableRun = Invoke-Runner $liveFreezePath $s6BAUnverifiablePaths.Evidence $s6BAUnverifiablePaths.Raw -FixturePath $s6BAUnverifiablePath
    $s6BAUnverifiableRecord = Get-Content -LiteralPath (Join-Path $s6BAUnverifiablePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    $s6BAUnverifiableS6 = $s6BAUnverifiableRecord.scenarios[5]
    $s6BAUnverifiableS7 = $s6BAUnverifiableRecord.scenarios[6]
    Assert-True ($s6BAUnverifiableRun.ExitCode -eq 0 -and $s6BAUnverifiableS6.result -eq 'blocked' -and $s6BAUnverifiableS6.reason -eq 'A-active-state-unverifiable-after-B-terminal') 'S6 rejected post-terminal A unverifiable branch did not record the exact blocked reason'
    Assert-True ($s6BAUnverifiableS7.result -eq 'blocked' -and $s6BAUnverifiableS7.reason -eq 'not-run-after-platform-blocked') 'S7 did not stay blocked/not-run-after-platform-blocked after the terminal process block'
    Assert-True ([int]$s6BAUnverifiableS6.accepted_session_count_in_window -eq 2) 'S6 rejected-terminal blocked branch used a different accepted marker count'
    Assert-True ($null -ne $s6BAUnverifiableS6.PSObject.Properties['machine_process_checkpoint'] -and $s6BAUnverifiableS6.machine_process_checkpoint.status -eq 'blocked' -and [string]$s6BAUnverifiableS6.machine_process_checkpoint.reason -match 'process-state-mismatch|process-check-unavailable') 'S6 terminal process checkpoint was not recorded on the blocked branch'

    Write-Host 'SELFTEST_PHASE=s6-b-authorization-accepted-fail'
    $s6BAcceptedFixture = New-S6BAuthorizationFixture 'accepted'
    $s6BAcceptedFixture.hdc_failures = @([ordered]@{ operation = 'PidOf'; occurrence = 16; exit_code = 0; stdout = ''; stderr = '' })
    $s6BAcceptedPath = Write-JsonFixture 'simulation-s6-b-authorization-accepted-a-absent.json' $s6BAcceptedFixture
    $s6BAcceptedPaths = New-CasePaths 's6-b-authorization-accepted'
    $s6BAcceptedRun = Invoke-Runner $liveFreezePath $s6BAcceptedPaths.Evidence $s6BAcceptedPaths.Raw -FixturePath $s6BAcceptedPath
    $s6BAcceptedRecord = Get-Content -LiteralPath (Join-Path $s6BAcceptedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6BAcceptedRun.ExitCode -eq 0 -and $s6BAcceptedRecord.overall -eq 'fail' -and $s6BAcceptedRecord.scenarios[5].result -eq 'fail' -and $s6BAcceptedRecord.scenarios[5].reason -eq 'two-accepted-sessions-observed' -and $s6BAcceptedRecord.scenarios[5].b_accepted) 'S6 B accepted/dual accepted was not fail'
    Assert-True ([int]$s6BAcceptedRecord.scenarios[5].accepted_session_count_in_window -eq 3) 'S6 accepted-terminal branch did not count only verified A/B accepted markers'
    Assert-True ($s6BAcceptedRecord.scenarios[5].machine_process_checkpoint.status -eq 'blocked' -and [string]$s6BAcceptedRecord.scenarios[5].machine_process_checkpoint.reason -match 'process-state-mismatch') 'S6 B accepted with A absent did not preserve functional fail priority over process mismatch'

    Write-Host 'SELFTEST_PHASE=s6-b-authorization-stray-action-invalid'
    $s6BStrayFixture = New-S6BAuthorizationFixture
    $s6BStrayFixture.scenario_events.'6' += [ordered]@{ offset_seconds = 8.5; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" }
    $s6BStrayPath = Write-JsonFixture 'simulation-s6-b-authorization-stray-action.json' $s6BStrayFixture
    $s6BStrayPaths = New-CasePaths 's6-b-authorization-stray-action'
    $s6BStrayRun = Invoke-Runner $liveFreezePath $s6BStrayPaths.Evidence $s6BStrayPaths.Raw -FixturePath $s6BStrayPath
    $s6BStrayRecord = Get-Content -LiteralPath (Join-Path $s6BStrayPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6BStrayRun.ExitCode -eq 2 -and $s6BStrayRecord.overall -eq 'invalid' -and [string]$s6BStrayRecord.scenario_invalid.reason -match 'unexpected-UI_STOP|stray-operator-action') 'S6 B authorization extra UI action was not invalid'

    Write-Host 'SELFTEST_PHASE=s6-b-frozen-late-accepted-a-absent-fail'
    $s6BLateAcceptedFixture = New-S6BAuthorizationFixture 'frozen'
    $s6BLateAcceptedFixture.scenario_events.'6' += [ordered]@{ offset_seconds = 10; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=b6|accepted=true|marker=CREATE_ACCEPTED" }
    $s6BLateAcceptedFixture.hdc_failures = @([ordered]@{ operation = 'PidOf'; occurrence = 16; exit_code = 1; stdout = ''; stderr = '' })
    $s6BLateAcceptedPath = Write-JsonFixture 'simulation-s6-b-frozen-late-accepted-a-absent.json' $s6BLateAcceptedFixture
    $s6BLateAcceptedPaths = New-CasePaths 's6-b-frozen-late-accepted-a-absent'
    $s6BLateAcceptedRun = Invoke-Runner $liveFreezePath $s6BLateAcceptedPaths.Evidence $s6BLateAcceptedPaths.Raw -FixturePath $s6BLateAcceptedPath
    $s6BLateAcceptedRecord = Get-Content -LiteralPath (Join-Path $s6BLateAcceptedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    $s6BLateAcceptedS6 = $s6BLateAcceptedRecord.scenarios[5]
    Assert-True ($s6BLateAcceptedRun.ExitCode -eq 0 -and $s6BLateAcceptedRecord.overall -eq 'fail' -and $s6BLateAcceptedS6.result -eq 'fail' -and $s6BLateAcceptedS6.reason -eq 'two-accepted-sessions-observed') 'S6 frozen reject plus late same-B accepted was downgraded from functional fail'
    Assert-True ($s6BLateAcceptedS6.b_rejected -and $s6BLateAcceptedS6.b_accepted -and [int]$s6BLateAcceptedS6.accepted_session_count_in_window -eq 3 -and $s6BLateAcceptedS6.machine_process_checkpoint.status -eq 'blocked') 'S6 late accepted record lost terminal rejection, accepted count, or true terminal checkpoint'

    Write-Host 'SELFTEST_PHASE=s6-nonfrozen-unexpected-accepted-invalid'
    foreach ($unexpectedCase in @([ordered]@{ label = 'foreign'; request_id = 'foreign6' }, [ordered]@{ label = 'missing'; request_id = 'missing' })) {
        $unexpectedFixture = New-S6BAuthorizationFixture 'frozen'
        $unexpectedFixture.scenario_events.'6'[-1].text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203001,name=BusinessError,message=conflict"
        $unexpectedFixture.scenario_events.'6' += [ordered]@{ offset_seconds = 10; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=$([string]$unexpectedCase.request_id)|accepted=true|marker=CREATE_ACCEPTED" }
        $unexpectedPath = Write-JsonFixture "simulation-s6-nonfrozen-$([string]$unexpectedCase.label)-accepted.json" $unexpectedFixture
        $unexpectedPaths = New-CasePaths "s6-nonfrozen-$([string]$unexpectedCase.label)-accepted"
        $unexpectedRun = Invoke-Runner $liveFreezePath $unexpectedPaths.Evidence $unexpectedPaths.Raw -FixturePath $unexpectedPath
        $unexpectedRecord = Get-Content -LiteralPath (Join-Path $unexpectedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
        Assert-True ($unexpectedRun.ExitCode -eq 2 -and $unexpectedRecord.overall -eq 'invalid' -and [string]$unexpectedRecord.scenario_invalid.reason -eq 'unexpected-accepted-request-in-window') "S6 nonfrozen $([string]$unexpectedCase.label) accepted marker did not take unexpected-accepted invalid priority"
    }

    Write-Host 'SELFTEST_PHASE=c7-s4-deny-pre-capture-proof'
    # S4 deny proof must be capturable BEFORE the operator clicks Deny: a fixture with no B
    # rejection marker must pass via pre-screenshot + manual confirmation + no-B-create over the
    # full window, and the deny capture hdc-command must precede the scenario-4 observation record
    # (i.e. it happened before the action, not after action+60s).
    $s4PreCaptureFixture = New-SimulationFixture
    $s4PreCaptureFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    )
    $s4PreCapturePath = Write-JsonFixture 'simulation-c7-s4-deny-pre-capture.json' $s4PreCaptureFixture
    $s4PreCapturePaths = New-CasePaths 'c7-s4-deny-pre-capture'
    $s4PreCaptureRun = Invoke-Runner $liveFreezePath $s4PreCapturePaths.Evidence $s4PreCapturePaths.Raw -FixturePath $s4PreCapturePath
    Assert-True ($s4PreCaptureRun.ExitCode -eq 0) "S4 pre-capture deny proof simulation crashed: $($s4PreCaptureRun.Text)"
    $s4PreCaptureRecord = Get-Content -LiteralPath (Join-Path $s4PreCapturePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s4PreCaptureRecord.scenarios[3].result -eq 'pass' -and $s4PreCaptureRecord.scenarios[3].reason -eq 'deny-layout-and-full-window-without-B-create') 'S4 pre-capture deny proof did not pass'
    Assert-True ($s4PreCaptureRecord.scenarios[3].deny_screen -eq $true -and $s4PreCaptureRecord.scenarios[3].deny_screen_capture.status -eq 'collected' -and $s4PreCaptureRecord.scenarios[3].deny_screen_capture.visible -eq $true -and $s4PreCaptureRecord.scenarios[3].deny_screen_capture.result -eq 'pass') 'S4 pre-capture deny screen fields mismatch'
    $s4TranscriptEntries = @(Get-Content -LiteralPath (Join-Path $s4PreCapturePaths.Evidence 'projection\transcript.redacted.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    $s4CaptureIndex = @($s4TranscriptEntries | Where-Object { [string]$_.payload.kind -eq 'hdc-command' -and [string]$_.payload.data.operation -eq 'ScreenCap' -and (([string[]]$_.payload.data.arguments) -join ' ') -match 'scenario-4-authorization' } | ForEach-Object { [int]$_.payload.index } | Select-Object -First 1)
    $s4ObservationIndex = @($s4TranscriptEntries | Where-Object { [string]$_.payload.kind -eq 'scenario-observation' -and [int]$_.payload.data.scenario -eq 4 } | ForEach-Object { [int]$_.payload.index } | Select-Object -First 1)
    Assert-True ($s4CaptureIndex.Count -eq 1 -and $s4ObservationIndex.Count -eq 1 -and $s4CaptureIndex[0] -lt $s4ObservationIndex[0]) 'S4 deny capture happened after the observation (after the Deny action)'
    Assert-True ($s4PreCaptureRecord.scenarios[3].full_window_after_action -eq $true) 'S4 pre-capture deny proof did not observe the full 60s window'
    Assert-ManifestAndSeal $s4PreCapturePaths.Evidence
    Assert-ProjectionChain $s4PreCapturePaths.Evidence

    Write-Host 'SELFTEST_PHASE=m3-scenario6-new-b-ui-start'
    $m3NewStartFixture = New-SimulationFixture
    $m3NewStartFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    )
    $m3NewStartFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN" }
    )
    $m3NewStartPath = Write-JsonFixture 'simulation-m3-new-b-start.json' $m3NewStartFixture
    $m3NewStartPaths = New-CasePaths 'm3-new-b-start'
    $m3NewStartRun = Invoke-Runner $liveFreezePath $m3NewStartPaths.Evidence $m3NewStartPaths.Raw -FixturePath $m3NewStartPath
    Assert-True ($m3NewStartRun.ExitCode -eq 0) "M3 new B UI_START simulation crashed: $($m3NewStartRun.Text)"
    $m3NewStartRecord = Get-Content -LiteralPath (Join-Path $m3NewStartPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m3NewStartRecord.scenarios[5].result -eq 'pass' -and $m3NewStartRecord.scenarios[5].reason -eq 'B-explicit-conflict-rejection') 'M3 scenario 6 with new B UI_START did not pass with expected reason'
    Assert-True ($m3NewStartRecord.scenarios[5].a_accepted -eq $true -and $m3NewStartRecord.scenarios[5].request_id_b -eq 'b6') 'M3 scenario 6 a_accepted or request_id_b mismatch'
    Assert-True (($m3NewStartRecord.scenarios[5].observation.events.text -join "`n") -notmatch 'UI_START\|bundle=cn\.alfadb\.netbird\.e3physvpnb\|requestId=b4') 'released S4 request polluted scenario 6 new B UI_START correlation'

    Write-Host 'SELFTEST_PHASE=m3-scenario6-no-new-b-ui-start'
    $m3NoNewStartFixture = New-SimulationFixture
    $m3NoNewStartFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    )
    $m3NoNewStartFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpnb|reason=operation-pending" }
    )
    $m3NoNewStartPath = Write-JsonFixture 'simulation-m3-no-new-b-start.json' $m3NoNewStartFixture
    $m3NoNewStartPaths = New-CasePaths 'm3-no-new-b-start'
    $m3NoNewStartRun = Invoke-Runner $liveFreezePath $m3NoNewStartPaths.Evidence $m3NoNewStartPaths.Raw -FixturePath $m3NoNewStartPath
    Assert-True ($m3NoNewStartRun.ExitCode -eq 2) "M3 no new B UI_START did not invalidate immediately: $($m3NoNewStartRun.Text)"
    $m3NoNewStartRecord = Get-Content -LiteralPath (Join-Path $m3NoNewStartPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m3NoNewStartRecord.overall -eq 'invalid' -and $m3NoNewStartRecord.scenarios[5].result -eq 'invalid' -and $m3NoNewStartRecord.scenarios[5].reason -match 'UI_START_SKIPPED') 'M3 scenario 6 without new B UI_START was not invalid'
    Assert-True ($null -eq $m3NoNewStartRecord.scenarios[6].PSObject.Properties['observation'] -and $m3NoNewStartRecord.cleanup_result.verified_absent) 'M3 invalid run continued into S7 or skipped verified cleanup'

    Write-Host 'SELFTEST_PHASE=m4-s7-active-request-binding-and-screenshot-naming'
    # Under ADJ-20260807-0003 B3, S7 binds the calculated active A/B request only. Null activeRequest
    # (S6 produced no UI_START) must not fall back to window-event inference, even if a complete
    # foreign stop chain is present.
    $m4NullActiveFixture = New-SimulationFixture
    $m4NullActiveFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpna|reason=operation-pending" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpnb|reason=operation-pending" }
    )
    $m4NullActiveFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') STOP_PROMISE_RESOLVED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a5" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $m4NullActivePath = Write-JsonFixture 'simulation-m4-null-active-s7.json' $m4NullActiveFixture
    $m4NullActivePaths = New-CasePaths 'm4-null-active-s7'
    $m4NullActiveRun = Invoke-Runner $liveFreezePath $m4NullActivePaths.Evidence $m4NullActivePaths.Raw -FixturePath $m4NullActivePath
    Assert-True ($m4NullActiveRun.ExitCode -eq 2) "M4 null-active protocol did not invalidate in S6: $($m4NullActiveRun.Text)"
    $m4NullActiveRecord = Get-Content -LiteralPath (Join-Path $m4NullActivePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m4NullActiveRecord.overall -eq 'invalid' -and $m4NullActiveRecord.scenarios[5].result -eq 'invalid' -and $m4NullActiveRecord.scenarios[5].reason -match 'UI_START_SKIPPED') 'M4 invalid S6 action was not surfaced'
    Assert-True ($null -eq $m4NullActiveRecord.scenarios[6].PSObject.Properties['observation'] -and $m4NullActiveRecord.cleanup_result.verified_absent) 'M4 invalid S6 still entered S7 or skipped cleanup'

    # Positive binding: S6 keeps A active as a6; S7 stop chain must use the same bound id.
    $m4BoundFixture = New-SimulationFixture
    $m4BoundPath = Write-JsonFixture 'simulation-m4-bound-s7.json' $m4BoundFixture
    $m4BoundPaths = New-CasePaths 'm4-bound-s7'
    $m4BoundRun = Invoke-Runner $liveFreezePath $m4BoundPaths.Evidence $m4BoundPaths.Raw -FixturePath $m4BoundPath
    Assert-True ($m4BoundRun.ExitCode -eq 0) "M4 bound S7 simulation crashed: $($m4BoundRun.Text)"
    $m4BoundRecord = Get-Content -LiteralPath (Join-Path $m4BoundPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m4BoundRecord.scenarios[6].request_id -eq 'a6' -and $m4BoundRecord.scenarios[6].reason -eq 'terminal-and-post-destroy-snapshot-confirmed') 'M4 scenario 7 did not bind active S6 request a6'
    Assert-True ($m4BoundRecord.scenarios[6].result -eq 'pass' -and $m4BoundRecord.scenarios[6].reason -notmatch 'requestId-missing') 'M4 bound scenario 7 must not report requestId-missing'
    Assert-True ($m4BoundRecord.scenarios[6].post_cleanup_capture -eq $true -and $m4BoundRecord.scenarios[6].post_cleanup_capture_name -eq 'scenario-7-post-cleanup') 'M4 post-cleanup screenshot naming mismatch'
    $m4BoundScreens = @($m4BoundRecord.screenshot_reference | Where-Object { $_.name -eq 'scenario-7-post-cleanup' })
    Assert-True ($m4BoundScreens.Count -ge 1) 'M4 post-cleanup screenshot reference missing'

    $m4NoCleanupFixture = New-SimulationFixture
    $m4NoCleanupFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') STOP_PROMISE_RESOLVED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" }
    )
    $m4NoCleanupPath = Write-JsonFixture 'simulation-m4-no-cleanup-s7.json' $m4NoCleanupFixture
    $m4NoCleanupPaths = New-CasePaths 'm4-no-cleanup-s7'
    $m4NoCleanupRun = Invoke-Runner $liveFreezePath $m4NoCleanupPaths.Evidence $m4NoCleanupPaths.Raw -FixturePath $m4NoCleanupPath
    Assert-True ($m4NoCleanupRun.ExitCode -eq 2) "M4 missing S7 destroy postcondition did not invalidate: $($m4NoCleanupRun.Text)"
    $m4NoCleanupRecord = Get-Content -LiteralPath (Join-Path $m4NoCleanupPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m4NoCleanupRecord.overall -eq 'invalid' -and $m4NoCleanupRecord.scenarios[6].result -eq 'invalid' -and $m4NoCleanupRecord.scenarios[6].reason -match 'onDestroy-or-destroy-begin') 'M4 scenario 7 missing destroy postcondition was not invalid'
    Assert-True ($m4NoCleanupRecord.cleanup_result.verified_absent -and $m4NoCleanupRecord.cleanup_result.status -eq 'verified-clean') 'M4 invalid S7 did not finish verified finally cleanup'

    Write-Host 'SELFTEST_PHASE=adj-s3-strict-process-boundary-fallback-and-reactivation'
    $s3FallbackFixture = New-SimulationFixture
    $s3FallbackFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" }
    )
    $s3FallbackPath = Write-JsonFixture 'simulation-adj-s3-fallback.json' $s3FallbackFixture
    $s3FallbackPaths = New-CasePaths 'adj-s3-fallback'
    $s3FallbackRun = Invoke-Runner $liveFreezePath $s3FallbackPaths.Evidence $s3FallbackPaths.Raw -FixturePath $s3FallbackPath
    Assert-True ($s3FallbackRun.ExitCode -eq 0) "S3 strict fallback simulation crashed: $($s3FallbackRun.Text)"
    $s3FallbackRecord = Get-Content -LiteralPath (Join-Path $s3FallbackPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3FallbackRecord.scenarios[2].result -eq 'pass' -and $s3FallbackRecord.scenarios[2].terminal_mode -eq 'strict-process-boundary' -and $s3FallbackRecord.scenarios[2].reason -eq 'strict-process-boundary-terminal') 'S3 strict-process-boundary fallback did not pass'
    Assert-True (@($s3FallbackRecord.scenarios[2].process_final_state_probes).Count -ge 2) 'S3 fallback probes missing'
    Assert-True ($s3FallbackRecord.scenarios[2].bundle_present_during_probe -eq $true) 'S3 fallback bundle was not present during probes'
    Assert-True ($s3FallbackRecord.scenarios[2].clean_reactivation_proof -eq $true) 'S3 fallback clean reactivation proof missing'
    Assert-True ($s3FallbackRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'S3 fallback with S5 reactivation did not aggregate pass'

    Write-Host 'SELFTEST_PHASE=adj-s3-fallback-without-s5-reactivation-blocked'
    # Full combination: S3 passes via strict-process-boundary, but the same-bundle S5 fresh
    # start/create lacks a post-create open snapshot. S3 must record clean_reactivation_proof=false,
    # S5 stays blocked, and aggregation/overall stays blocked despite every scenario result.
    $s3NoReactivationFixture = New-SimulationFixture
    $s3NoReactivationFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" }
    )
    $s3NoReactivationFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" }
    )
    $s3NoReactivationPath = Write-JsonFixture 'simulation-adj-s3-no-reactivation.json' $s3NoReactivationFixture
    $s3NoReactivationPaths = New-CasePaths 'adj-s3-no-reactivation'
    $s3NoReactivationRun = Invoke-Runner $liveFreezePath $s3NoReactivationPaths.Evidence $s3NoReactivationPaths.Raw -FixturePath $s3NoReactivationPath
    Assert-True ($s3NoReactivationRun.ExitCode -eq 0) "S3 fallback without reactivation simulation crashed: $($s3NoReactivationRun.Text)"
    $s3NoReactivationRecord = Get-Content -LiteralPath (Join-Path $s3NoReactivationPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3NoReactivationRecord.scenarios[2].result -eq 'pass' -and $s3NoReactivationRecord.scenarios[2].terminal_mode -eq 'strict-process-boundary') 'S3 strict fallback without reactivation did not pass individually'
    Assert-True ($s3NoReactivationRecord.scenarios[2].clean_reactivation_proof -eq $false) 'S3 clean_reactivation_proof not false without S5 post-create open'
    Assert-True ($null -ne $s3NoReactivationRecord.scenario_aggregation.s3_clean_reactivation_proof -and $s3NoReactivationRecord.scenario_aggregation.s3_clean_reactivation_proof -eq $false) 'aggregation s3_clean_reactivation_proof not false without reactivation proof'
    Assert-True ($s3NoReactivationRecord.scenarios[4].result -eq 'blocked' -and $s3NoReactivationRecord.scenarios[4].reason -eq 'fresh-create-proof-missing') 'S5 fresh create without post-create open did not stay blocked'
    Assert-True ([string]$s3NoReactivationRecord.scenario_aggregation.measured_scenario_overall -eq 'blocked' -and [string]$s3NoReactivationRecord.scenario_aggregation.overall -eq 'blocked') 'S3 strict fallback without reactivation proof did not block aggregation/overall'

    Write-Host 'SELFTEST_PHASE=adj-s5-fd-still-open-not-overridable'
    # S5 current request shows a post-destroy FD_STILL_OPEN marker; consecutive-absent probes must
    # never override the fail verdict.
    $s5FdStillOpenFixture = New-SimulationFixture
    $s5FdStillOpenFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_STILL_OPEN" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s5FdStillOpenPath = Write-JsonFixture 'simulation-adj-s5-fd-still-open.json' $s5FdStillOpenFixture
    $s5FdStillOpenPaths = New-CasePaths 'adj-s5-fd-still-open'
    $s5FdStillOpenRun = Invoke-Runner $liveFreezePath $s5FdStillOpenPaths.Evidence $s5FdStillOpenPaths.Raw -FixturePath $s5FdStillOpenPath
    Assert-True ($s5FdStillOpenRun.ExitCode -eq 0) "S5 FD_STILL_OPEN simulation crashed: $($s5FdStillOpenRun.Text)"
    $s5FdStillOpenRecord = Get-Content -LiteralPath (Join-Path $s5FdStillOpenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5FdStillOpenRecord.scenarios[4].result -eq 'fail' -and $s5FdStillOpenRecord.scenarios[4].reason -eq 'FD_STILL_OPEN' -and $s5FdStillOpenRecord.scenarios[4].fd_still_open -eq $true) 'S5 post-destroy FD_STILL_OPEN did not fail'
    Assert-True (@($s5FdStillOpenRecord.scenarios[4].process_final_state_probes | Where-Object { $_.status -eq 'absent' }).Count -ge 2 -and $s5FdStillOpenRecord.scenarios[4].process_absent_evidence.met -eq $true) 'S5 FD_STILL_OPEN case had no absent probe evidence that would have been overridden'

    Write-Host 'SELFTEST_PHASE=adj-s5-pre-destroy-open-not-fail'
    # A pre-destroy open snapshot is expected mid-destroy and must never fail S5.
    $s5PreDestroyOpenFixture = New-SimulationFixture
    $s5PreDestroyOpenFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a5|trigger=onDestroy" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" }
    )
    $s5PreDestroyOpenPath = Write-JsonFixture 'simulation-adj-s5-pre-destroy-open.json' $s5PreDestroyOpenFixture
    $s5PreDestroyOpenPaths = New-CasePaths 'adj-s5-pre-destroy-open'
    $s5PreDestroyOpenRun = Invoke-Runner $liveFreezePath $s5PreDestroyOpenPaths.Evidence $s5PreDestroyOpenPaths.Raw -FixturePath $s5PreDestroyOpenPath
    Assert-True ($s5PreDestroyOpenRun.ExitCode -eq 0) "S5 pre-destroy open simulation crashed: $($s5PreDestroyOpenRun.Text)"
    $s5PreDestroyOpenRecord = Get-Content -LiteralPath (Join-Path $s5PreDestroyOpenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5PreDestroyOpenRecord.scenarios[4].result -eq 'pass' -and $s5PreDestroyOpenRecord.scenarios[4].reason -eq 'settings-app-info-force-stop-terminal') 'S5 pre-destroy open snapshot wrongly failed'
    Assert-True ($s5PreDestroyOpenRecord.scenarios[4].fd_still_open -eq $false) 'S5 pre-destroy open recorded as fd_still_open'

    Write-Host 'SELFTEST_PHASE=adj-s5-probe-spacing-recheck-blocked'
    # Force-stop assessment must re-check recorded probe timestamps instead of trusting execution
    # Wait: a probe_spacing_override below the freeze 3s spacing stays blocked.
    $s5SpacingOverrideFixture = New-SimulationFixture
    $s5SpacingOverrideFixture.probe_spacing_override_seconds = 1
    $s5SpacingOverridePath = Write-JsonFixture 'simulation-adj-s5-spacing-override.json' $s5SpacingOverrideFixture
    $s5SpacingOverridePaths = New-CasePaths 'adj-s5-spacing-override'
    $s5SpacingOverrideRun = Invoke-Runner $liveFreezePath $s5SpacingOverridePaths.Evidence $s5SpacingOverridePaths.Raw -FixturePath $s5SpacingOverridePath
    Assert-True ($s5SpacingOverrideRun.ExitCode -eq 2) "S5 spacing override did not invalidate: $($s5SpacingOverrideRun.Text)"
    $s5SpacingOverrideRecord = Get-Content -LiteralPath (Join-Path $s5SpacingOverridePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5SpacingOverrideRecord.overall -eq 'invalid' -and $s5SpacingOverrideRecord.scenarios[4].result -eq 'invalid' -and $s5SpacingOverrideRecord.scenarios[4].reason -match 'probe-spacing-insufficient') 'S5 probe spacing override was not invalid'
    Assert-True ($s5SpacingOverrideRecord.cleanup_result.verified_absent) 'S5 spacing invalid did not finish cleanup'

    Write-Host 'SELFTEST_PHASE=adj-s5-probe-infrastructure-reason'
    # Simulation probe 124/125 must classify as infrastructure exactly like live HDC: campaign
    # infrastructure_reason becomes hdc-usb-interruption and the probe aborts as unknown/error.
    $s5ProbeInfraFixture = New-SimulationFixture
    # ADJ-20260808-0003: S2 adds a precise `:vpn` process-target checkpoint after CREATE_ACCEPTED
    # (+1 BundleDump), so the first S5 force-stop terminal probe BundleDump is occurrence 14.
    $s5ProbeInfraFixture.hdc_failures = @(
        [ordered]@{ operation = 'BundleDump'; occurrence = 14; exit_code = 124; stdout = ''; stderr = 'timeout' }
    )
    $s5ProbeInfraPath = Write-JsonFixture 'simulation-adj-s5-probe-infra.json' $s5ProbeInfraFixture
    $s5ProbeInfraPaths = New-CasePaths 'adj-s5-probe-infra'
    $s5ProbeInfraRun = Invoke-Runner $liveFreezePath $s5ProbeInfraPaths.Evidence $s5ProbeInfraPaths.Raw -FixturePath $s5ProbeInfraPath
    Assert-True ($s5ProbeInfraRun.ExitCode -eq 2) "S5 probe infrastructure interruption did not stop: $($s5ProbeInfraRun.Text)"
    $s5ProbeInfraRecord = Get-Content -LiteralPath (Join-Path $s5ProbeInfraPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5ProbeInfraRecord.scenarios[4].result -eq 'blocked' -and $s5ProbeInfraRecord.scenarios[4].reason -match 'force-stop-process-check-unverifiable') 'S5 probe 124/125 did not remain blocked'
    Assert-True ([string]$s5ProbeInfraRecord.infrastructure_reason -eq 'hdc-usb-interruption' -and $s5ProbeInfraRecord.overall -eq 'blocked') 'S5 probe 124/125 did not set blocked infrastructure reason like live'

    Write-Host 'SELFTEST_PHASE=adj-s5-reopen-not-open'
    # Test-PostCreateOpen matches the open=true field boundary only: a post-create reopen=true
    # snapshot never counts as the clean reactivation proof.
    $s5ReopenFixture = New-SimulationFixture
    $s5ReopenFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|reopen=true|marker=CREATE_ACCEPTED" }
    )
    $s5ReopenPath = Write-JsonFixture 'simulation-adj-s5-reopen-not-open.json' $s5ReopenFixture
    $s5ReopenPaths = New-CasePaths 'adj-s5-reopen-not-open'
    $s5ReopenRun = Invoke-Runner $liveFreezePath $s5ReopenPaths.Evidence $s5ReopenPaths.Raw -FixturePath $s5ReopenPath
    Assert-True ($s5ReopenRun.ExitCode -eq 0) "S5 reopen simulation crashed: $($s5ReopenRun.Text)"
    $s5ReopenRecord = Get-Content -LiteralPath (Join-Path $s5ReopenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5ReopenRecord.scenarios[4].result -eq 'blocked' -and $s5ReopenRecord.scenarios[4].reason -eq 'fresh-create-proof-missing') 'S5 reopen=true snapshot counted as open=true proof'

    Write-Host 'SELFTEST_PHASE=adj-s3-fd-still-open-not-overridable'
    $s3FdOpenFixture = New-SimulationFixture
    $s3FdOpenFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_STILL_OPEN" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s3FdOpenPath = Write-JsonFixture 'simulation-adj-s3-fd-still-open.json' $s3FdOpenFixture
    $s3FdOpenPaths = New-CasePaths 'adj-s3-fd-still-open'
    $s3FdOpenRun = Invoke-Runner $liveFreezePath $s3FdOpenPaths.Evidence $s3FdOpenPaths.Raw -FixturePath $s3FdOpenPath
    Assert-True ($s3FdOpenRun.ExitCode -eq 0) "S3 FD_STILL_OPEN simulation crashed: $($s3FdOpenRun.Text)"
    $s3FdOpenRecord = Get-Content -LiteralPath (Join-Path $s3FdOpenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3FdOpenRecord.scenarios[2].result -eq 'fail' -and $s3FdOpenRecord.scenarios[2].reason -eq 'fd-still-open-after-destroy' -and $s3FdOpenRecord.scenarios[2].terminal_mode -eq 'callback-post-fd') 'S3 FD_STILL_OPEN was overridden by the process fallback'

    Write-Host 'SELFTEST_PHASE=adj-s3-s7-terminal-missing-post-destroy-fd-still-open-fail'
    # Destroy terminal is missing (no VPN_DESTROY_RESOLVED/REJECTED) but a post-destroy-phase
    # FD_STILL_OPEN snapshot is present and the strict-process-boundary prerequisites are complete
    # with consecutive absent probes: the leaked fd must still fail and the fallback must not pass.
    $s3TermMissingFdFixture = New-SimulationFixture
    $s3TermMissingFdFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s3TermMissingFdFixture.process_probe_override = [ordered]@{ '3' = @(
        [ordered]@{ pid = 'absent'; dump = 'present' },
        [ordered]@{ pid = 'absent'; dump = 'present' }
    ) }
    $s3TermMissingFdPath = Write-JsonFixture 'simulation-adj-s3-terminal-missing-fd-still-open.json' $s3TermMissingFdFixture
    $s3TermMissingFdPaths = New-CasePaths 'adj-s3-terminal-missing-fd-still-open'
    $s3TermMissingFdRun = Invoke-Runner $liveFreezePath $s3TermMissingFdPaths.Evidence $s3TermMissingFdPaths.Raw -FixturePath $s3TermMissingFdPath
    Assert-True ($s3TermMissingFdRun.ExitCode -eq 0) "S3 terminal-missing FD_STILL_OPEN simulation crashed: $($s3TermMissingFdRun.Text)"
    $s3TermMissingFdRecord = Get-Content -LiteralPath (Join-Path $s3TermMissingFdPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3TermMissingFdRecord.scenarios[2].result -eq 'fail' -and $s3TermMissingFdRecord.scenarios[2].reason -eq 'fd-still-open-after-destroy' -and $s3TermMissingFdRecord.scenarios[2].terminal_mode -eq 'callback-post-fd') 'S3 terminal-missing post-destroy FD_STILL_OPEN did not fail'
    Assert-True (@($s3TermMissingFdRecord.scenarios[2].process_final_state_probes | Where-Object { $_.status -eq 'absent' }).Count -ge 2) 'S3 terminal-missing case absent probes were not recorded (fallback evidence absent)'

    $s7TermMissingFdFixture = New-SimulationFixture
    $s7TermMissingFdFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy|createAccepted=true" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s7TermMissingFdFixture.process_probe_override = [ordered]@{ '7' = @(
        [ordered]@{ pid = 'absent'; dump = 'present' },
        [ordered]@{ pid = 'absent'; dump = 'present' }
    ) }
    $s7TermMissingFdPath = Write-JsonFixture 'simulation-adj-s7-terminal-missing-fd-still-open.json' $s7TermMissingFdFixture
    $s7TermMissingFdPaths = New-CasePaths 'adj-s7-terminal-missing-fd-still-open'
    $s7TermMissingFdRun = Invoke-Runner $liveFreezePath $s7TermMissingFdPaths.Evidence $s7TermMissingFdPaths.Raw -FixturePath $s7TermMissingFdPath
    Assert-True ($s7TermMissingFdRun.ExitCode -eq 0) "S7 terminal-missing FD_STILL_OPEN simulation crashed: $($s7TermMissingFdRun.Text)"
    $s7TermMissingFdRecord = Get-Content -LiteralPath (Join-Path $s7TermMissingFdPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s7TermMissingFdRecord.scenarios[6].result -eq 'fail' -and $s7TermMissingFdRecord.scenarios[6].reason -eq 'fd-still-open-after-destroy' -and $s7TermMissingFdRecord.scenarios[6].terminal_mode -eq 'callback-post-fd') 'S7 terminal-missing post-destroy FD_STILL_OPEN did not fail'
    Assert-True (-not $s7TermMissingFdRecord.scenarios[6].post_cleanup_capture -and -not $s7TermMissingFdRecord.scenarios[6].terminal_assessed) 'S7 FD_STILL_OPEN case still ran uninstall cleanup during scenario'

    Write-Host 'SELFTEST_PHASE=adj-s5-fd-still-open-with-capture-degraded-fail'
    # The S5 post-force capture is observation-only. A degraded artifact cannot override the
    # independently decisive post-destroy FD_STILL_OPEN failure.
    $s5FdDegradedFixture = New-SimulationFixture
    $s5FdDegradedFixture.capture_failures = @('scenario-5-app-info-force-stop')
    $s5FdDegradedFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_STILL_OPEN" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s5FdDegradedPath = Write-JsonFixture 'simulation-adj-s5-fd-still-open-degraded.json' $s5FdDegradedFixture
    $s5FdDegradedPaths = New-CasePaths 'adj-s5-fd-still-open-degraded'
    $s5FdDegradedRun = Invoke-Runner $liveFreezePath $s5FdDegradedPaths.Evidence $s5FdDegradedPaths.Raw -FixturePath $s5FdDegradedPath
    Assert-True ($s5FdDegradedRun.ExitCode -eq 0) "S5 observation-only capture loss crashed: $($s5FdDegradedRun.Text)"
    $s5FdDegradedRecord = Get-Content -LiteralPath (Join-Path $s5FdDegradedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5FdDegradedRecord.overall -eq 'fail' -and $s5FdDegradedRecord.scenarios[4].result -eq 'fail' -and $s5FdDegradedRecord.scenarios[4].reason -eq 'FD_STILL_OPEN') 'S5 observation-only capture loss changed the functional failure'
    Assert-True ($s5FdDegradedRecord.scenarios[4].app_info_force_stop_capture.status -eq 'degraded' -and $s5FdDegradedRecord.scenarios[4].app_info_force_stop_capture.observation_only) 'S5 degraded post-force artifact was not recorded as observation-only'

    Write-Host 'SELFTEST_PHASE=adj-s2-create-rejected-with-capture-degraded-fail'
    # A missing decisive after-Allow capture makes the protocol unverifiable and outranks functional evidence.
    $s2RejectedFixture = New-SimulationFixture
    $s2RejectedFixture.capture_failures = @('scenario-2-after-allow')
    $s2RejectedFixture.scenario_events.'2' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=a2|phase=create|summary=create-rejected" }
    )
    $s2RejectedPath = Write-JsonFixture 'simulation-adj-s2-create-rejected-degraded.json' $s2RejectedFixture
    $s2RejectedPaths = New-CasePaths 'adj-s2-create-rejected-degraded'
    $s2RejectedRun = Invoke-Runner $liveFreezePath $s2RejectedPaths.Evidence $s2RejectedPaths.Raw -FixturePath $s2RejectedPath
    Assert-True ($s2RejectedRun.ExitCode -eq 2) "S2 decisive capture loss did not invalidate: $($s2RejectedRun.Text)"
    $s2RejectedRecord = Get-Content -LiteralPath (Join-Path $s2RejectedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s2RejectedRecord.overall -eq 'invalid' -and $s2RejectedRecord.scenarios[1].result -eq 'invalid' -and $s2RejectedRecord.scenarios[1].reason -match 'authorization-not-dismissed|capture-not-collected') 'S2 decisive capture loss was not protocol invalid'
    Assert-True ($null -eq $s2RejectedRecord.scenarios[2].PSObject.Properties['observation'] -and $s2RejectedRecord.cleanup_result.verified_absent) 'S2 invalid run continued or skipped cleanup'

    Write-Host 'SELFTEST_PHASE=adj-s4-deny-created-with-capture-degraded-fail'
    # The authorization pre-capture must be machine-verifiable before Deny is offered.
    $s4DenyCreatedFixture = New-SimulationFixture
    $s4DenyCreatedFixture.capture_failures = @('scenario-4-authorization')
    $s4DenyCreatedFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    )
    $s4DenyCreatedPath = Write-JsonFixture 'simulation-adj-s4-deny-created-degraded.json' $s4DenyCreatedFixture
    $s4DenyCreatedPaths = New-CasePaths 'adj-s4-deny-created-degraded'
    $s4DenyCreatedRun = Invoke-Runner $liveFreezePath $s4DenyCreatedPaths.Evidence $s4DenyCreatedPaths.Raw -FixturePath $s4DenyCreatedPath
    Assert-True ($s4DenyCreatedRun.ExitCode -eq 2) "S4 authorization pre-capture loss did not invalidate: $($s4DenyCreatedRun.Text)"
    $s4DenyCreatedRecord = Get-Content -LiteralPath (Join-Path $s4DenyCreatedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s4DenyCreatedRecord.record_status -eq 'invalidated' -and $s4DenyCreatedRecord.scenarios[3].result -eq 'invalid') 'S4 pre-action capture loss did not stop the scenario as invalid'
    Assert-True ($s4DenyCreatedRecord.cleanup_result.verified_absent) 'S4 invalid run did not finish cleanup'

    Write-Host 'SELFTEST_PHASE=adj-s6-conflict-checkpoint-capture-required'
    $s6CaptureFixture = New-SimulationFixture
    $s6CaptureFixture.capture_failures = @('scenario-6-conflict')
    $s6CapturePath = Write-JsonFixture 'simulation-adj-s6-conflict-capture-required.json' $s6CaptureFixture
    $s6CapturePaths = New-CasePaths 'adj-s6-conflict-capture-required'
    $s6CaptureRun = Invoke-Runner $liveFreezePath $s6CapturePaths.Evidence $s6CapturePaths.Raw -FixturePath $s6CapturePath
    Assert-True ($s6CaptureRun.ExitCode -eq 2) "S6 continued after its conflict checkpoint capture failed: $($s6CaptureRun.Text)"
    $s6CaptureRecord = Get-Content -LiteralPath (Join-Path $s6CapturePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6CaptureRecord.record_status -eq 'invalidated' -and $s6CaptureRecord.scenarios[5].result -eq 'invalid') 'S6 conflict capture failure did not invalidate the scenario'
    Assert-True ($s6CaptureRecord.cleanup_result.verified_absent) 'S6 conflict checkpoint invalidation did not finish cleanup'

    Write-Host 'SELFTEST_PHASE=adj-s6-legacy-confirmation-object-ignored'
    $s6LegacyFixture = New-SimulationFixture
    $s6LegacyFixture.operator['confirmations'] = [ordered]@{ legacy_semantic_claim = $true }
    $s6LegacyPath = Write-JsonFixture 'simulation-adj-s6-legacy-confirmation-ignored.json' $s6LegacyFixture
    $s6LegacyPaths = New-CasePaths 'adj-s6-legacy-confirmation-ignored'
    $s6LegacyRun = Invoke-Runner $liveFreezePath $s6LegacyPaths.Evidence $s6LegacyPaths.Raw -FixturePath $s6LegacyPath
    Assert-True ($s6LegacyRun.ExitCode -eq 0) "S6 legacy confirmation compatibility simulation crashed: $($s6LegacyRun.Text)"
    $s6LegacyRecord = Get-Content -LiteralPath (Join-Path $s6LegacyPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6LegacyRecord.scenarios[5].result -eq 'pass' -and $s6LegacyRecord.scenarios[5].reason -eq 'B-explicit-conflict-rejection') 'S6 consumed a legacy semantic confirmation object'
    Assert-True (-not ($s6LegacyRecord.scenarios[5].PSObject.Properties.Name -contains 'operator_state')) 'S6 evidence retained removed semantic operator fields'

    Write-Host 'SELFTEST_PHASE=semantic-layout-mismatch-stops-before-next-action'
    $layoutMismatchFixture = New-SimulationFixture
    $layoutMismatchFixture.layout_profiles['scenario-4-authorization'] = 'wrong-page'
    $layoutMismatchPath = Write-JsonFixture 'simulation-layout-review-mismatch.json' $layoutMismatchFixture
    $layoutMismatchPaths = New-CasePaths 'layout-review-mismatch'
    $layoutMismatchRun = Invoke-Runner $liveFreezePath $layoutMismatchPaths.Evidence $layoutMismatchPaths.Raw -FixturePath $layoutMismatchPath
    Assert-True ($layoutMismatchRun.ExitCode -eq 2) "layout mismatch did not invalidate immediately: $($layoutMismatchRun.Text)"
    $layoutMismatchRecord = Get-Content -LiteralPath (Join-Path $layoutMismatchPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($layoutMismatchRecord.record_status -eq 'invalidated' -and $layoutMismatchRecord.invalidated_step.step_index -eq 2) 'layout mismatch invalidated the wrong step'
    $layoutMismatchTranscriptPath = Join-Path $layoutMismatchPaths.Evidence 'projection\transcript.redacted.jsonl'
    $layoutMismatchTranscriptText = Get-Content -LiteralPath $layoutMismatchTranscriptPath -Raw
    $layoutMismatchTranscript = @(Get-Content -LiteralPath $layoutMismatchTranscriptPath | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    Assert-True (@($layoutMismatchTranscript | Where-Object { [string]$_.payload.kind -eq 'machine-layout-checkpoint' -and [string]$_.payload.data.checkpoint.name -eq 'scenario-4-authorization' -and $_.payload.data.checkpoint.matching -eq $false }).Count -eq 1) 'layout mismatch assessment missing from transcript'
    Assert-True (@($layoutMismatchTranscript | Where-Object { [string]$_.payload.kind -eq 'operator-mechanical-action' -and [int]$_.payload.data.scenario -eq 4 -and [int]$_.payload.data.step_index -eq 2 }).Count -eq 0) 'Deny action occurred after the failed pre-action visual gate'
    Assert-True ($layoutMismatchTranscriptText -notmatch 'NO-DUAL-ACTIVE-CAPTURED|DUAL-ACTIVE-CAPTURED|FINAL-CLEANUP-CAPTURED|Confirm-VisibleFact|Read-OperatorResponse') 'transcript retained a removed semantic confirmation token'
    Assert-True ($layoutMismatchRecord.scenarios[4].reason -eq 'not-run-due-to-invalid' -and $layoutMismatchRecord.scenarios[5].reason -eq 'not-run-due-to-invalid' -and $layoutMismatchRecord.scenarios[6].reason -eq 'not-run-due-to-invalid') 'later scenarios were not stopped as not-run-due-to-invalid'

    Write-Host 'SELFTEST_PHASE=adj-s5-vpn-page-not-required'
    # ADJ-20260808-0003: the Settings>VPN page is no longer a decisive step and is never asked
    # of the operator. A capture failure there must never invalidate/block/pass S5.
    $s5ObsOnlyFixture = New-SimulationFixture
    $s5ObsOnlyFixture.capture_failures = @('scenario-5-settings-vpn-page')
    $s5ObsOnlyPath = Write-JsonFixture 'simulation-adj-s5-vpn-page-not-required.json' $s5ObsOnlyFixture
    $s5ObsOnlyPaths = New-CasePaths 'adj-s5-vpn-page-not-required'
    $s5ObsOnlyRun = Invoke-Runner $liveFreezePath $s5ObsOnlyPaths.Evidence $s5ObsOnlyPaths.Raw -FixturePath $s5ObsOnlyPath
    Assert-True ($s5ObsOnlyRun.ExitCode -eq 0) "S5 treated the optional Settings>VPN page as decisive: $($s5ObsOnlyRun.Text)"
    $s5ObsOnlyRecord = Get-Content -LiteralPath (Join-Path $s5ObsOnlyPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5ObsOnlyRecord.scenarios[4].result -eq 'pass' -and $s5ObsOnlyRecord.scenarios[4].settings_vpn_page_capture.status -eq 'not-required') 'S5 optional Settings>VPN page changed the scenario result or recorded a decisive capture'

    Write-Host 'SELFTEST_PHASE=adj-s5-force-stop-capture-observation-only'
    $s5ObservationFixture = New-SimulationFixture
    $s5ObservationFixture.capture_failures = @('scenario-5-app-info-force-stop')
    $s5ObservationPath = Write-JsonFixture 'simulation-adj-s5-force-stop-capture-observation-only.json' $s5ObservationFixture
    $s5ObservationPaths = New-CasePaths 'adj-s5-force-stop-capture-observation-only'
    $s5ObservationRun = Invoke-Runner $liveFreezePath $s5ObservationPaths.Evidence $s5ObservationPaths.Raw -FixturePath $s5ObservationPath
    Assert-True ($s5ObservationRun.ExitCode -eq 0) "S5 observation-only post-force capture changed the verdict: $($s5ObservationRun.Text)"
    $s5ObservationRecord = Get-Content -LiteralPath (Join-Path $s5ObservationPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5ObservationRecord.scenarios[4].result -eq 'pass' -and $s5ObservationRecord.record_status -ne 'invalidated') 'S5 observation-only post-force capture invalidated the campaign'
    Assert-True ($s5ObservationRecord.scenarios[4].app_info_force_stop_capture.status -eq 'degraded' -and $s5ObservationRecord.scenarios[4].app_info_force_stop_capture.observation_only) 'S5 observation-only degradation was not recorded'

    Write-Host 'SELFTEST_PHASE=adj-s7-final-destroy-fail-with-capture-degraded-fail'
    # S7 final destroy leaks the fd (FD_STILL_OPEN): fail must outrank the degraded final-state capture.
    $s7FdFailFixture = New-SimulationFixture
    $s7FdFailFixture.capture_failures = @('scenario-7-final-state')
    $s7FdFailFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a6|fdMarker=FD_STILL_OPEN" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN" }
    )
    $s7FdFailPath = Write-JsonFixture 'simulation-adj-s7-final-destroy-fail-degraded.json' $s7FdFailFixture
    $s7FdFailPaths = New-CasePaths 'adj-s7-final-destroy-fail-degraded'
    $s7FdFailRun = Invoke-Runner $liveFreezePath $s7FdFailPaths.Evidence $s7FdFailPaths.Raw -FixturePath $s7FdFailPath
    Assert-True ($s7FdFailRun.ExitCode -eq 0) "S7 final destroy fail degraded simulation crashed: $($s7FdFailRun.Text)"
    $s7FdFailRecord = Get-Content -LiteralPath (Join-Path $s7FdFailPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s7FdFailRecord.scenarios[6].result -eq 'fail' -and $s7FdFailRecord.scenarios[6].reason -eq 'fd-still-open-after-destroy') 'S7 final destroy fail with capture degraded was downgraded to blocked'
    Assert-True (-not $s7FdFailRecord.scenarios[6].post_cleanup_capture -and -not $s7FdFailRecord.scenarios[6].terminal_assessed) 'S7 final destroy fail still ran in-window cleanup'
    Assert-True (@($s7FdFailRecord.screenshot_reference | Where-Object { $_.name -eq 'scenario-7-final-state' -and $_.status -eq 'degraded' }).Count -ge 1) 'S7 degraded fixture did not actually degrade the final-state capture'
    Assert-True ([string]$s7FdFailRecord.scenario_aggregation.measured_scenario_overall -eq 'fail') 'S7 final destroy fail degraded measured aggregation not fail'
    Assert-True ([string]$s7FdFailRecord.scenario_aggregation.overall -eq 'fail' -and [string]$s7FdFailRecord.overall -eq 'fail' -and [string]$s7FdFailRecord.verdict -eq 'fail') 'S7 final destroy fail degraded final overall/verdict downgraded to blocked'

    Write-Host 'SELFTEST_PHASE=adj-s5-force-stop-flow'
    # The force-stop process effect remains decisive: a still-present A <bundle>:vpn is invalid
    # even though the pre-action AppDetail gate matched.
    $s5MissingConfirmFixture = New-SimulationFixture
    $s5MissingConfirmFixture.operator.no_effect_steps = @('5.4')
    $s5MissingConfirmFixture.process_probe_override = [ordered]@{ '5' = @(
        [ordered]@{ pid = 'present'; dump = 'present' },
        [ordered]@{ pid = 'present'; dump = 'present' },
        [ordered]@{ pid = 'present'; dump = 'present' }
    ) }
    $s5MissingConfirmPath = Write-JsonFixture 'simulation-adj-s5-missing-confirm.json' $s5MissingConfirmFixture
    $s5MissingConfirmPaths = New-CasePaths 'adj-s5-missing-confirm'
    $s5MissingConfirmRun = Invoke-Runner $liveFreezePath $s5MissingConfirmPaths.Evidence $s5MissingConfirmPaths.Raw -FixturePath $s5MissingConfirmPath
    Assert-True ($s5MissingConfirmRun.ExitCode -eq 2) "S5 force-stop process still present did not invalidate: $($s5MissingConfirmRun.Text)"
    $s5MissingConfirmRecord = Get-Content -LiteralPath (Join-Path $s5MissingConfirmPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5MissingConfirmRecord.overall -eq 'invalid' -and $s5MissingConfirmRecord.scenarios[4].result -eq 'invalid') 'S5 without machine force-stop effect was not invalid'
    Assert-True ($s5MissingConfirmRecord.scenarios[4].reason -match 'process-state|absent|probe|present') 'S5 force-stop process-still-present reason mismatch'

    Write-Host 'SELFTEST_PHASE=s5-post-force-page-change-observation-only'
    $s5PageChangeFixture = New-SimulationFixture
    $s5PageChangeFixture.layout_profiles['scenario-5-app-info-force-stop'] = 'wrong-page'
    $s5PageChangePath = Write-JsonFixture 'simulation-s5-post-force-page-change.json' $s5PageChangeFixture
    $s5PageChangePaths = New-CasePaths 's5-post-force-page-change'
    $s5PageChangeRun = Invoke-Runner $liveFreezePath $s5PageChangePaths.Evidence $s5PageChangePaths.Raw -FixturePath $s5PageChangePath
    Assert-True ($s5PageChangeRun.ExitCode -eq 0) "post-force page change invalidated S5: $($s5PageChangeRun.Text)"
    $s5PageChangeRecord = Get-Content -LiteralPath (Join-Path $s5PageChangePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5PageChangeRecord.scenarios[4].result -eq 'pass' -and $s5PageChangeRecord.scenarios[4].process_absent_evidence.met -and $s5PageChangeRecord.scenarios[4].bundle_present_during_probe) 'post-force page shape changed the decisive process verdict'

    $s5BundleAbsentFixture = New-SimulationFixture
    $s5BundleAbsentFixture.process_probe_override = [ordered]@{ '5' = @([ordered]@{ pid = 'absent'; dump = 'absent' }) }
    $s5BundleAbsentPath = Write-JsonFixture 'simulation-adj-s5-bundle-absent.json' $s5BundleAbsentFixture
    $s5BundleAbsentPaths = New-CasePaths 'adj-s5-bundle-absent'
    $s5BundleAbsentRun = Invoke-Runner $liveFreezePath $s5BundleAbsentPaths.Evidence $s5BundleAbsentPaths.Raw -FixturePath $s5BundleAbsentPath
    Assert-True ($s5BundleAbsentRun.ExitCode -eq 2) "S5 bundle absent did not stop: $($s5BundleAbsentRun.Text)"
    $s5BundleAbsentRecord = Get-Content -LiteralPath (Join-Path $s5BundleAbsentPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5BundleAbsentRecord.overall -eq 'blocked' -and $s5BundleAbsentRecord.scenarios[4].result -eq 'blocked') 'S5 with non-pass BundleDump during probes was not blocked'
    Assert-True ($s5BundleAbsentRecord.scenarios[4].reason -match 'force-stop-process-check-unverifiable|probe-unknown-or-error|absent|bundle') 'S5 non-pass BundleDump reason mismatch'

    # ADJ-20260808-0003 (C6): the extra UI_STOP_SKIPPED arrives while S5 waits for the platform
    # create terminal. The create-terminal wait re-asserts the verified Start window first, so
    # this extra operator UI action is scenario invalid on the spot (unexpected-UI_STOP_SKIPPED-
    # during-start-step) and is never masked as a platform marker-missing blocked.
    $s5NoFreshFixture = New-SimulationFixture
    $s5NoFreshFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP_SKIPPED|bundle=cn.alfadb.netbird.e3physvpna|reason=no-active-request" }
    )
    $s5NoFreshPath = Write-JsonFixture 'simulation-adj-s5-no-fresh-create.json' $s5NoFreshFixture
    $s5NoFreshPaths = New-CasePaths 'adj-s5-no-fresh-create'
    $s5NoFreshRun = Invoke-Runner $liveFreezePath $s5NoFreshPaths.Evidence $s5NoFreshPaths.Raw -FixturePath $s5NoFreshPath
    Assert-True ($s5NoFreshRun.ExitCode -eq 2) "S5 no fresh create / STOP_SKIPPED did not invalidate: $($s5NoFreshRun.Text)"
    $s5NoFreshRecord = Get-Content -LiteralPath (Join-Path $s5NoFreshPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5NoFreshRecord.overall -eq 'invalid' -and $s5NoFreshRecord.scenarios[4].result -eq 'invalid') 'S5 without fresh create was not invalid'
    Assert-True ($s5NoFreshRecord.scenarios[4].reason -match 'UI_STOP_SKIPPED|fresh-create|create-terminal') 'S5 no-fresh reason mismatch'

    # ADJ-20260808-0003 (C6): pure missing — a verified Start with NO create marker and NO extra
    # UI action while waiting for the platform create terminal is a plain runner blocked
    # (platform-marker-missing:fresh-create-terminal-missing), never a scenario invalid. This
    # contrasts with the no-fresh case above where the extra UI_STOP_SKIPPED makes it invalid.
    $s5PureMissingFixture = New-SimulationFixture
    $s5PureMissingFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" }
    )
    $s5PureMissingPath = Write-JsonFixture 'simulation-adj-s5-pure-missing-create.json' $s5PureMissingFixture
    $s5PureMissingPaths = New-CasePaths 'adj-s5-pure-missing-create'
    $s5PureMissingRun = Invoke-Runner $liveFreezePath $s5PureMissingPaths.Evidence $s5PureMissingPaths.Raw -FixturePath $s5PureMissingPath
    Assert-True ($s5PureMissingRun.ExitCode -eq 2) "S5 pure-missing create terminal did not stop as blocked: $($s5PureMissingRun.Text)"
    $s5PureMissingRecord = Get-Content -LiteralPath (Join-Path $s5PureMissingPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5PureMissingRecord.overall -eq 'blocked' -and $s5PureMissingRecord.scenarios[4].result -eq 'blocked') 'S5 pure-missing create terminal was not blocked'
    Assert-True ($s5PureMissingRecord.scenarios[4].reason -match 'platform-marker-missing|fresh-create-terminal-missing|create-terminal') 'S5 pure-missing create terminal reason mismatch'

    $s5NoOpenFixture = New-SimulationFixture
    $s5NoOpenFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" }
    )
    $s5NoOpenPath = Write-JsonFixture 'simulation-adj-s5-no-post-create-open.json' $s5NoOpenFixture
    $s5NoOpenPaths = New-CasePaths 'adj-s5-no-post-create-open'
    $s5NoOpenRun = Invoke-Runner $liveFreezePath $s5NoOpenPaths.Evidence $s5NoOpenPaths.Raw -FixturePath $s5NoOpenPath
    Assert-True ($s5NoOpenRun.ExitCode -eq 0) "S5 no post-create open simulation crashed: $($s5NoOpenRun.Text)"
    $s5NoOpenRecord = Get-Content -LiteralPath (Join-Path $s5NoOpenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5NoOpenRecord.scenarios[4].result -eq 'blocked' -and $s5NoOpenRecord.scenarios[4].reason -eq 'fresh-create-proof-missing') 'S5 without post-create open=true passed'
    Assert-True ([string]$s5NoOpenRecord.scenarios[4].assertions.fresh_create_proof -eq 'blocked') 'S5 fresh_create_proof assertion not blocked without open snapshot'

    $s5OverrideGarbageFixture = New-SimulationFixture
    $s5OverrideGarbageFixture.process_probe_override = [ordered]@{ '5' = @([ordered]@{ pid = 'not-a-status'; dump = 'present' }) }
    $s5OverrideGarbagePath = Write-JsonFixture 'simulation-adj-s5-override-garbage.json' $s5OverrideGarbageFixture
    $s5OverrideGarbagePaths = New-CasePaths 'adj-s5-override-garbage'
    $s5OverrideGarbageRun = Invoke-Runner $liveFreezePath $s5OverrideGarbagePaths.Evidence $s5OverrideGarbagePaths.Raw -FixturePath $s5OverrideGarbagePath
    Assert-True ($s5OverrideGarbageRun.ExitCode -eq 2) "S5 garbage override did not stop: $($s5OverrideGarbageRun.Text)"
    $s5OverrideGarbageRecord = Get-Content -LiteralPath (Join-Path $s5OverrideGarbagePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5OverrideGarbageRecord.overall -eq 'blocked' -and $s5OverrideGarbageRecord.scenarios[4].result -eq 'blocked') 'S5 unknown process_probe_override enum was not blocked'

    $s5OverrideFailureFixture = New-SimulationFixture
    # A would-be absent override is overridden by an injected BundleDump failure on the first S5 probe pair.
    $s5OverrideFailureFixture.process_probe_override = [ordered]@{ '5' = @([ordered]@{ pid = 'absent'; dump = 'present' }) }
    # ADJ-20260808-0003: S2 adds a precise `:vpn` process-target checkpoint, shifting the first
    # S5 force-stop terminal probe BundleDump to occurrence 14.
    $s5OverrideFailureFixture.hdc_failures = @(
        [ordered]@{ operation = 'BundleDump'; occurrence = 14; exit_code = 1; stdout = ''; stderr = 'forced-failure-over-override' }
    )
    $s5OverrideFailurePath = Write-JsonFixture 'simulation-adj-s5-override-failure.json' $s5OverrideFailureFixture
    $s5OverrideFailurePaths = New-CasePaths 'adj-s5-override-failure'
    $s5OverrideFailureRun = Invoke-Runner $liveFreezePath $s5OverrideFailurePaths.Evidence $s5OverrideFailurePaths.Raw -FixturePath $s5OverrideFailurePath
    Assert-True ($s5OverrideFailureRun.ExitCode -eq 2) "S5 override+failure did not stop: $($s5OverrideFailureRun.Text)"
    $s5OverrideFailureRecord = Get-Content -LiteralPath (Join-Path $s5OverrideFailurePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5OverrideFailureRecord.overall -eq 'blocked' -and $s5OverrideFailureRecord.scenarios[4].result -eq 'blocked') 'S5 hdc_failures did not take priority over process_probe_override as blocked'
    Assert-True ($s5OverrideFailureRecord.scenarios[4].reason -match 'force-stop-process-check-unverifiable|probe-unknown-or-error') 'S5 hdc_failures over override reason mismatch'

    Write-Host 'SELFTEST_PHASE=adj-s5-vpn-page-optional-layout-irrelevant'
    # ADJ-20260808-0003: the Settings>VPN page is not captured and never gated, so a wrong page
    # override for it is irrelevant: S5 still passes on the decisive app-info force-stop gate.
    $s5VpnPageFalseFixture = New-SimulationFixture
    $s5VpnPageFalseFixture.layout_profiles['scenario-5-settings-vpn-page'] = 'wrong-page'
    $s5VpnPageFalsePath = Write-JsonFixture 'simulation-adj-s5-vpn-page-optional.json' $s5VpnPageFalseFixture
    $s5VpnPageFalsePaths = New-CasePaths 'adj-s5-vpn-page-optional'
    $s5VpnPageFalseRun = Invoke-Runner $liveFreezePath $s5VpnPageFalsePaths.Evidence $s5VpnPageFalsePaths.Raw -FixturePath $s5VpnPageFalsePath
    Assert-True ($s5VpnPageFalseRun.ExitCode -eq 0) "S5 treated the optional Settings>VPN page as decisive: $($s5VpnPageFalseRun.Text)"
    $s5VpnPageFalseRecord = Get-Content -LiteralPath (Join-Path $s5VpnPageFalsePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5VpnPageFalseRecord.scenarios[4].result -eq 'pass' -and $s5VpnPageFalseRecord.scenarios[4].settings_vpn_page_capture.status -eq 'not-required') 'S5 optional Settings>VPN layout still decided the scenario'

    Write-Host 'SELFTEST_PHASE=adj-s3-s7-request-binding-and-future-events'
    $s3WrongIdFixture = New-SimulationFixture
    $s3WrongIdFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a-wrong" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a-wrong" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a-wrong|trigger=onDestroy" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a-wrong|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a-wrong|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $s3WrongIdPath = Write-JsonFixture 'simulation-adj-s3-wrong-request.json' $s3WrongIdFixture
    $s3WrongIdPaths = New-CasePaths 'adj-s3-wrong-request'
    $s3WrongIdRun = Invoke-Runner $liveFreezePath $s3WrongIdPaths.Evidence $s3WrongIdPaths.Raw -FixturePath $s3WrongIdPath
    Assert-True ($s3WrongIdRun.ExitCode -eq 2) "S3 wrong requestId did not invalidate: $($s3WrongIdRun.Text)"
    $s3WrongIdRecord = Get-Content -LiteralPath (Join-Path $s3WrongIdPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3WrongIdRecord.overall -eq 'invalid' -and $s3WrongIdRecord.scenarios[2].result -eq 'invalid') 'S3 wrong requestId was not protocol invalid'
    Assert-True ($s3WrongIdRecord.scenarios[2].reason -match 'UI_STOP-wrong-requestId|requestId') 'S3 wrong requestId reason mismatch'
    Assert-True ($s3WrongIdRecord.scenarios[3].reason -eq 'not-run-due-to-invalid') 'S3 invalid did not stop later scenarios'

    $s7WrongIdFixture = New-SimulationFixture
    $s7WrongIdFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a-wrong7" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a-wrong7" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a-wrong7|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a-wrong7|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $s7WrongIdPath = Write-JsonFixture 'simulation-adj-s7-wrong-request.json' $s7WrongIdFixture
    $s7WrongIdPaths = New-CasePaths 'adj-s7-wrong-request'
    $s7WrongIdRun = Invoke-Runner $liveFreezePath $s7WrongIdPaths.Evidence $s7WrongIdPaths.Raw -FixturePath $s7WrongIdPath
    Assert-True ($s7WrongIdRun.ExitCode -eq 2) "S7 wrong requestId did not invalidate: $($s7WrongIdRun.Text)"
    $s7WrongIdRecord = Get-Content -LiteralPath (Join-Path $s7WrongIdPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s7WrongIdRecord.overall -eq 'invalid' -and $s7WrongIdRecord.scenarios[6].result -eq 'invalid') 'S7 wrong requestId was not protocol invalid'
    Assert-True ($s7WrongIdRecord.scenarios[6].reason -match 'UI_STOP-wrong-requestId|requestId') 'S7 wrong requestId reason mismatch'
    Assert-True ($s7WrongIdRecord.cleanup_result.verified_absent) 'S7 wrong-id invalid still skipped finally cleanup'

    $s3FutureDestroyFixture = New-SimulationFixture
    $s3FutureDestroyFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 45; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 46; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true" },
        [ordered]@{ offset_seconds = 47; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN" }
    )
    $s3FutureDestroyPath = Write-JsonFixture 'simulation-adj-s3-future-destroy.json' $s3FutureDestroyFixture
    $s3FutureDestroyPaths = New-CasePaths 'adj-s3-future-destroy'
    $s3FutureDestroyRun = Invoke-Runner $liveFreezePath $s3FutureDestroyPaths.Evidence $s3FutureDestroyPaths.Raw -FixturePath $s3FutureDestroyPath
    Assert-True ($s3FutureDestroyRun.ExitCode -eq 0) "S3 future destroy simulation crashed: $($s3FutureDestroyRun.Text)"
    $s3FutureDestroyRecord = Get-Content -LiteralPath (Join-Path $s3FutureDestroyPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s3FutureDestroyRecord.scenarios[2].result -eq 'pass' -and $s3FutureDestroyRecord.scenarios[2].terminal_mode -eq 'strict-process-boundary') 'S3 future destroy did not pass via in-window progressive probes'
    $s3FutureProbes = @($s3FutureDestroyRecord.scenarios[2].process_final_state_probes)
    Assert-True ($s3FutureProbes.Count -ge 2) 'S3 future destroy probes missing'
    $s3ActionPrompt = [DateTimeOffset]::Parse([string]$s3FutureDestroyRecord.scenarios[2].observation.action_prompt_at)
    $s3FirstProbeAt = [DateTimeOffset]::Parse([string]$s3FutureProbes[0].time)
    Assert-True (($s3FirstProbeAt - $s3ActionPrompt).TotalSeconds -ge 45) 'S3 probes armed from future destroy markers before virtual elapsed time'

    Write-Host 'SELFTEST_PHASE=adj-s7-post-uninstall-backfeed-blocked'
    $s7BackfeedFixture = New-SimulationFixture
    $s7BackfeedFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy" }
    )
    $s7BackfeedFixture.process_probe_override = [ordered]@{ '7' = @([ordered]@{ pid = 'present'; dump = 'present' }) }
    $s7BackfeedPath = Write-JsonFixture 'simulation-adj-s7-backfeed.json' $s7BackfeedFixture
    $s7BackfeedPaths = New-CasePaths 'adj-s7-backfeed'
    $s7BackfeedRun = Invoke-Runner $liveFreezePath $s7BackfeedPaths.Evidence $s7BackfeedPaths.Raw -FixturePath $s7BackfeedPath
    Assert-True ($s7BackfeedRun.ExitCode -eq 0) "S7 backfeed simulation crashed: $($s7BackfeedRun.Text)"
    $s7BackfeedRecord = Get-Content -LiteralPath (Join-Path $s7BackfeedPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s7BackfeedRecord.scenarios[6].result -eq 'blocked' -and -not $s7BackfeedRecord.scenarios[6].post_cleanup_capture) 'S7 post-uninstall state backfilled a terminal pass'
    $s7BackfeedProbes = @($s7BackfeedRecord.scenarios[6].process_final_state_probes)
    Assert-True ($s7BackfeedProbes.Count -ge 2 -and @($s7BackfeedProbes | Where-Object { $_.status -ne 'present' }).Count -eq 0) 'S7 pre-uninstall probes were backfilled with post-uninstall absent'
    Assert-True ($s7BackfeedRecord.cleanup_result.verified_absent -eq $true) 'S7 backfeed case finally cleanup was not verified'

    Write-Host 'SELFTEST_PHASE=adj-legacy-freeze-decision-fields-rejected'
    $legacyDecisionFreeze = Copy-JsonObject $liveFreeze
    foreach ($field in @('settings_revoke_mechanism', 'settings_vpn_page_policy', 'destroy_terminal_policy', 'process_absent_required_count', 'process_absent_probe_spacing_seconds')) {
        if ($null -ne $legacyDecisionFreeze.PSObject.Properties[$field]) { $legacyDecisionFreeze.PSObject.Properties.Remove($field) }
    }
    $legacyDecisionPath = Write-JsonFixture 'freeze-legacy-decisions.json' $legacyDecisionFreeze
    $legacyDecisionPaths = New-CasePaths 'legacy-decisions'
    $legacyDecisionRun = Invoke-Runner $legacyDecisionPath $legacyDecisionPaths.Evidence $legacyDecisionPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($legacyDecisionRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $legacyDecisionPaths.Evidence) -and $legacyDecisionRun.Text -match 'settings_revoke_mechanism') 'old freeze without ADJ-20260807-0003 decision fields was accepted for live simulation'
    $legacyDecisionDryPaths = New-CasePaths 'legacy-decisions-dry'
    $legacyDecisionDryRun = Invoke-Runner $legacyDecisionPath $legacyDecisionDryPaths.Evidence $legacyDecisionDryPaths.Raw -AsDryRun
    Assert-True ($legacyDecisionDryRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $legacyDecisionDryPaths.Evidence) -and $legacyDecisionDryRun.Text -match 'settings_revoke_mechanism') 'old freeze dry-run was not explicitly blocked'

    Write-Host 'SELFTEST_PHASE=legacy-spacing-field-rejected'
    $legacySpacingFreeze = Copy-JsonObject $liveFreeze
    $legacySpacingFreeze.PSObject.Properties.Remove('process_absent_probe_spacing_seconds')
    $legacySpacingFreeze | Add-Member -NotePropertyName 'spacing' -NotePropertyValue 3
    $legacySpacingPath = Write-JsonFixture 'freeze-legacy-spacing.json' $legacySpacingFreeze
    $legacySpacingPaths = New-CasePaths 'legacy-spacing'
    $legacySpacingRun = Invoke-Runner $legacySpacingPath $legacySpacingPaths.Evidence $legacySpacingPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($legacySpacingRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $legacySpacingPaths.Evidence) -and $legacySpacingRun.Text -match 'legacy spacing field') 'old freeze with legacy spacing field was accepted or compatibly reused'
    $legacySpacingDryPaths = New-CasePaths 'legacy-spacing-dry'
    $legacySpacingDryRun = Invoke-Runner $legacySpacingPath $legacySpacingDryPaths.Evidence $legacySpacingDryPaths.Raw -AsDryRun
    Assert-True ($legacySpacingDryRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $legacySpacingDryPaths.Evidence) -and $legacySpacingDryRun.Text -match 'legacy spacing field') 'legacy spacing field dry-run was not explicitly rejected'

    Write-Host 'SELFTEST_PHASE=duplicate-lock-no-truncation'
    $transcriptBefore = Get-Sha256 (Join-Path $livePaths.Evidence 'projection\transcript.redacted.jsonl')
    $recordBefore = Get-Sha256 $liveRecordPath
    $duplicate = Invoke-Runner $liveFreezePath $livePaths.Evidence $livePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($duplicate.ExitCode -ne 0) 'duplicate evidence root was accepted'
    Assert-True ((Get-Sha256 (Join-Path $livePaths.Evidence 'projection\transcript.redacted.jsonl')) -eq $transcriptBefore -and (Get-Sha256 $liveRecordPath) -eq $recordBefore) 'duplicate run truncated existing evidence'

    Write-Host 'SELFTEST_PHASE=plan-code-artifact-negative-gates'
    $blockedLivePaths = New-CasePaths 'blocked-live'
    $blockedLive = Invoke-Runner $dryFreezePath $blockedLivePaths.Evidence $blockedLivePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($blockedLive.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $blockedLivePaths.Evidence)) 'LiveSimulation accepted blocked plan'
    $oldApiBuildFreeze = Copy-JsonObject $liveFreeze
    $oldApiBuildFreeze.target_tuple.full_system_build = 'PLA-AL10 6.1.0.117(SP6C00E115R7P7)'
    $oldApiBuildFreeze.target_tuple.api = '23'
    $oldApiBuildPath = Write-JsonFixture 'freeze-old-api23-build.json' $oldApiBuildFreeze
    $oldApiBuildPaths = New-CasePaths 'old-api23-build'
    $oldApiBuild = Invoke-Runner $oldApiBuildPath $oldApiBuildPaths.Evidence $oldApiBuildPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($oldApiBuild.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $oldApiBuildPaths.Evidence)) 'old API23/build freeze was accepted'
    Assert-True ($oldApiBuild.Text -match 'frozen target tuple mismatch') 'old API23/build freeze rejection message missing'
    $oldApiOnlyFreeze = Copy-JsonObject $liveFreeze
    $oldApiOnlyFreeze.target_tuple.api = '23'
    $oldApiOnlyPath = Write-JsonFixture 'freeze-old-api23-only.json' $oldApiOnlyFreeze
    $oldApiOnlyPaths = New-CasePaths 'old-api23-only'
    $oldApiOnly = Invoke-Runner $oldApiOnlyPath $oldApiOnlyPaths.Evidence $oldApiOnlyPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($oldApiOnly.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $oldApiOnlyPaths.Evidence) -and $oldApiOnly.Text -match 'frozen target tuple mismatch: api') 'old API23-only freeze was accepted'
    $oldBuildOnlyFreeze = Copy-JsonObject $liveFreeze
    $oldBuildOnlyFreeze.target_tuple.full_system_build = 'PLA-AL10 6.1.0.117(SP6C00E115R7P7)'
    $oldBuildOnlyPath = Write-JsonFixture 'freeze-old-build-only.json' $oldBuildOnlyFreeze
    $oldBuildOnlyPaths = New-CasePaths 'old-build-only'
    $oldBuildOnly = Invoke-Runner $oldBuildOnlyPath $oldBuildOnlyPaths.Evidence $oldBuildOnlyPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($oldBuildOnly.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $oldBuildOnlyPaths.Evidence) -and $oldBuildOnly.Text -match 'frozen target tuple mismatch: full_system_build') 'old build-only freeze was accepted'
    $badCodeFreeze = Copy-JsonObject $liveFreeze
    $badCodeFreeze.code_sha = ('0' * 40)
    $badCodePath = Write-JsonFixture 'freeze-bad-code.json' $badCodeFreeze
    $badCodePaths = New-CasePaths 'bad-code'
    $badCode = Invoke-Runner $badCodePath $badCodePaths.Evidence $badCodePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($badCode.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $badCodePaths.Evidence)) 'code SHA negative gate failed'
    $badHashFreeze = Copy-JsonObject $liveFreeze
    $badHashFreeze.artifact_sha256.hap_a = ('0' * 64)
    $badHashPath = Write-JsonFixture 'freeze-bad-hash.json' $badHashFreeze
    $badHashPaths = New-CasePaths 'bad-hash'
    $badHash = Invoke-Runner $badHashPath $badHashPaths.Evidence $badHashPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($badHash.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $badHashPaths.Evidence)) 'artifact hash negative gate failed'

    Write-Host 'SELFTEST_PHASE=junction-rejection'
    if ($IsWindows) {
        $junctionTarget = Join-Path $tempRoot 'junction-target'
        $junctionPath = Join-Path $tempRoot 'junction-link'
        [IO.Directory]::CreateDirectory($junctionTarget) | Out-Null
        $junctionCreated = $false
        try {
            $null = New-Item -ItemType Junction -Path $junctionPath -Target $junctionTarget -ErrorAction Stop
            $junctionCreated = $true
        } catch {
            Write-Host "SELFTEST_JUNCTION=skipped reason=$($_.Exception.Message)"
        }
        if ($junctionCreated) {
            $junctionEvidence = Join-Path $junctionPath 'evidence-through-junction'
            $junctionRaw = Join-Path $junctionPath 'raw-through-junction'
            $junctionRun = Invoke-Runner $liveFreezePath $junctionEvidence $junctionRaw -FixturePath $baseFixturePath
            Assert-True ($junctionRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $junctionEvidence)) 'junction ancestor was accepted'
            Write-Host 'SELFTEST_JUNCTION=passed'
        }
    } else {
        Write-Host 'SELFTEST_JUNCTION=skipped non-Windows'
    }

    Write-Host 'SELFTEST_PHASE=preinstall-usb-blocked-and-legal-retry'
    $usbFailure = [ordered]@{ operation = 'TupleModel'; occurrence = 1; exit_code = 1; stderr = 'USB transport offline' }
    $usbFixture = New-SimulationFixture -HdcFailures @($usbFailure)
    $usbFixturePath = Write-JsonFixture 'simulation-usb-fail.json' $usbFixture
    $usbPaths = New-CasePaths 'usb-fail'
    $usbRun = Invoke-Runner $liveFreezePath $usbPaths.Evidence $usbPaths.Raw -FixturePath $usbFixturePath
    Assert-True ($usbRun.ExitCode -ne 0) 'preinstall USB failure did not fail runner'
    $usbRecordPath = Join-Path $usbPaths.Evidence 'scenario-results.json'
    $usbRecord = Get-Content -LiteralPath $usbRecordPath -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($usbRecord.record_status -eq 'blocked' -and $usbRecord.overall -eq 'blocked' -and $usbRecord.infrastructure_reason -eq 'hdc-usb-interruption' -and @($usbRecord.scenarios).Count -eq 7) 'preinstall USB blocked record incomplete'
    Assert-ManifestAndSeal $usbPaths.Evidence
    $retryFreeze = Copy-JsonObject $liveFreeze
    $retryFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0003'
    $retryFreeze.attempt = 'infrastructure-blocked-retry-1'
    $retryFreeze.retry = [ordered]@{
        basis = 'prior blocked record and frozen infrastructure-only retry rule'
        infrastructure_reason = 'hdc-usb-interruption'
        prior_record_path = $usbRecordPath
        prior_record_sha256 = Get-Sha256 $usbRecordPath
    }
    $simulationPriorFreezePath = Write-JsonFixture 'freeze-retry-simulation-prior.json' $retryFreeze
    $simulationPriorPaths = New-CasePaths 'retry-simulation-prior'
    $simulationPriorRun = Invoke-Runner $simulationPriorFreezePath $simulationPriorPaths.Evidence $simulationPriorPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($simulationPriorRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $simulationPriorPaths.Evidence)) 'simulation prior record authorized retry'

    $syntheticLivePrior = Copy-JsonObject $usbRecord
    $syntheticLivePrior.execution_mode = 'live'
    $syntheticLivePrior.simulation = $false
    $syntheticLivePrior.is_evidence = $true
    $syntheticLivePrior.record_status = 'blocked'
    $syntheticLivePrior.overall = 'blocked'
    $syntheticLivePrior.verdict = 'blocked'
    $syntheticLivePriorPath = Write-JsonFixture 'synthetic-live-prior-record.json' $syntheticLivePrior
    $retryFreeze.retry.prior_record_path = $syntheticLivePriorPath
    $retryFreeze.retry.prior_record_sha256 = Get-Sha256 $syntheticLivePriorPath
    $retryFreezePath = Write-JsonFixture 'freeze-retry.json' $retryFreeze
    $retryPaths = New-CasePaths 'retry'
    $retryRun = Invoke-Runner $retryFreezePath $retryPaths.Evidence $retryPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($retryRun.ExitCode -eq 0) "legal infrastructure retry failed: $($retryRun.Text)"
    $retryRecordPath = Join-Path $retryPaths.Evidence 'scenario-results.json'
    $retryRecordText = Get-Content -LiteralPath $retryRecordPath -Raw
    $retryRecord = $retryRecordText | ConvertFrom-Json -Depth 60
    Assert-True ($retryRecord.attempt -eq 'infrastructure-blocked-retry-1' -and $retryRecord.record_status -eq 'blocked' -and $retryRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'legal retry record mismatch'
    Assert-True (-not $retryRecordText.Contains($syntheticLivePriorPath) -and $retryRecord.retry.prior_record_reference -eq 'PRIOR-BLOCKED-RECORD') 'retry record leaked the prior host path'
    Assert-True ([string]$retryRecord.prior_blocked_binding -eq 'N/A') 'retry without prior_blocked_binding freeze field must project N/A'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $retryPaths.Raw 'prior-blocked-record.json') -PathType Leaf)) 'retry must not copy prior blocked record into RawRoot'

    Write-Host 'SELFTEST_PHASE=prior-blocked-binding-projection'
    $priorScenarioSha = 'a' * 64
    $priorManifestSha = 'b' * 64
    $priorSealSha = 'c' * 64
    $bindingFreeze = Copy-JsonObject $liveFreeze
    $bindingFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0010'
    $bindingFreeze.prior_blocked_binding = [ordered]@{
        source = 'consumed-blocked'
        evidence_id = 'EV-E3-SELFTEST-20990101-0009'
        scenario_results_sha256 = $priorScenarioSha
        hash_manifest_sha256 = $priorManifestSha
        campaign_seal_sha256 = $priorSealSha
    }
    $bindingFreezePath = Write-JsonFixture 'freeze-prior-binding.json' $bindingFreeze
    $bindingPaths = New-CasePaths 'prior-binding'
    $bindingRun = Invoke-Runner $bindingFreezePath $bindingPaths.Evidence $bindingPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($bindingRun.ExitCode -eq 0) "prior blocked binding simulation failed: $($bindingRun.Text)"
    $bindingRecordPath = Join-Path $bindingPaths.Evidence 'scenario-results.json'
    $bindingRecord = Get-Content -LiteralPath $bindingRecordPath -Raw | ConvertFrom-Json -Depth 60
    Assert-True ([string]$bindingRecord.prior_blocked_binding.source -eq 'consumed-blocked') 'record prior_blocked_binding.source mismatch'
    Assert-True ([string]$bindingRecord.prior_blocked_binding.evidence_id -eq 'EV-E3-SELFTEST-20990101-0009') 'record did not project prior_blocked_binding.evidence_id'
    Assert-True ([string]$bindingRecord.prior_blocked_binding.scenario_results_sha256 -eq $priorScenarioSha) 'record prior_blocked_binding.scenario_results_sha256 mismatch'
    Assert-True ([string]$bindingRecord.prior_blocked_binding.hash_manifest_sha256 -eq $priorManifestSha) 'record prior_blocked_binding.hash_manifest_sha256 mismatch'
    Assert-True ([string]$bindingRecord.prior_blocked_binding.campaign_seal_sha256 -eq $priorSealSha) 'record prior_blocked_binding.campaign_seal_sha256 mismatch'
    Assert-True ([string]$bindingRecord.prior_blocked_binding.binding_source -eq 'freeze-manifest') 'record prior_blocked_binding.binding_source must be freeze-manifest'
    Assert-True ($null -eq $bindingRecord.prior_blocked_binding.PSObject.Properties['verified'] -and $null -eq $bindingRecord.prior_blocked_binding.PSObject.Properties['reverified'] -and $null -eq $bindingRecord.prior_blocked_binding.PSObject.Properties['record_path']) 'prior binding must not declare re-verification or host paths'
    Assert-True (-not (Test-Path -LiteralPath (Join-Path $bindingPaths.Raw 'prior-blocked-record.json') -PathType Leaf)) 'prior binding must not copy raw prior record'
    $bindingManifest = Get-Content -LiteralPath (Join-Path $bindingPaths.Evidence 'hash-manifest.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True (@($bindingManifest.external_raw_files | Where-Object { [string]$_.reference -match 'PRIOR-BLOCKED' }).Count -eq 0) 'manifest must not seal prior raw copies'
    $bindingRecordSha = Get-Sha256 $bindingRecordPath
    $bindingManifestSha = Get-Sha256 (Join-Path $bindingPaths.Evidence 'hash-manifest.json')
    $bindingSeal = Get-Content -LiteralPath (Join-Path $bindingPaths.Evidence 'campaign-seal.json') -Raw | ConvertFrom-Json -Depth 20
    Assert-True ([string]$bindingSeal.record.sha256 -eq $bindingRecordSha) 'campaign seal must bind projected scenario-results containing prior_blocked_binding'
    Assert-True ([string]$bindingSeal.manifest.sha256 -eq $bindingManifestSha) 'campaign seal must bind final manifest'
    Assert-ManifestAndSeal $bindingPaths.Evidence
    Assert-True (@($bindingRecord.integrity_violations).Count -eq 0) 'prior binding run must pass integrity'

    $bindingHashFlipFreeze = Copy-JsonObject $bindingFreeze
    $bindingHashFlipFreeze.prior_blocked_binding.scenario_results_sha256 = 'd' * 64
    $bindingHashFlipFreezePath = Write-JsonFixture 'freeze-prior-binding-hash-flip.json' $bindingHashFlipFreeze
    $bindingHashFlipPaths = New-CasePaths 'prior-binding-hash-flip'
    $bindingHashFlipRun = Invoke-Runner $bindingHashFlipFreezePath $bindingHashFlipPaths.Evidence $bindingHashFlipPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($bindingHashFlipRun.ExitCode -eq 0) "hash-flip prior binding simulation failed: $($bindingHashFlipRun.Text)"
    $flipRecordPath = Join-Path $bindingHashFlipPaths.Evidence 'scenario-results.json'
    $flipRecordSha = Get-Sha256 $flipRecordPath
    $flipSeal = Get-Content -LiteralPath (Join-Path $bindingHashFlipPaths.Evidence 'campaign-seal.json') -Raw | ConvertFrom-Json -Depth 20
    Assert-True ($flipRecordSha -ne $bindingRecordSha) 'changing a projected prior hash must change scenario-results sha256'
    Assert-True ([string]$flipSeal.record.sha256 -ne [string]$bindingSeal.record.sha256) 'changing a projected prior hash must change campaign seal record binding'

    Write-Host 'SELFTEST_PHASE=prior-blocked-binding-negatives-and-legacy'
    $badHashFreeze = Copy-JsonObject $bindingFreeze
    $badHashFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0011'
    $badHashFreeze.prior_blocked_binding.scenario_results_sha256 = 'not-a-sha'
    $badHashFreezePath = Write-JsonFixture 'freeze-prior-binding-bad-hash.json' $badHashFreeze
    $badHashPaths = New-CasePaths 'prior-binding-bad-hash'
    $badHashRun = Invoke-Runner $badHashFreezePath $badHashPaths.Evidence $badHashPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($badHashRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $badHashPaths.Evidence)) 'bad prior hash was accepted'
    Assert-True ($badHashRun.Text -match 'scenario_results_sha256|final SHA-256') 'bad prior hash rejection message missing'

    $incompleteFreeze = Copy-JsonObject $liveFreeze
    $incompleteFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0013'
    $incompleteFreeze.prior_blocked_binding = [ordered]@{
        source = 'consumed-blocked'
        evidence_id = 'EV-E3-SELFTEST-20990101-0009'
        scenario_results_sha256 = $priorScenarioSha
    }
    $incompleteFreezePath = Write-JsonFixture 'freeze-prior-binding-incomplete.json' $incompleteFreeze
    $incompletePaths = New-CasePaths 'prior-binding-incomplete'
    $incompleteRun = Invoke-Runner $incompleteFreezePath $incompletePaths.Evidence $incompletePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($incompleteRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $incompletePaths.Evidence)) 'incomplete prior binding object was accepted'
    Assert-True ($incompleteRun.Text -match 'hash_manifest_sha256|campaign_seal_sha256|final SHA-256') 'incomplete prior binding rejection message missing'

    $badSourceFreeze = Copy-JsonObject $bindingFreeze
    $badSourceFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0015'
    $badSourceFreeze.prior_blocked_binding.source = 'retry'
    $badSourceFreezePath = Write-JsonFixture 'freeze-prior-binding-bad-source.json' $badSourceFreeze
    $badSourcePaths = New-CasePaths 'prior-binding-bad-source'
    $badSourceRun = Invoke-Runner $badSourceFreezePath $badSourcePaths.Evidence $badSourcePaths.Raw -FixturePath $baseFixturePath
    Assert-True ($badSourceRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $badSourcePaths.Evidence)) 'non-consumed-blocked source was accepted'

    $legacyFreeze = Copy-JsonObject $liveFreeze
    $legacyFreeze.evidence_id = 'EV-E3-SELFTEST-20990101-0014'
    if ($null -ne $legacyFreeze.PSObject.Properties['prior_blocked_binding']) {
        $legacyFreeze.PSObject.Properties.Remove('prior_blocked_binding')
    }
    $legacyFreezePath = Write-JsonFixture 'freeze-legacy-no-prior-binding.json' $legacyFreeze
    $legacyPaths = New-CasePaths 'legacy-no-prior-binding'
    $legacyRun = Invoke-Runner $legacyFreezePath $legacyPaths.Evidence $legacyPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($legacyRun.ExitCode -eq 0) "legacy freeze without prior_blocked_binding failed: $($legacyRun.Text)"
    $legacyRecord = Get-Content -LiteralPath (Join-Path $legacyPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ([string]$legacyRecord.prior_blocked_binding -eq 'N/A') 'legacy freeze must project prior_blocked_binding N/A'

    Write-Host 'SELFTEST_PHASE=finally-installation-flags'
    $installBFailure = [ordered]@{ operation = 'InstallB'; occurrence = 1; exit_code = 1; stderr = 'signature rejected' }
    $installBFixturePath = Write-JsonFixture 'simulation-install-b-fail.json' (New-SimulationFixture -HdcFailures @($installBFailure))
    $installBPaths = New-CasePaths 'install-b-fail'
    $installBRun = Invoke-Runner $liveFreezePath $installBPaths.Evidence $installBPaths.Raw -FixturePath $installBFixturePath
    Assert-True ($installBRun.ExitCode -ne 0) 'InstallB failure did not fail runner'
    $installBRecord = Get-Content -LiteralPath (Join-Path $installBPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    $finallyUninstalls = @($installBRecord.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-uninstall' })
    Assert-True ($finallyUninstalls.Count -eq 1 -and $finallyUninstalls[0].bundle -eq 'cn.alfadb.netbird.e3physvpna') 'finally did not honor InstalledA/InstalledB flags'
    Assert-True (@($installBRecord.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-remove-staging' }).Count -eq 1) 'finally did not honor StagingSent flag'

    $installAFailure = [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 1; stderr = 'signature rejected' }
    $installAFixturePath = Write-JsonFixture 'simulation-install-a-fail.json' (New-SimulationFixture -HdcFailures @($installAFailure))
    $installAPaths = New-CasePaths 'install-a-fail'
    $installARun = Invoke-Runner $liveFreezePath $installAPaths.Evidence $installAPaths.Raw -FixturePath $installAFixturePath
    $installARecord = Get-Content -LiteralPath (Join-Path $installAPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True (@($installARecord.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-uninstall' }).Count -eq 0) 'finally uninstalled a bundle whose install never succeeded'

    Write-Host 'SELFTEST_PHASE=capture-degraded-blocks-without-crash'
    $faultFailure = [ordered]@{ operation = 'FaultA'; occurrence = 1; exit_code = 127; stderr = 'unknown command' }
    $faultFixturePath = Write-JsonFixture 'simulation-fault-degraded.json' (New-SimulationFixture -HdcFailures @($faultFailure))
    $faultPaths = New-CasePaths 'fault-degraded'
    $faultRun = Invoke-Runner $liveFreezePath $faultPaths.Evidence $faultPaths.Raw -FixturePath $faultFixturePath
    Assert-True ($faultRun.ExitCode -eq 0) "unknown fault capture crashed campaign: $($faultRun.Text)"
    $faultRecord = Get-Content -LiteralPath (Join-Path $faultPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($faultRecord.scenarios[6].result -eq 'blocked' -and $faultRecord.scenarios[6].fault_capture_degraded) 'unknown fault capture did not degrade to blocked'
    Assert-True (@($faultRecord.fault_reference.artifacts | Where-Object { $_.operation -eq 'FaultA' -and $_.status -eq 'degraded' }).Count -eq 1) 'failed fault artifact was not hashed and referenced as degraded'
    Assert-True ([bool]$faultRecord.scenarios[6].observation.complete_window_observed) 'fault failure shortened the 60-second observation window'
    Assert-True ($null -eq $faultRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$faultRecord.infrastructure_reason)) 'unsupported fault was treated as infrastructure'
    $faultDegradedEntries = @($faultRecord.capture_degraded | Where-Object { $_.component -eq 'FaultA' })
    Assert-True ($faultDegradedEntries.Count -ge 1 -and [string]$faultDegradedEntries[0].category -eq 'non-infrastructure') 'fault CaptureDegraded entry missing non-infrastructure category'

    $faultPermission = [ordered]@{ operation = 'FaultB'; occurrence = 1; exit_code = 1; stderr = 'Permission denied' }
    $faultPermissionPath = Write-JsonFixture 'simulation-fault-permission.json' (New-SimulationFixture -HdcFailures @($faultPermission))
    $faultPermissionPaths = New-CasePaths 'fault-permission'
    $faultPermissionRun = Invoke-Runner $liveFreezePath $faultPermissionPaths.Evidence $faultPermissionPaths.Raw -FixturePath $faultPermissionPath
    Assert-True ($faultPermissionRun.ExitCode -eq 0) "permission fault capture crashed campaign: $($faultPermissionRun.Text)"
    $faultPermissionRecord = Get-Content -LiteralPath (Join-Path $faultPermissionPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($faultPermissionRecord.scenarios[6].result -eq 'blocked' -and $faultPermissionRecord.scenarios[6].fault_capture_degraded) 'permission fault did not block scenario 7'
    Assert-True ([bool]$faultPermissionRecord.scenarios[6].observation.complete_window_observed) 'permission fault shortened the 60-second observation window'
    Assert-True ($null -eq $faultPermissionRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$faultPermissionRecord.infrastructure_reason)) 'permission fault was treated as infrastructure'

    Write-Host 'SELFTEST_PHASE=capture-death-late-create-confirmation-and-timeout'
    $deadFixturePath = Write-JsonFixture 'simulation-capture-dead.json' (New-SimulationFixture -CaptureDieScenario 4)
    $deadPaths = New-CasePaths 'capture-dead'
    $deadRun = Invoke-Runner $liveFreezePath $deadPaths.Evidence $deadPaths.Raw -FixturePath $deadFixturePath
    Assert-True ($deadRun.ExitCode -ne 0) 'capture death did not stop later scenario execution'
    $deadRecord = Get-Content -LiteralPath (Join-Path $deadPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    $s4Dead = $deadRecord.scenarios[3]
    $s4DeadBlocked = [string]$s4Dead.result -eq 'blocked' -and $null -ne $s4Dead.PSObject.Properties['observation'] -and [bool]$s4Dead.observation.capture_degraded -and -not [bool]$s4Dead.full_window_after_action
    $s4DeadStopped = [string]$s4Dead.result -in @('blocked', 'invalid') -and [string]$s4Dead.result -ne 'pass' -and [string]$s4Dead.result -ne 'fail'
    Assert-True ($s4DeadBlocked -or $s4DeadStopped) 'capture death allowed deny to fail open'
    Assert-True ($deadRecord.infrastructure_reason -eq 'hdc-usb-interruption' -or $deadRecord.overall -in @('blocked', 'invalid')) 'capture death did not set hdc-usb-interruption infrastructure reason or stop overall'
    Assert-True ($null -eq $deadRecord.scenarios[4].PSObject.Properties['observation'] -or [string]$deadRecord.scenarios[4].reason -match 'not-run|capture|invalid') 'campaign continued into scenario 5 after capture death'

    $installExit0Fail = [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 0; stdout = 'error: failed to execute your command.'; stderr = '' }
    $installExit0Path = Write-JsonFixture 'simulation-install-exit0-fail.json' (New-SimulationFixture -HdcFailures @($installExit0Fail))
    $installExit0Paths = New-CasePaths 'install-exit0-fail'
    $installExit0Run = Invoke-Runner $liveFreezePath $installExit0Paths.Evidence $installExit0Paths.Raw -FixturePath $installExit0Path
    Assert-True ($installExit0Run.ExitCode -ne 0) 'exit0 install semantic failure did not fail runner'
    $installExit0Record = Get-Content -LiteralPath (Join-Path $installExit0Paths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($installExit0Record.actual -match 'FUNCTIONAL_FAIL' -and @($installExit0Record.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-uninstall' }).Count -eq 0) 'exit0 install semantic failure was not FUNCTIONAL_FAIL or incorrectly marked installed'

    $installDumpAbsent = @(
        [ordered]@{ operation = 'BundleDump'; occurrence = 3; exit_code = 0; stdout = 'error: failed to get information and the parameters may be wrong.'; stderr = '' }
    )
    $installDumpAbsentPath = Write-JsonFixture 'simulation-install-dump-absent.json' (New-SimulationFixture -HdcFailures $installDumpAbsent)
    $installDumpAbsentPaths = New-CasePaths 'install-dump-absent'
    $installDumpAbsentRun = Invoke-Runner $liveFreezePath $installDumpAbsentPaths.Evidence $installDumpAbsentPaths.Raw -FixturePath $installDumpAbsentPath
    Assert-True ($installDumpAbsentRun.ExitCode -ne 0) 'install dump-absent did not fail runner'
    $installDumpAbsentRecord = Get-Content -LiteralPath (Join-Path $installDumpAbsentPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($installDumpAbsentRecord.actual -match 'install confirmation blocked' -and $installDumpAbsentRecord.actual -match 'bundle-dump-absent' -and $installDumpAbsentRecord.actual -notmatch 'FUNCTIONAL_FAIL' -and @($installDumpAbsentRecord.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-uninstall' }).Count -eq 0) 'install dump-absent was not non-infrastructure blocked without InstalledA'
    Assert-True ($null -eq $installDumpAbsentRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$installDumpAbsentRecord.infrastructure_reason)) 'install dump-absent authorized infrastructure retry'

    $installDumpPermission = @(
        [ordered]@{ operation = 'BundleDump'; occurrence = 3; exit_code = 1; stdout = ''; stderr = 'Permission denied' }
    )
    $installDumpPermissionPath = Write-JsonFixture 'simulation-install-dump-permission.json' (New-SimulationFixture -HdcFailures $installDumpPermission)
    $installDumpPermissionPaths = New-CasePaths 'install-dump-permission'
    $installDumpPermissionRun = Invoke-Runner $liveFreezePath $installDumpPermissionPaths.Evidence $installDumpPermissionPaths.Raw -FixturePath $installDumpPermissionPath
    Assert-True ($installDumpPermissionRun.ExitCode -ne 0) 'install dump-permission did not stop runner'
    $installDumpPermissionRecord = Get-Content -LiteralPath (Join-Path $installDumpPermissionPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($installDumpPermissionRecord.actual -match 'install confirmation blocked' -and $installDumpPermissionRecord.actual -notmatch 'FUNCTIONAL_FAIL' -and ($null -eq $installDumpPermissionRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$installDumpPermissionRecord.infrastructure_reason))) 'install dump-permission was not non-infrastructure blocked without retry'

    $installWarningUncertain = [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 0; stdout = 'warning: cache rebuild skipped'; stderr = '' }
    $installWarningPath = Write-JsonFixture 'simulation-install-warning.json' (New-SimulationFixture -HdcFailures @($installWarningUncertain))
    $installWarningPaths = New-CasePaths 'install-warning'
    $installWarningRun = Invoke-Runner $liveFreezePath $installWarningPaths.Evidence $installWarningPaths.Raw -FixturePath $installWarningPath
    Assert-True ($installWarningRun.ExitCode -ne 0) 'install warning-without-success did not stop runner'
    $installWarningRecord = Get-Content -LiteralPath (Join-Path $installWarningPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($installWarningRecord.actual -match 'install outcome blocked' -and $installWarningRecord.actual -notmatch 'FUNCTIONAL_FAIL' -and ($null -eq $installWarningRecord.PSObject.Properties['infrastructure_reason'] -or [string]::IsNullOrEmpty([string]$installWarningRecord.infrastructure_reason))) 'install warning-without-success was not non-infrastructure blocked without retry'

    $stagingResidual = @(
        [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 1; stderr = 'signature rejected' },
        [ordered]@{ operation = 'StagingProbe'; occurrence = 2; exit_code = 0; stdout = 'drwxrwxrwx 3 shell shell 4096 2026-01-01 00:00 /data/local/tmp/e3-phys-preflight'; stderr = '' }
    )
    $stagingResidualPath = Write-JsonFixture 'simulation-staging-residual.json' (New-SimulationFixture -HdcFailures $stagingResidual)
    $stagingResidualPaths = New-CasePaths 'staging-residual'
    $stagingResidualRun = Invoke-Runner $liveFreezePath $stagingResidualPaths.Evidence $stagingResidualPaths.Raw -FixturePath $stagingResidualPath
    Assert-True ($stagingResidualRun.ExitCode -ne 0) 'staging residual simulation did not fail runner'
    $stagingResidualRecord = Get-Content -LiteralPath (Join-Path $stagingResidualPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($stagingResidualRecord.cleanup_result.status -eq 'blocked-unknown-residual' -and $stagingResidualRecord.cleanup_result.staging_sent_remaining -and -not $stagingResidualRecord.cleanup_result.verified_absent) 'exit0 rm with residual staging was declared clean'

    $stagingCannotAccess = @(
        [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 1; stderr = 'signature rejected' },
        [ordered]@{ operation = 'StagingProbe'; occurrence = 2; exit_code = 1; stdout = ''; stderr = "ls: cannot access '/data/local/tmp/e3-phys-preflight': Permission denied" }
    )
    $stagingCannotAccessPath = Write-JsonFixture 'simulation-staging-cannot-access.json' (New-SimulationFixture -HdcFailures $stagingCannotAccess)
    $stagingCannotAccessPaths = New-CasePaths 'staging-cannot-access'
    $stagingCannotAccessRun = Invoke-Runner $liveFreezePath $stagingCannotAccessPaths.Evidence $stagingCannotAccessPaths.Raw -FixturePath $stagingCannotAccessPath
    Assert-True ($stagingCannotAccessRun.ExitCode -ne 0) 'staging cannot-access simulation did not fail runner'
    $stagingCannotAccessRecord = Get-Content -LiteralPath (Join-Path $stagingCannotAccessPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($stagingCannotAccessRecord.cleanup_result.status -eq 'blocked-unknown-residual' -and -not $stagingCannotAccessRecord.cleanup_result.verified_absent) 'cannot-access staging probe was treated as absent/clean'

    $mkdirFail = [ordered]@{ operation = 'MkdirStaging'; occurrence = 1; exit_code = 1; stderr = 'mkdir failed' }
    $mkdirFailPath = Write-JsonFixture 'simulation-mkdir-fail.json' (New-SimulationFixture -HdcFailures @($mkdirFail))
    $mkdirFailPaths = New-CasePaths 'mkdir-fail'
    $mkdirFailRun = Invoke-Runner $liveFreezePath $mkdirFailPaths.Evidence $mkdirFailPaths.Raw -FixturePath $mkdirFailPath
    Assert-True ($mkdirFailRun.ExitCode -ne 0) 'mkdir failure did not stop runner'
    $mkdirFailRecord = Get-Content -LiteralPath (Join-Path $mkdirFailPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True (@($mkdirFailRecord.cleanup_result.actions | Where-Object { $_.operation -eq 'finally-remove-staging' }).Count -eq 1) 'mkdir failure did not attempt fixed staging cleanup in finally'

    $authCaptureFailPath = Write-JsonFixture 'simulation-auth-capture-fail.json' (New-SimulationFixture -CaptureFailures @('scenario-2-authorization'))
    $authCaptureFailPaths = New-CasePaths 'auth-capture-fail'
    $authCaptureFailRun = Invoke-Runner $liveFreezePath $authCaptureFailPaths.Evidence $authCaptureFailPaths.Raw -FixturePath $authCaptureFailPath
    Assert-True ($authCaptureFailRun.ExitCode -eq 2) "authorization capture failure did not invalidate: $($authCaptureFailRun.Text)"
    $authCaptureFailRecord = Get-Content -LiteralPath (Join-Path $authCaptureFailPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($authCaptureFailRecord.overall -eq 'invalid' -and $authCaptureFailRecord.scenarios[1].result -eq 'invalid' -and $authCaptureFailRecord.scenarios[1].reason -match 'scenario-2-authorization-capture-not-collected') 'authorization capture failure did not invalidate scenario 2 at the machine gate'
    Assert-True ($authCaptureFailRecord.scenarios[2].reason -eq 'not-run-due-to-invalid' -and $authCaptureFailRecord.cleanup_result.verified_absent) 'authorization capture invalid did not stop later scenarios or skip cleanup'

    $lateFixture = New-SimulationFixture
    $lateFixture.scenario_events.'4' += [ordered]@{ offset_seconds = 50; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    $lateFixturePath = Write-JsonFixture 'simulation-late-b-create.json' $lateFixture
    $latePaths = New-CasePaths 'late-b-create'
    $lateRun = Invoke-Runner $liveFreezePath $latePaths.Evidence $latePaths.Raw -FixturePath $lateFixturePath
    Assert-True ($lateRun.ExitCode -eq 2) "late B create simulation did not invalidate: $($lateRun.Text)"
    $lateRecord = Get-Content -LiteralPath (Join-Path $latePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($lateRecord.scenarios[3].result -eq 'invalid' -and $lateRecord.scenarios[3].reason -eq 'deny-action-produced-create-untrusted') 'trailing B create was not caught inside the measured window'

    $multiReqFixture = New-SimulationFixture
    $multiReqFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-primary" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-secondary" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-secondary" }
    )
    $multiReqPath = Write-JsonFixture 'simulation-multi-b-requestid.json' $multiReqFixture
    $multiReqPaths = New-CasePaths 'multi-b-requestid'
    $multiReqRun = Invoke-Runner $liveFreezePath $multiReqPaths.Evidence $multiReqPaths.Raw -FixturePath $multiReqPath
    Assert-True ($multiReqRun.ExitCode -eq 2) "multi B requestId did not invalidate: $($multiReqRun.Text)"
    $multiReqRecord = Get-Content -LiteralPath (Join-Path $multiReqPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($multiReqRecord.overall -eq 'invalid' -and $multiReqRecord.scenarios[3].result -eq 'invalid') 'secondary B requestId create was not protocol invalid'
    Assert-True ($multiReqRecord.scenarios[3].reason -match 'UI_START|expected-one') 'multi B requestId reason mismatch'

    $bmDumpFixture = New-SimulationFixture
    $bmJson = '{"udid":"DEVICE-UDID-SHOULD-REDACT","deviceIds":["ID-1"],"endpoint":"192.0.2.55:8710"}'
    $bmDumpFixture.scenario_events.'2' += [ordered]@{ offset_seconds = 6; text = ("<DEVICE_OBSERVED_AT> BM_DUMP_JSON " + $bmJson) }
    $bmDumpPath = Write-JsonFixture 'simulation-bm-dump-json.json' $bmDumpFixture
    $bmDumpPaths = New-CasePaths 'bm-dump-json'
    $bmDumpRun = Invoke-Runner $liveFreezePath $bmDumpPaths.Evidence $bmDumpPaths.Raw -FixturePath $bmDumpPath
    Assert-True ($bmDumpRun.ExitCode -eq 0) "bm dump JSON simulation crashed: $($bmDumpRun.Text)"
    $bmDumpEvidenceText = (Get-ChildItem -LiteralPath $bmDumpPaths.Evidence -File -Recurse | ForEach-Object { Get-Content -LiteralPath $_.FullName -Raw }) -join "`n"
    Assert-True ($bmDumpEvidenceText -notmatch 'DEVICE-UDID-SHOULD-REDACT|192\.0\.2\.55') 'bm dump JSON values were not redacted in projected evidence'

    $missingConfirmationFixture = New-SimulationFixture
    $missingConfirmationFixture.operator['confirmations'] = [ordered]@{ 'FINAL-CLEANUP-CAPTURED' = $false }
    $missingConfirmationPath = Write-JsonFixture 'simulation-missing-confirmation.json' $missingConfirmationFixture
    $missingConfirmationPaths = New-CasePaths 'missing-confirmation'
    $missingConfirmationRun = Invoke-Runner $liveFreezePath $missingConfirmationPaths.Evidence $missingConfirmationPaths.Raw -FixturePath $missingConfirmationPath
    Assert-True ($missingConfirmationRun.ExitCode -eq 0) "legacy FINAL-CLEANUP confirmation object crashed: $($missingConfirmationRun.Text)"
    $missingConfirmationRecord = Get-Content -LiteralPath (Join-Path $missingConfirmationPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($missingConfirmationRecord.scenarios[6].result -eq 'pass' -and $null -eq $missingConfirmationRecord.scenarios[6].PSObject.Properties['visible_cleanup_confirmed']) 'S7 still depended on FINAL-CLEANUP operator confirmation'

    $strictBooleanFixture = New-SimulationFixture
    $strictBooleanFixture.operator['confirmations'] = [ordered]@{ 'DENY-SCREEN-CAPTURED' = 'true' }
    $strictBooleanPath = Write-JsonFixture 'simulation-strict-boolean.json' $strictBooleanFixture
    $strictBooleanPaths = New-CasePaths 'strict-boolean'
    $strictBooleanRun = Invoke-Runner $liveFreezePath $strictBooleanPaths.Evidence $strictBooleanPaths.Raw -FixturePath $strictBooleanPath
    Assert-True ($strictBooleanRun.ExitCode -eq 0) "legacy string confirmation object crashed the strong protocol: $($strictBooleanRun.Text)"
    $strictBooleanRecord = Get-Content -LiteralPath (Join-Path $strictBooleanPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($strictBooleanRecord.scenarios[3].result -eq 'pass' -and $strictBooleanRecord.record_status -eq 'blocked' -and -not $strictBooleanRecord.is_evidence) 'legacy string confirmation was still treated as a semantic gate'

    Write-Host 'SELFTEST_PHASE=settings-reallow-path-observation-only'
    $pathMismatchFixture = New-SimulationFixture
    $pathMismatchFixture.layout_profiles['scenario-5-reactivation'] = 'authorization'
    $pathMismatchFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 9; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $pathMismatchPath = Write-JsonFixture 'simulation-path-mismatch.json' $pathMismatchFixture
    $pathMismatchPaths = New-CasePaths 'path-mismatch-pass'
    $pathMismatchRun = Invoke-Runner $liveFreezePath $pathMismatchPaths.Evidence $pathMismatchPaths.Raw -FixturePath $pathMismatchPath
    Assert-True ($pathMismatchRun.ExitCode -eq 0) "path mismatch with complete functional chain crashed: $($pathMismatchRun.Text)"
    $pathMismatchRecord = Get-Content -LiteralPath (Join-Path $pathMismatchPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($pathMismatchRecord.scenarios[4].result -eq 'pass') 'predicted-direct actual-reauth with complete functional chain did not pass'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.match -eq $false) 'path mismatch was not recorded as match=false'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.expected -eq 'direct-system-activation' -and $pathMismatchRecord.scenarios[4].settings_reallow_path.actual -eq 'system-reauthorization-UI') 'path observation expected/actual fields wrong'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.observation -eq 'machine-layout-and-event-classified' -and $pathMismatchRecord.scenarios[4].settings_reallow_path.policy -eq 'observation-only') 'path observation metadata wrong'
    Assert-True ($pathMismatchRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'path mismatch alone blocked overall aggregation'

    # ADJ-20260808-0003 (C6): this is the pure-missing case — a verified Start with NO create
    # marker and NO extra UI action while waiting for the platform create terminal. It is a plain
    # runner blocked (platform-marker-missing:fresh-create-terminal-missing), never a scenario
    # invalid. Only an extra operator UI action (e.g. the UI_STOP_SKIPPED in adj-s5-no-fresh-create)
    # turns the wait into a scenario invalid.
    $missingFunctionalFixture = New-SimulationFixture
    $missingFunctionalFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $missingFunctionalPath = Write-JsonFixture 'simulation-missing-functional.json' $missingFunctionalFixture
    $missingFunctionalPaths = New-CasePaths 'missing-functional-marker'
    $missingFunctionalRun = Invoke-Runner $liveFreezePath $missingFunctionalPaths.Evidence $missingFunctionalPaths.Raw -FixturePath $missingFunctionalPath
    Assert-True ($missingFunctionalRun.ExitCode -eq 2) "missing functional marker did not stop as blocked: $($missingFunctionalRun.Text)"
    $missingFunctionalRecord = Get-Content -LiteralPath (Join-Path $missingFunctionalPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($missingFunctionalRecord.overall -eq 'blocked' -and $missingFunctionalRecord.scenarios[4].result -eq 'blocked') 'missing VPN_ONCREATE/create-fd markers did not stay blocked'
    Assert-True ($missingFunctionalRecord.scenarios[4].reason -match 'platform-marker-missing|fresh-create-terminal-missing|create-terminal') 'missing functional markers reason mismatch'

    $badPolicyFreeze = Copy-JsonObject $liveFreeze
    $badPolicyFreeze.settings_reallow_path_policy = 'strict-equal'
    $badPolicyPath = Write-JsonFixture 'freeze-bad-path-policy.json' $badPolicyFreeze
    $badPolicyPaths = New-CasePaths 'bad-path-policy'
    $badPolicyRun = Invoke-Runner $badPolicyPath $badPolicyPaths.Evidence $badPolicyPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($badPolicyRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $badPolicyPaths.Evidence)) 'non-observation-only path policy was accepted'
    Assert-True ($badPolicyRun.Text -match 'settings_reallow_path_policy must be observation-only') 'bad path policy rejection message missing'

    $missingPolicyFreeze = Copy-JsonObject $liveFreeze
    $missingPolicyFreeze.PSObject.Properties.Remove('settings_reallow_path_policy')
    $missingPolicyPath = Write-JsonFixture 'freeze-missing-path-policy.json' $missingPolicyFreeze
    $missingPolicyPaths = New-CasePaths 'missing-path-policy'
    $missingPolicyRun = Invoke-Runner $missingPolicyPath $missingPolicyPaths.Evidence $missingPolicyPaths.Raw -FixturePath $baseFixturePath
    Assert-True ($missingPolicyRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $missingPolicyPaths.Evidence)) 'missing path policy was accepted'

    $installTimeout = [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 124; stderr = 'operation timeout' }
    $installTimeoutPath = Write-JsonFixture 'simulation-install-timeout.json' (New-SimulationFixture -HdcFailures @($installTimeout))
    $installTimeoutPaths = New-CasePaths 'install-timeout'
    $installTimeoutRun = Invoke-Runner $liveFreezePath $installTimeoutPaths.Evidence $installTimeoutPaths.Raw -FixturePath $installTimeoutPath
    Assert-True ($installTimeoutRun.ExitCode -ne 0) 'install timeout did not stop the campaign'
    $installTimeoutRecord = Get-Content -LiteralPath (Join-Path $installTimeoutPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($installTimeoutRecord.infrastructure_reason -eq 'hdc-usb-interruption' -and $installTimeoutRecord.overall -eq 'blocked' -and $installTimeoutRecord.record_status -eq 'blocked') 'install timeout was classified as a functional failure'

    $cleanupUnknownFailures = @(
        [ordered]@{ operation = 'InstallA'; occurrence = 1; exit_code = 1; stderr = 'signature rejected' },
        [ordered]@{ operation = 'BundleDump'; occurrence = 3; exit_code = 127; stderr = 'unknown query state' }
    )
    $cleanupUnknownPath = Write-JsonFixture 'simulation-cleanup-unknown.json' (New-SimulationFixture -HdcFailures $cleanupUnknownFailures)
    $cleanupUnknownPaths = New-CasePaths 'cleanup-unknown'
    $cleanupUnknownRun = Invoke-Runner $liveFreezePath $cleanupUnknownPaths.Evidence $cleanupUnknownPaths.Raw -FixturePath $cleanupUnknownPath
    $cleanupUnknownRecord = Get-Content -LiteralPath (Join-Path $cleanupUnknownPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($cleanupUnknownRecord.cleanup_result.status -eq 'blocked-unknown-residual' -and -not $cleanupUnknownRecord.cleanup_result.verified_absent -and $cleanupUnknownRecord.record_status -eq 'blocked') 'unknown residual cleanup state was declared clean'

    Write-Host 'SELFTEST_PHASE=adj-0003-slow-operator-pre-enter-capture'
    # ADJ-20260808-0003 A: events must be captured even when the operator is slow (action
    # delay >= 8s) and the UI_START / UI_STOP / CREATE_ACCEPTED fire shortly after each prompt,
    # well before Enter. The device-time lower bound is the prompt (minus frozen skew), never
    # the completedAt. Events are split by step so each step's postcondition sees its own window.
    $slowOpFixture = New-SimulationFixture
    $slowOpFixture.operator.action_delay_seconds = 8
    $slowOpFixture.scenario_events.'2' = @(
        [ordered]@{ offset_seconds = 0.5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 0.5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 1; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a2|fd=42|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 2; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=post-create|open=true|marker=CREATE_ACCEPTED" }
    )
    $slowOpFixture.scenario_events.'3' = @(
        [ordered]@{ offset_seconds = 0.5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2|basis=last-known-request" },
        [ordered]@{ offset_seconds = 1; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') STOP_PROMISE_RESOLVED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 2; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a2" },
        [ordered]@{ offset_seconds = 3; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy" },
        [ordered]@{ offset_seconds = 4; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $slowOpPath = Write-JsonFixture 'simulation-adj-0003-slow-operator.json' $slowOpFixture
    $slowOpPaths = New-CasePaths 'adj-0003-slow-operator'
    $slowOpRun = Invoke-Runner $liveFreezePath $slowOpPaths.Evidence $slowOpPaths.Raw -FixturePath $slowOpPath
    Assert-True ($slowOpRun.ExitCode -eq 0) "slow-operator pre-enter events were not captured: $($slowOpRun.Text)"
    $slowOpRecord = Get-Content -LiteralPath (Join-Path $slowOpPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($slowOpRecord.scenarios[1].result -eq 'pass' -and $slowOpRecord.scenarios[2].result -eq 'pass') 'S2/S3 pre-enter events lost under slow operator'
    Assert-True ([double]$slowOpRecord.scenarios[1].observation.action_interval_seconds -ge 8 -and [double]$slowOpRecord.scenarios[1].observation.action_interval_seconds -lt 60) 'S2 action interval did not reflect the slow operator delay'

    Write-Host 'SELFTEST_PHASE=adj-0003-extra-action-invalid'
    # ADJ-20260808-0003 A/B: an extra UI action that is not owned by the current step is invalid
    # (a stray UI_START arriving in the gap after S2 step 1 must invalidate before step 2's prompt).
    $extraActionFixture = New-SimulationFixture
    $extraActionFixture.scenario_events.'2' = @(
        [ordered]@{ offset_seconds = 0.5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 0.5; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 1; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a2|fd=42|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 2; relative_to_prompt = $true; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a2|phase=post-create|open=true|marker=CREATE_ACCEPTED" }
    )
    $extraActionFixture.gap_actions = @(
        [ordered]@{ scenario = 2; after_step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2-extra" }
    )
    $extraActionPath = Write-JsonFixture 'simulation-adj-0003-extra-action.json' $extraActionFixture
    $extraActionPaths = New-CasePaths 'adj-0003-extra-action'
    $extraActionRun = Invoke-Runner $liveFreezePath $extraActionPaths.Evidence $extraActionPaths.Raw -FixturePath $extraActionPath
    Assert-True ($extraActionRun.ExitCode -eq 2) "extra UI action did not invalidate: $($extraActionRun.Text)"
    $extraActionRecord = Get-Content -LiteralPath (Join-Path $extraActionPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($extraActionRecord.overall -eq 'invalid' -and $extraActionRecord.scenarios[1].result -eq 'invalid' -and $extraActionRecord.scenarios[1].reason -match 'stray-operator-action|UI_START') 'extra UI action reason mismatch'
    Assert-True ($extraActionRecord.scenarios[2].result -eq 'invalid' -and $extraActionRecord.scenarios[2].reason -eq 'not-run-due-to-invalid') 'extra UI action did not stop later scenarios'

    Write-Host 'SELFTEST_PHASE=adj-0003-cross-scenario-gap-extra-action'
    # ADJ-20260808-0003 B: a UI action arriving in the gap after a scenario's window (not owned
    # by any step) invalidates the next scenario before any prompt.
    $gapFixture = New-SimulationFixture
    $gapFixture.gap_actions = @(
        [ordered]@{ scenario = 5; after_step_index = 0; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5-stray" }
    )
    $gapPath = Write-JsonFixture 'simulation-adj-0003-cross-scenario-gap.json' $gapFixture
    $gapPaths = New-CasePaths 'adj-0003-cross-scenario-gap'
    $gapRun = Invoke-Runner $liveFreezePath $gapPaths.Evidence $gapPaths.Raw -FixturePath $gapPath
    Assert-True ($gapRun.ExitCode -eq 2) "cross-scenario gap action did not invalidate: $($gapRun.Text)"
    $gapRecord = Get-Content -LiteralPath (Join-Path $gapPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($gapRecord.overall -eq 'invalid' -and $gapRecord.scenarios[3].result -eq 'pass' -and $gapRecord.scenarios[4].result -eq 'invalid' -and $gapRecord.scenarios[4].reason -match 'stray-operator-action|UI_START') 'cross-scenario gap action reason mismatch'

    Write-Host 'SELFTEST_PHASE=adj-0003-real-layouts-selftest'
    # ADJ-20260808-0003 (C6): the authorization / entry / settings profiles must match the real
    # attributes/children array shape (top-level array; each node { attributes: { bundleName,
    # type, id, key, text, ... }, children: [...] }). The overrides below are sanitized
    # STRUCTURE fixtures, not byte copies of any sealed raw.
    $realLayoutFixture = New-SimulationFixture
    $realLayoutFixture.layout_profiles['scenario-2-authorization'] = @(
        [ordered]@{ attributes = [ordered]@{ bundleName = 'com.huawei.hmos.vpndialog'; type = 'Dialog'; id = ''; key = ''; text = 'E3 Physical VPN Preflight' }; children = @(
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = '是否允许使用 VPN？' }; children = @() },
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_cancel_button'; key = 'permission_cancel_button'; text = '取消' }; children = @() },
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_allow_button'; key = 'permission_allow_button'; text = '允许' }; children = @() }
        ) }
    )
    $realLayoutPath = Write-JsonFixture 'simulation-adj-0003-real-layouts.json' $realLayoutFixture
    $realLayoutPaths = New-CasePaths 'adj-0003-real-layouts'
    $realLayoutRun = Invoke-Runner $liveFreezePath $realLayoutPaths.Evidence $realLayoutPaths.Raw -FixturePath $realLayoutPath
    Assert-True ($realLayoutRun.ExitCode -eq 0) "real authorization layout did not match: $($realLayoutRun.Text)"

    Write-Host 'SELFTEST_PHASE=adj-0003-api26-auth-minimal-shape'
    # API26 minimal authorization shape: the smallest real attribute/children dialog surface
    # (dialog node owns bundleName+type=Dialog; question text carries 允许+VPN; 允许/取消 buttons).
    $authMinFixture = New-SimulationFixture
    $authMinFixture.layout_profiles['scenario-2-authorization'] = @(
        [ordered]@{ attributes = [ordered]@{ bundleName = 'com.huawei.hmos.vpndialog'; type = 'Dialog'; id = ''; key = ''; text = '' }; children = @(
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = '是否允许使用 VPN？' }; children = @() },
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_allow_button'; key = 'permission_allow_button'; text = '允许' }; children = @() },
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_cancel_button'; key = 'permission_cancel_button'; text = '取消' }; children = @() }
        ) }
    )
    $authMinPath = Write-JsonFixture 'simulation-adj-0003-api26-auth-min.json' $authMinFixture
    $authMinPaths = New-CasePaths 'adj-0003-api26-auth-min'
    $authMinRun = Invoke-Runner $liveFreezePath $authMinPaths.Evidence $authMinPaths.Raw -FixturePath $authMinPath
    Assert-True ($authMinRun.ExitCode -eq 0) "API26 minimal authorization layout did not match: $($authMinRun.Text)"

    Write-Host 'SELFTEST_PHASE=adj-0003-historical-attributes-shape'
    # Historical attributes shape: the vpndialog is a deep child (the emulator dumps a
    # WindowScene/sceneboard root with the system dialog nested several levels down), and the
    # buttons use the explicit English Allow/Cancel compat tokens. The generic any-node matcher
    # must still find owner/type/text/controls at depth.
    $histFixture = New-SimulationFixture
    $histFixture.layout_profiles['scenario-2-authorization'] = @(
        [ordered]@{ attributes = [ordered]@{ bundleName = 'com.ohos.sceneboard'; type = 'WindowScene'; id = 'session10'; key = 'session10'; text = '' }; children = @(
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'root'; id = ''; key = ''; text = '' }; children = @(
                [ordered]@{ attributes = [ordered]@{ bundleName = 'com.huawei.hmos.vpndialog'; type = 'Dialog'; id = ''; key = ''; text = 'E3 Physical VPN Preflight' }; children = @(
                    [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Text'; id = ''; key = ''; text = '是否允许使用 VPN？' }; children = @() },
                    [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_allow_button'; key = 'permission_allow_button'; text = 'Allow' }; children = @() },
                    [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'permission_cancel_button'; key = 'permission_cancel_button'; text = 'Cancel' }; children = @() }
                ) }
            ) }
        ) }
    )
    $histPath = Write-JsonFixture 'simulation-adj-0003-historical-shape.json' $histFixture
    $histPaths = New-CasePaths 'adj-0003-historical-shape'
    $histRun = Invoke-Runner $liveFreezePath $histPaths.Evidence $histPaths.Raw -FixturePath $histPath
    Assert-True ($histRun.ExitCode -eq 0) "historical attributes authorization layout did not match: $($histRun.Text)"

    Write-Host 'SELFTEST_PHASE=adj-0003-layout-resample-same-name-final-only'
    # ADJ-20260808-0003 (C6): the bounded same-name layout resample re-captures under the SAME
    # name and REPLACES the previous same-name CaptureArtifacts record, so the final
    # layout_state_reference and the manifest external_raw_files hold exactly ONE entry per
    # capture name (the final converged bytes), never the intermediate mismatched bytes. The
    # layout_ready_delays knob keeps the authorization layout "not ready" (generic) for a few
    # virtual seconds, so the first checkpoint capture mismatches and the resample converges in
    # time. Intermediate mismatched bytes are not required to be preserved anywhere.
    $resampleFixture = New-SimulationFixture
    $resampleFixture.layout_ready_delays = [ordered]@{ 'scenario-2-authorization' = 3 }
    $resamplePath = Write-JsonFixture 'simulation-adj-0003-layout-resample.json' $resampleFixture
    $resamplePaths = New-CasePaths 'adj-0003-layout-resample'
    $resampleRun = Invoke-Runner $liveFreezePath $resamplePaths.Evidence $resamplePaths.Raw -FixturePath $resamplePath
    Assert-True ($resampleRun.ExitCode -eq 0) "same-name layout resample did not converge: $($resampleRun.Text)"
    $resampleRecord = Get-Content -LiteralPath (Join-Path $resamplePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    # The final layout_state_reference must hold exactly ONE collected entry for the resampled name.
    $resampleLayoutRefs = @($resampleRecord.layout_state_reference | Where-Object { $_.name -eq 'scenario-2-authorization' -and $_.status -eq 'collected' })
    Assert-True ($resampleLayoutRefs.Count -eq 1) 'same-name layout resample left more than one final CaptureArtifacts/layout_state_reference entry'
    Assert-True ($resampleLayoutRefs[0].layout.reference -eq 'RAW-LAYOUT-scenario-2-authorization') 'resample final layout reference mismatch'
    # The manifest external_raw_files must reference the same single capture (json + png), not
    # any intermediate overwritten bytes.
    $resampleManifest = Get-Content -LiteralPath (Join-Path $resamplePaths.Evidence 'hash-manifest.json') -Raw | ConvertFrom-Json -Depth 20
    $resampleRawRefs = @($resampleManifest.external_raw_files | Where-Object { [string]$_.reference -match 'capture-scenario-2-authorization\.(json|png)$' })
    Assert-True ($resampleRawRefs.Count -eq 2) 'same-name layout resample manifest references intermediate overwritten capture files'
    Assert-True ((@($resampleRawRefs | Where-Object { $_.reference -eq 'RAW-capture-scenario-2-authorization.json' }).Count -eq 1) -and (@($resampleRawRefs | Where-Object { $_.reference -eq 'RAW-capture-scenario-2-authorization.png' }).Count -eq 1)) 'resample manifest did not keep exactly one json + one png for the resampled name'
    # The transcript must record the resample attempts (proving a real bounded resample happened).
    $resampleTranscript = @(Get-Content -LiteralPath (Join-Path $resamplePaths.Evidence 'projection\transcript.redacted.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    $resampleAttempts = @($resampleTranscript | Where-Object { [string]$_.payload.kind -eq 'machine-layout-resample' -and [string]$_.payload.data.name -eq 'scenario-2-authorization' })
    Assert-True ($resampleAttempts.Count -ge 1) 'same-name layout resample did not record any machine-layout-resample attempts'
    Assert-ManifestAndSeal $resamplePaths.Evidence
    Assert-ProjectionChain $resamplePaths.Evidence

    Write-Host 'SELFTEST_PHASE=adj-0003-wrong-bundle-entry'
    # The entry profile requires ExpectedBundle + button id/key start-vpn/stop-vpn: a real-shape
    # entry page with the right start/stop buttons but the WRONG bundle must be invalid.
    $wrongEntryFixture = New-SimulationFixture
    $wrongEntryFixture.layout_profiles['scenario-2-entry-a'] = @(
        [ordered]@{ attributes = [ordered]@{ bundleName = 'com.example.wrongbundle'; type = 'root'; id = ''; key = ''; text = '' }; children = @(
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'start-vpn'; key = 'start-vpn'; text = 'Start VPN' }; children = @() },
            [ordered]@{ attributes = [ordered]@{ bundleName = ''; type = 'Button'; id = 'stop-vpn'; key = 'stop-vpn'; text = 'Stop VPN' }; children = @() }
        ) }
    )
    $wrongEntryPath = Write-JsonFixture 'simulation-adj-0003-wrong-bundle-entry.json' $wrongEntryFixture
    $wrongEntryPaths = New-CasePaths 'adj-0003-wrong-bundle-entry'
    $wrongEntryRun = Invoke-Runner $liveFreezePath $wrongEntryPaths.Evidence $wrongEntryPaths.Raw -FixturePath $wrongEntryPath
    Assert-True ($wrongEntryRun.ExitCode -eq 2) "entry layout with wrong bundle passed the entry gate: $($wrongEntryRun.Text)"
    $wrongEntryRecord = Get-Content -LiteralPath (Join-Path $wrongEntryPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($wrongEntryRecord.overall -eq 'invalid' -and $wrongEntryRecord.scenarios[1].result -eq 'invalid' -and $wrongEntryRecord.scenarios[1].reason -match 'expected-bundle|layout') 'wrong-bundle entry layout was not invalidated as expected-bundle'

    $fakeVpnAppFixture = New-SimulationFixture
    $fakeVpnAppFixture.layout_profiles['scenario-5-app-info'] = 'settings-vpn-fake-app'
    $fakeVpnAppPath = Write-JsonFixture 'simulation-adj-0003-fake-vpn-app.json' $fakeVpnAppFixture
    $fakeVpnAppPaths = New-CasePaths 'adj-0003-fake-vpn-app'
    $fakeVpnAppRun = Invoke-Runner $liveFreezePath $fakeVpnAppPaths.Evidence $fakeVpnAppPaths.Raw -FixturePath $fakeVpnAppPath
    Assert-True ($fakeVpnAppRun.ExitCode -eq 2) "app page containing only the word VPN passed the settings-app-info gate: $($fakeVpnAppRun.Text)"
    $fakeVpnAppRecord = Get-Content -LiteralPath (Join-Path $fakeVpnAppPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($fakeVpnAppRecord.overall -eq 'invalid' -and $fakeVpnAppRecord.scenarios[4].result -eq 'invalid') 'app page with only the word VPN was not invalidated'

    Write-Host 'SELFTEST_PHASE=s5-production-settings-app-info-positive'
    # The committed fixture is an ancestor-preserving contraction of sealed production capture
    # 0004. A/B remain under hidden Setting.Application; only the visible AppDetail subtree may match.
    $s5MinFixture = New-SimulationFixture
    $productionFixturePath = Join-Path $project 'tests\fixtures\settings-app-info-production-0004.json'
    $s5MinFixture.layout_profiles['scenario-5-app-info'] = Get-Content -LiteralPath $productionFixturePath -Raw | ConvertFrom-Json -Depth 60
    $hiddenApplication = @($s5MinFixture.layout_profiles['scenario-5-app-info'][0].children | Where-Object { $_.attributes.id -eq 'Setting.Application' })[0]
    Assert-True ($null -ne $hiddenApplication -and [string]$hiddenApplication.attributes.visible -eq 'false' -and @($hiddenApplication.children).Count -eq 2) 'production fixture did not preserve the hidden Setting.Application ancestor for A/B labels'
    $s5MinPath = Write-JsonFixture 'simulation-s5-production-settings-app-info.json' $s5MinFixture
    $s5MinPaths = New-CasePaths 's5-production-settings-app-info'
    $s5MinRun = Invoke-Runner $liveFreezePath $s5MinPaths.Evidence $s5MinPaths.Raw -FixturePath $s5MinPath
    Assert-True ($s5MinRun.ExitCode -eq 0) "minimal settings-app-info layout failed the gate: $($s5MinRun.Text)"
    $s5MinRecord = Get-Content -LiteralPath (Join-Path $s5MinPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5MinRecord.scenarios[4].result -eq 'pass' -and -not $s5MinRecord.scenarios[4].app_info_force_stop_capture.machine_verified -and $s5MinRecord.scenarios[4].app_info_force_stop_capture.observation_only) 'production settings-app-info layout did not pass S5 with an observation-only post-force capture'
    # A correctness is proven by the process effect gate: :vpn absent (met) while the A bundle stays present.
    Assert-True ($s5MinRecord.scenarios[4].process_absent_evidence.met -and $s5MinRecord.scenarios[4].bundle_present_during_probe) 'minimal settings-app-info A-correctness process effect gate not met'
    Assert-True ($null -eq $s5MinRecord.PSObject.Properties['scenario_invalid'] -and $s5MinRecord.record_status -ne 'invalidated') 'minimal settings-app-info layout was invalidated instead of matching'

    Write-Host 'SELFTEST_PHASE=s5-production-expected-label-mismatch'
    $s5MismatchFixture = New-SimulationFixture
    $s5MismatchLayout = Copy-JsonObject (Get-Content -LiteralPath $productionFixturePath -Raw | ConvertFrom-Json -Depth 60)
    $s5MismatchDetail = @($s5MismatchLayout[0].children | Where-Object { $_.attributes.id -eq 'Setting.AppDetail' })[0]
    $s5MismatchDetail.children[0].attributes.text = 'E3 Preflight B'
    $s5MismatchFixture.layout_profiles['scenario-5-app-info'] = $s5MismatchLayout
    $s5MismatchPath = Write-JsonFixture 'simulation-s5-production-expected-label-mismatch.json' $s5MismatchFixture
    $s5MismatchPaths = New-CasePaths 's5-production-expected-label-mismatch'
    $s5MismatchRun = Invoke-Runner $liveFreezePath $s5MismatchPaths.Evidence $s5MismatchPaths.Raw -FixturePath $s5MismatchPath
    Assert-True ($s5MismatchRun.ExitCode -eq 2) "production-shaped B detail passed the expected A gate: $($s5MismatchRun.Text)"
    $s5MismatchRecord = Get-Content -LiteralPath (Join-Path $s5MismatchPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5MismatchRecord.overall -eq 'invalid' -and $s5MismatchRecord.scenarios[4].reason -match 'app-label|layout-mismatch') 'outside A pollution satisfied a B AppDetail mismatch'

    Write-Host 'SELFTEST_PHASE=s5-production-near-text-negative'
    $s5NearFixture = New-SimulationFixture
    $s5NearLayout = Copy-JsonObject (Get-Content -LiteralPath $productionFixturePath -Raw | ConvertFrom-Json -Depth 60)
    $s5NearDetail = @($s5NearLayout[0].children | Where-Object { $_.attributes.id -eq 'Setting.AppDetail' })[0]
    $s5NearDetail.children[0].attributes.text = 'E3 Preflight A preview'
    $s5NearDetail.children[1].attributes.text = '强行停止应用'
    $s5NearFixture.layout_profiles['scenario-5-app-info'] = $s5NearLayout
    $s5NearPath = Write-JsonFixture 'simulation-s5-production-near-text-negative.json' $s5NearFixture
    $s5NearPaths = New-CasePaths 's5-production-near-text-negative'
    $s5NearRun = Invoke-Runner $liveFreezePath $s5NearPaths.Evidence $s5NearPaths.Raw -FixturePath $s5NearPath
    Assert-True ($s5NearRun.ExitCode -eq 2) "near-match label/control passed the strict AppDetail gate: $($s5NearRun.Text)"
    $s5NearRecord = Get-Content -LiteralPath (Join-Path $s5NearPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5NearRecord.overall -eq 'invalid' -and $s5NearRecord.scenarios[4].reason -match 'layout-mismatch') 'near-match AppDetail text was not rejected'

    Write-Host 'SELFTEST_PHASE=s5-hidden-ancestor-app-detail-filter'
    $s5HiddenAndVisibleFixture = New-SimulationFixture
    $s5HiddenAndVisibleLayout = Copy-JsonObject (Get-Content -LiteralPath $productionFixturePath -Raw | ConvertFrom-Json -Depth 60)
    $s5VisibleDetail = @($s5HiddenAndVisibleLayout[0].children | Where-Object { $_.attributes.id -eq 'Setting.AppDetail' })[0]
    $s5HiddenDuplicate = Copy-JsonObject $s5VisibleDetail
    $s5HiddenWrapper = [pscustomobject]([ordered]@{
        attributes = [ordered]@{ bundleName = ''; type = 'Column'; id = 'hidden-detail-ancestor'; key = 'hidden-detail-ancestor'; text = ''; visible = 'false' }
        children = @($s5HiddenDuplicate)
    })
    $s5HiddenAndVisibleLayout[0].children = @($s5HiddenAndVisibleLayout[0].children) + @($s5HiddenWrapper)
    $s5HiddenAndVisibleFixture.layout_profiles['scenario-5-app-info'] = $s5HiddenAndVisibleLayout
    $s5HiddenAndVisiblePath = Write-JsonFixture 'simulation-s5-hidden-and-visible-app-detail.json' $s5HiddenAndVisibleFixture
    $s5HiddenAndVisiblePaths = New-CasePaths 's5-hidden-and-visible-app-detail'
    $s5HiddenAndVisibleRun = Invoke-Runner $liveFreezePath $s5HiddenAndVisiblePaths.Evidence $s5HiddenAndVisiblePaths.Raw -FixturePath $s5HiddenAndVisiblePath
    Assert-True ($s5HiddenAndVisibleRun.ExitCode -eq 0) "hidden-ancestor AppDetail polluted the unique visible set: $($s5HiddenAndVisibleRun.Text)"

    $s5HiddenOnlyFixture = New-SimulationFixture
    $s5HiddenOnlyLayout = Copy-JsonObject (Get-Content -LiteralPath $productionFixturePath -Raw | ConvertFrom-Json -Depth 60)
    $s5HiddenOnlyRoot = $s5HiddenOnlyLayout[0]
    $s5HiddenOnlyDetail = Copy-JsonObject (@($s5HiddenOnlyRoot.children | Where-Object { $_.attributes.id -eq 'Setting.AppDetail' })[0])
    $s5HiddenOnlyWrapper = [pscustomobject]([ordered]@{
        attributes = [ordered]@{ bundleName = ''; type = 'Column'; id = 'hidden-detail-ancestor'; key = 'hidden-detail-ancestor'; text = ''; visible = 'false' }
        children = @($s5HiddenOnlyDetail)
    })
    $s5HiddenOnlyRoot.children = @($s5HiddenOnlyRoot.children | Where-Object { $_.attributes.id -ne 'Setting.AppDetail' }) + @($s5HiddenOnlyWrapper)
    $s5HiddenOnlyFixture.layout_profiles['scenario-5-app-info'] = $s5HiddenOnlyLayout
    $s5HiddenOnlyPath = Write-JsonFixture 'simulation-s5-hidden-only-app-detail.json' $s5HiddenOnlyFixture
    $s5HiddenOnlyPaths = New-CasePaths 's5-hidden-only-app-detail'
    $s5HiddenOnlyRun = Invoke-Runner $liveFreezePath $s5HiddenOnlyPaths.Evidence $s5HiddenOnlyPaths.Raw -FixturePath $s5HiddenOnlyPath
    Assert-True ($s5HiddenOnlyRun.ExitCode -eq 2) "hidden-only AppDetail passed the visible gate: $($s5HiddenOnlyRun.Text)"
    $s5HiddenOnlyRecord = Get-Content -LiteralPath (Join-Path $s5HiddenOnlyPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s5HiddenOnlyRecord.overall -eq 'invalid' -and $s5HiddenOnlyRecord.scenarios[4].reason -match 'layout-mismatch') 'hidden-only AppDetail was not rejected as a layout mismatch'

    Write-Host 'SELFTEST_PHASE=adj-0003-infra-capture-blocked'
    # ADJ-20260808-0003 E: a ScreenCap/DumpLayout/Receive exit 124/125 / timeout must propagate
    # infrastructure blocked (never ScenarioInvalid), with infrastructure_reason set for retry.
    $infraCaptureFixture = New-SimulationFixture
    # ScreenCap occurrence 1 belongs to scenario-1-baseline (plain capture, degrades only);
    # occurrence 2 is scenario-2-entry-a (a decisive layout checkpoint) and must propagate
    # infrastructure blocked instead of ScenarioInvalid.
    $infraCaptureFixture.hdc_failures = @(
        [ordered]@{ operation = 'ScreenCap'; occurrence = 2; exit_code = 124; stdout = ''; stderr = 'operation timeout' }
    )
    $infraCapturePath = Write-JsonFixture 'simulation-adj-0003-infra-capture.json' $infraCaptureFixture
    $infraCapturePaths = New-CasePaths 'adj-0003-infra-capture'
    $infraCaptureRun = Invoke-Runner $liveFreezePath $infraCapturePaths.Evidence $infraCapturePaths.Raw -FixturePath $infraCapturePath
    Assert-True ($infraCaptureRun.ExitCode -ne 0) "infrastructure capture failure did not stop the campaign: $($infraCaptureRun.Text)"
    $infraCaptureRecord = Get-Content -LiteralPath (Join-Path $infraCapturePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($infraCaptureRecord.overall -eq 'blocked' -and $infraCaptureRecord.record_status -eq 'blocked' -and $infraCaptureRecord.infrastructure_reason -eq 'hdc-usb-interruption') 'infrastructure capture failure was not blocked with retry reason'
    Assert-True ($infraCaptureRecord.record_status -ne 'invalidated' -and $null -eq $infraCaptureRecord.PSObject.Properties['scenario_invalid']) 'infrastructure capture failure was misclassified as scenario invalid'

    Write-Host 'SELFTEST_PHASE=adj-0003-continuous-capture-infra-degraded'
    # ADJ-20260808-0003 (C6): a continuous raw-hilog PROCESS/STDERR degradation (capture dies)
    # during a decisive step/layout must classify as infrastructure: the campaign propagates the
    # raw-hilog entry's category, overall stays blocked, infrastructure_reason is
    # hdc-usb-interruption (retry authorized), and there is NO scenario_invalid.
    $continuousInfraFixture = New-SimulationFixture -CaptureDieScenario 2
    $continuousInfraPath = Write-JsonFixture 'simulation-adj-0003-continuous-infra-degraded.json' $continuousInfraFixture
    $continuousInfraPaths = New-CasePaths 'adj-0003-continuous-infra-degraded'
    $continuousInfraRun = Invoke-Runner $liveFreezePath $continuousInfraPaths.Evidence $continuousInfraPaths.Raw -FixturePath $continuousInfraPath
    Assert-True ($continuousInfraRun.ExitCode -ne 0) "continuous capture process death did not stop the campaign: $($continuousInfraRun.Text)"
    $continuousInfraRecord = Get-Content -LiteralPath (Join-Path $continuousInfraPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($continuousInfraRecord.overall -eq 'blocked' -and $continuousInfraRecord.record_status -eq 'blocked' -and $continuousInfraRecord.infrastructure_reason -eq 'hdc-usb-interruption') 'continuous capture process death was not blocked with infrastructure retry reason'
    Assert-True ($continuousInfraRecord.record_status -ne 'invalidated' -and $null -eq $continuousInfraRecord.PSObject.Properties['scenario_invalid']) 'continuous capture process death was misclassified as scenario invalid'
    Assert-True (@($continuousInfraRecord.capture_degraded | Where-Object { $_.component -match 'raw-hilog' -and $_.category -eq 'infrastructure' }).Count -ge 1) 'continuous raw-hilog infrastructure degradation not recorded in capture_degraded'

    Write-Host 'SELFTEST_PHASE=adj-0003-layout-choice-s5-auth-delay'
    # ADJ-20260808-0003: dual-profile 8s same-name resample for S5 reactivation. layout_ready_delays=5
    # keeps the capture generic until the authorization profile becomes ready; the choice gate must
    # converge to authorization (reauth path), not invalid, and keep a single same-name capture ref.
    $s5ChoiceFixture = New-SimulationFixture
    $s5ChoiceFixture.layout_profiles['scenario-5-reactivation'] = 'authorization'
    $s5ChoiceFixture.layout_ready_delays = [ordered]@{ 'scenario-5-reactivation' = 5 }
    # Explicit S5 events so the create chain lands on the Allow step (2) after the reauth choice
    # converges, and the force-stop destroy chain lands on step (4); the layout delay then only
    # exercises the dual-profile resample, never a missing create marker after Allow.
    $s5ChoiceFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 9; step_index = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $s5ChoicePath = Write-JsonFixture 'simulation-adj-0003-s5-layout-choice-auth-delay.json' $s5ChoiceFixture
    $s5ChoicePaths = New-CasePaths 'adj-0003-s5-layout-choice-auth-delay'
    $s5ChoiceRun = Invoke-Runner $liveFreezePath $s5ChoicePaths.Evidence $s5ChoicePaths.Raw -FixturePath $s5ChoicePath
    Assert-True ($s5ChoiceRun.ExitCode -eq 0) "S5 layout-choice auth delay did not converge: $($s5ChoiceRun.Text)"
    $s5ChoiceRecord = Get-Content -LiteralPath (Join-Path $s5ChoicePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($null -eq $s5ChoiceRecord.PSObject.Properties['scenario_invalid'] -and $s5ChoiceRecord.record_status -ne 'invalidated') 'S5 layout-choice auth delay was misclassified as scenario invalid'
    Assert-True ($s5ChoiceRecord.scenarios[4].settings_reallow_path.actual -eq 'system-reauthorization-UI') 'S5 layout-choice did not select authorization/reauth path'
    $s5ChoiceLayoutRefs = @($s5ChoiceRecord.layout_state_reference | Where-Object { $_.name -eq 'scenario-5-reactivation' -and $_.status -eq 'collected' })
    Assert-True ($s5ChoiceLayoutRefs.Count -eq 1) 'S5 layout-choice left more than one same-name capture ref'
    $s5ChoiceTranscript = @(Get-Content -LiteralPath (Join-Path $s5ChoicePaths.Evidence 'projection\transcript.redacted.jsonl') | ForEach-Object { $_ | ConvertFrom-Json -Depth 20 })
    Assert-True ((@($s5ChoiceTranscript | Where-Object { [string]$_.payload.kind -eq 'machine-layout-choice-resample' -and [string]$_.payload.data.name -eq 'scenario-5-reactivation' }).Count -ge 1)) 'S5 layout-choice did not record dual-profile resample attempts'

    Write-Host 'SELFTEST_PHASE=adj-0003-layout-choice-s6-auth-delay'
    # Same dual-profile 8s resample for S6 A reactivation with authorization after delay=5.
    $s6ChoiceFixture = New-SimulationFixture
    $s6ChoiceFixture.layout_profiles['scenario-6-reactivation-a'] = 'authorization'
    $s6ChoiceFixture.layout_ready_delays = [ordered]@{ 'scenario-6-reactivation-a' = 5 }
    $s6ChoiceFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; step_index = 3; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" },
        [ordered]@{ offset_seconds = 9; step_index = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN" }
    )
    $s6ChoicePath = Write-JsonFixture 'simulation-adj-0003-s6-layout-choice-auth-delay.json' $s6ChoiceFixture
    $s6ChoicePaths = New-CasePaths 'adj-0003-s6-layout-choice-auth-delay'
    $s6ChoiceRun = Invoke-Runner $liveFreezePath $s6ChoicePaths.Evidence $s6ChoicePaths.Raw -FixturePath $s6ChoicePath
    Assert-True ($s6ChoiceRun.ExitCode -eq 0) "S6 layout-choice auth delay did not converge: $($s6ChoiceRun.Text)"
    $s6ChoiceRecord = Get-Content -LiteralPath (Join-Path $s6ChoicePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($null -eq $s6ChoiceRecord.PSObject.Properties['scenario_invalid'] -and $s6ChoiceRecord.record_status -ne 'invalidated') 'S6 layout-choice auth delay was misclassified as scenario invalid'
    Assert-True ($s6ChoiceRecord.scenarios[5].a_reauth_path -eq 'system-reauthorization-UI') 'S6 layout-choice did not select authorization/reauth path'
    $s6ChoiceLayoutRefs = @($s6ChoiceRecord.layout_state_reference | Where-Object { $_.name -eq 'scenario-6-reactivation-a' -and $_.status -eq 'collected' })
    Assert-True ($s6ChoiceLayoutRefs.Count -eq 1) 'S6 layout-choice left more than one same-name capture ref'

    Write-Host 'SELFTEST_PHASE=adj-0003-s2-process-precondition-infra-blocked'
    # ADJ-20260808-0003: S2 Get-ExactProcessCheckpoint PidOf infra (exit 124) is blocked with
    # hdc-usb-interruption, never scenario_invalid. PidOf#1/#2 = S1 baseline A/B; #3 = S2 pre A.
    $s2PreInfraFixture = New-SimulationFixture
    $s2PreInfraFixture.hdc_failures = @(
        [ordered]@{ operation = 'PidOf'; occurrence = 3; exit_code = 124; stdout = ''; stderr = 'operation timeout' }
    )
    $s2PreInfraPath = Write-JsonFixture 'simulation-adj-0003-s2-precondition-infra.json' $s2PreInfraFixture
    $s2PreInfraPaths = New-CasePaths 'adj-0003-s2-precondition-infra'
    $s2PreInfraRun = Invoke-Runner $liveFreezePath $s2PreInfraPaths.Evidence $s2PreInfraPaths.Raw -FixturePath $s2PreInfraPath
    Assert-True ($s2PreInfraRun.ExitCode -ne 0) "S2 process precondition infra did not stop the campaign: $($s2PreInfraRun.Text)"
    $s2PreInfraRecord = Get-Content -LiteralPath (Join-Path $s2PreInfraPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s2PreInfraRecord.overall -eq 'blocked' -and $s2PreInfraRecord.record_status -eq 'blocked' -and $s2PreInfraRecord.infrastructure_reason -eq 'hdc-usb-interruption') 'S2 process precondition infra was not blocked with retry reason'
    Assert-True ($null -eq $s2PreInfraRecord.PSObject.Properties['scenario_invalid'] -and $s2PreInfraRecord.record_status -ne 'invalidated') 'S2 process precondition infra was misclassified as scenario invalid'

    Write-Host 'SELFTEST_PHASE=adj-0003-s2-process-precondition-mismatch-blocked'
    # ADJ-20260808-0003: S2 process present when expected absent is blocked process-state-mismatch,
    # never operator scenario_invalid.
    $s2PreMismatchFixture = New-SimulationFixture
    $s2PreMismatchFixture.hdc_failures = @(
        [ordered]@{ operation = 'PidOf'; occurrence = 3; exit_code = 0; stdout = '99999'; stderr = '' }
    )
    $s2PreMismatchPath = Write-JsonFixture 'simulation-adj-0003-s2-precondition-mismatch.json' $s2PreMismatchFixture
    $s2PreMismatchPaths = New-CasePaths 'adj-0003-s2-precondition-mismatch'
    $s2PreMismatchRun = Invoke-Runner $liveFreezePath $s2PreMismatchPaths.Evidence $s2PreMismatchPaths.Raw -FixturePath $s2PreMismatchPath
    Assert-True ($s2PreMismatchRun.ExitCode -ne 0) "S2 process precondition mismatch did not stop the campaign: $($s2PreMismatchRun.Text)"
    $s2PreMismatchRecord = Get-Content -LiteralPath (Join-Path $s2PreMismatchPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s2PreMismatchRecord.overall -eq 'blocked' -and $s2PreMismatchRecord.record_status -eq 'blocked') 'S2 process precondition mismatch was not overall blocked'
    Assert-True ($null -eq $s2PreMismatchRecord.PSObject.Properties['scenario_invalid'] -and $s2PreMismatchRecord.record_status -ne 'invalidated') 'S2 process precondition mismatch was misclassified as scenario invalid'
    Assert-True ($s2PreMismatchRun.Text -match 'machine-precondition-blocked|process-state-mismatch') 'S2 process precondition mismatch did not surface machine-precondition-blocked text'

    Write-Host 'SELFTEST_PHASE=adj-0003-s6-b-non-frozen-code-blocked'
    # ADJ-20260808-0003 (C6): S6 B reject with a NON-frozen code (2203001, freeze only freezes
    # 2203002) is a platform result, not an operator error: complete the observation, record S6
    # blocked `B-conflict-code-not-frozen:2203001`, S7 not-run-after-platform-blocked, overall
    # blocked, NO scenario_invalid, finally cleanup verified. A reject stays functional fail.
    $s6NonFrozenFixture = New-SimulationFixture
    $s6NonFrozenFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203001,name=BusinessError,message=another active vpn exists" }
    )
    $s6NonFrozenPath = Write-JsonFixture 'simulation-adj-0003-s6-b-non-frozen-code.json' $s6NonFrozenFixture
    $s6NonFrozenPaths = New-CasePaths 'adj-0003-s6-b-non-frozen-code'
    $s6NonFrozenRun = Invoke-Runner $liveFreezePath $s6NonFrozenPaths.Evidence $s6NonFrozenPaths.Raw -FixturePath $s6NonFrozenPath
    Assert-True ($s6NonFrozenRun.ExitCode -eq 0) "S6 non-frozen B code crashed the runner: $($s6NonFrozenRun.Text)"
    $s6NonFrozenRecord = Get-Content -LiteralPath (Join-Path $s6NonFrozenPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6NonFrozenRecord.overall -eq 'blocked' -and $s6NonFrozenRecord.record_status -eq 'blocked') 'S6 non-frozen B code was not overall blocked'
    Assert-True ($null -eq $s6NonFrozenRecord.PSObject.Properties['scenario_invalid'] -and $s6NonFrozenRecord.record_status -ne 'invalidated') 'S6 non-frozen B code was misclassified as scenario invalid'
    Assert-True ($s6NonFrozenRecord.scenarios[5].result -eq 'blocked' -and $s6NonFrozenRecord.scenarios[5].reason -eq 'B-conflict-code-not-frozen:2203001') 'S6 non-frozen B code reason/result mismatch'
    Assert-True ([int]$s6NonFrozenRecord.scenarios[5].b_rejection_code -eq 2203001 -and -not $s6NonFrozenRecord.scenarios[5].b_accepted -and $s6NonFrozenRecord.scenarios[5].a_accepted -and [int]$s6NonFrozenRecord.scenarios[5].accepted_session_count_in_window -eq 2) 'S6 non-frozen B code record fields/count mismatch'
    Assert-True ($s6NonFrozenRecord.scenarios[6].result -eq 'blocked' -and $s6NonFrozenRecord.scenarios[6].reason -eq 'not-run-after-platform-blocked') 'S7 did not stay not-run-after-platform-blocked after S6 platform blocked'
    Assert-True ($s6NonFrozenRecord.cleanup_result.verified_absent -and $s6NonFrozenRecord.cleanup_result.status -eq 'verified-clean') 'S6 platform-blocked run did not finish verified finally cleanup'

    Write-Host 'SELFTEST_PHASE=adj-0003-s6-a-reject-fail'
    # ADJ-20260808-0003 F: S6 A EXTENSION create rejected / invalid fd (VPN_ONCREATE observed
    # BEFORE the VPN_CREATE_REJECTED) is a functional fail, not an operator invalid. B Start is
    # never asked and S7 is not-run after functional fail.
    $s6ARejectFixture = New-SimulationFixture
    $s6ARejectFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=a6|phase=create|summary=code=2201001,name=BusinessError,message=create rejected" }
    )
    $s6ARejectPath = Write-JsonFixture 'simulation-adj-0003-s6-a-reject.json' $s6ARejectFixture
    $s6ARejectPaths = New-CasePaths 'adj-0003-s6-a-reject'
    $s6ARejectRun = Invoke-Runner $liveFreezePath $s6ARejectPaths.Evidence $s6ARejectPaths.Raw -FixturePath $s6ARejectPath
    Assert-True ($s6ARejectRun.ExitCode -eq 0) "S6 A extension reject was not classified as functional fail: $($s6ARejectRun.Text)"
    $s6ARejectRecord = Get-Content -LiteralPath (Join-Path $s6ARejectPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6ARejectRecord.scenarios[5].result -eq 'fail' -and $s6ARejectRecord.scenarios[5].reason -eq 'A-create-rejected-or-invalid-fd') 'S6 A extension create reject was not fail'
    Assert-True ($s6ARejectRecord.scenarios[5].a_on_create -eq $true -and $s6ARejectRecord.scenarios[5].a_extension_rejected -eq $true -and -not $s6ARejectRecord.scenarios[5].a_auth_unclassified) 'S6 A extension reject classification fields mismatch'
    Assert-True ($s6ARejectRecord.scenarios[5].b_accepted -eq $false -and $s6ARejectRecord.scenarios[6].result -eq 'blocked' -and $s6ARejectRecord.scenarios[6].reason -match 'not-run-after-functional-fail') 'S6 A reject still asked B Start or ran S7'
    Assert-True ($s6ARejectRecord.overall -eq 'fail') 'S6 A reject overall not fail'

    Write-Host 'SELFTEST_PHASE=adj-0003-s6-a-start-promise-rejected-blocked'
    # ADJ-20260808-0003 F: S6 A with a PURE authorization-layer outcome — START_PROMISE_REJECTED
    # and NO VPN_ONCREATE — is S6 blocked `authorization-outcome-unclassified` (never a fail, never
    # a scenario invalid), S7 stays not-run-after-platform-blocked, overall blocked. This matches
    # S2's authorization-outcome-unclassified classification for the same marker set.
    $s6AStartPromiseFixture = New-SimulationFixture
    $s6AStartPromiseFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') START_PROMISE_REJECTED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6|summary=user-denied" }
    )
    $s6AStartPromisePath = Write-JsonFixture 'simulation-adj-0003-s6-a-start-promise-rejected.json' $s6AStartPromiseFixture
    $s6AStartPromisePaths = New-CasePaths 'adj-0003-s6-a-start-promise-rejected'
    $s6AStartPromiseRun = Invoke-Runner $liveFreezePath $s6AStartPromisePaths.Evidence $s6AStartPromisePaths.Raw -FixturePath $s6AStartPromisePath
    Assert-True ($s6AStartPromiseRun.ExitCode -eq 0) "S6 A START_PROMISE_REJECTED was not blocked: $($s6AStartPromiseRun.Text)"
    $s6AStartPromiseRecord = Get-Content -LiteralPath (Join-Path $s6AStartPromisePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6AStartPromiseRecord.overall -eq 'blocked' -and $s6AStartPromiseRecord.record_status -eq 'blocked' -and $s6AStartPromiseRecord.overall -ne 'invalid' -and $s6AStartPromiseRecord.overall -ne 'fail') 'S6 A START_PROMISE_REJECTED was not overall blocked'
    Assert-True ($null -eq $s6AStartPromiseRecord.PSObject.Properties['scenario_invalid'] -and $s6AStartPromiseRecord.record_status -ne 'invalidated') 'S6 A START_PROMISE_REJECTED was misclassified as scenario invalid'
    Assert-True ($s6AStartPromiseRecord.scenarios[5].result -eq 'blocked' -and $s6AStartPromiseRecord.scenarios[5].reason -eq 'authorization-outcome-unclassified') 'S6 A START_PROMISE_REJECTED reason/result mismatch'
    Assert-True (-not $s6AStartPromiseRecord.scenarios[5].a_on_create -and -not $s6AStartPromiseRecord.scenarios[5].a_extension_rejected -and $s6AStartPromiseRecord.scenarios[5].a_auth_unclassified) 'S6 A START_PROMISE_REJECTED classification fields mismatch'
    Assert-True ($s6AStartPromiseRecord.scenarios[6].result -eq 'blocked' -and $s6AStartPromiseRecord.scenarios[6].reason -eq 'not-run-after-platform-blocked') 'S7 did not stay not-run-after-platform-blocked after S6 A authorization blocked'
    Assert-True ($s6AStartPromiseRecord.cleanup_result.verified_absent -and $s6AStartPromiseRecord.cleanup_result.status -eq 'verified-clean') 'S6 A authorization-blocked run did not finish verified finally cleanup'

    Write-Host 'SELFTEST_PHASE=adj-0003-s6-a-reauth-success'
    # ADJ-20260808-0003 F: S6 A supports OPTIONAL reauthorization like S5. The A Start is
    # followed by a capture classified as authorization (system-reauthorization-UI); the operator
    # is asked one mechanical Allow step and the A create terminal arrives; S6 then runs the B
    # conflict path and passes on the frozen 2203002 code. The S6 record must expose
    # a_reauth_path=system-reauthorization-UI and the whole run stays pass.
    $s6AReauthFixture = New-SimulationFixture
    $s6AReauthFixture.layout_profiles['scenario-6-reactivation-a'] = 'authorization'
    $s6AReauthFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; step_index = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 3; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; step_index = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; step_index = 3; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b6" },
        [ordered]@{ offset_seconds = 9; step_index = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN" }
    )
    $s6AReauthPath = Write-JsonFixture 'simulation-adj-0003-s6-a-reauth-success.json' $s6AReauthFixture
    $s6AReauthPaths = New-CasePaths 'adj-0003-s6-a-reauth-success'
    $s6AReauthRun = Invoke-Runner $liveFreezePath $s6AReauthPaths.Evidence $s6AReauthPaths.Raw -FixturePath $s6AReauthPath
    Assert-True ($s6AReauthRun.ExitCode -eq 0) "S6 A reauth success crashed: $($s6AReauthRun.Text)"
    $s6AReauthRecord = Get-Content -LiteralPath (Join-Path $s6AReauthPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($s6AReauthRecord.overall -eq 'blocked' -and $s6AReauthRecord.record_status -eq 'blocked' -and $s6AReauthRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'S6 A reauth success did not aggregate pass'
    Assert-True (@($s6AReauthRecord.scenarios | Where-Object { $_.result -ne 'pass' }).Count -eq 0) 'S6 A reauth success run did not keep every scenario pass'
    Assert-True ($s6AReauthRecord.scenarios[5].result -eq 'pass' -and $s6AReauthRecord.scenarios[5].reason -eq 'B-explicit-conflict-rejection') 'S6 A reauth success conflict pass mismatch'
    Assert-True ($s6AReauthRecord.scenarios[5].a_reauth_path -eq 'system-reauthorization-UI') 'S6 A reauth path was not recorded as system-reauthorization-UI'
    Assert-True (@($s6AReauthRecord.scenarios[5].observation.operator_steps | Where-Object { [int]$_.step_index -eq 2 -and [string]$_.expected_action -eq '点击 Allow' }).Count -eq 1) 'S6 A reauth Allow step missing from the observation'

    Write-Host 'SELFTEST_PHASE=adj-0003-process-target-verified'
    # ADJ-20260808-0003 G: after CREATE_ACCEPTED the precise `:vpn` present checkpoint must pass
    # (naming verified); absent/unverifiable is blocked with process-target-unverified. PidOf#5
    # is the S2 process-target checkpoint (baseline A/B = #1/#2, S2 pre-check A/B = #3/#4).
    $ptAbsentFixture = New-SimulationFixture
    $ptAbsentFixture.hdc_failures = @(
        [ordered]@{ operation = 'PidOf'; occurrence = 5; exit_code = 1; stdout = ''; stderr = '' }
    )
    $ptAbsentPath = Write-JsonFixture 'simulation-adj-0003-process-target-absent.json' $ptAbsentFixture
    $ptAbsentPaths = New-CasePaths 'adj-0003-process-target-absent'
    $ptAbsentRun = Invoke-Runner $liveFreezePath $ptAbsentPaths.Evidence $ptAbsentPaths.Raw -FixturePath $ptAbsentPath
    Assert-True ($ptAbsentRun.ExitCode -eq 0) "process-target absent checkpoint crashed: $($ptAbsentRun.Text)"
    $ptAbsentRecord = Get-Content -LiteralPath (Join-Path $ptAbsentPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($ptAbsentRecord.scenarios[1].result -eq 'blocked' -and $ptAbsentRecord.scenarios[1].reason -match 'process-target-unverified') 'process-target absent was not blocked with explicit reason'
    Assert-True ($ptAbsentRecord.scenarios[1].process_target_verified -eq $false -and $ptAbsentRecord.scenarios[2].result -eq 'blocked') 'process-target absent still allowed S3 to consume an unverified checkpoint'

    $ptErrorFixture = New-SimulationFixture
    $ptErrorFixture.hdc_failures = @(
        [ordered]@{ operation = 'PidOf'; occurrence = 5; exit_code = 124; stdout = ''; stderr = 'timeout' }
    )
    $ptErrorPath = Write-JsonFixture 'simulation-adj-0003-process-target-error.json' $ptErrorFixture
    $ptErrorPaths = New-CasePaths 'adj-0003-process-target-error'
    $ptErrorRun = Invoke-Runner $liveFreezePath $ptErrorPaths.Evidence $ptErrorPaths.Raw -FixturePath $ptErrorPath
    Assert-True ($ptErrorRun.ExitCode -eq 0) "process-target error checkpoint crashed: $($ptErrorRun.Text)"
    $ptErrorRecord = Get-Content -LiteralPath (Join-Path $ptErrorPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($ptErrorRecord.scenarios[1].result -eq 'blocked' -and $ptErrorRecord.scenarios[1].reason -match 'process-target-unverified') 'process-target error was not blocked with explicit reason'

    Write-Host 'SELFTEST_PHASE=adj-0003-null-tri-state'
    # ADJ-20260808-0003 I: a present clean_reactivation_proof key with null stays null (not
    # cast to false).
    $nullProofFixture = New-SimulationFixture
    $nullProofFixture.scenario_events.'2' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a2" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP_SKIPPED|bundle=cn.alfadb.netbird.e3physvpna|reason=no-active-request" }
    )
    $nullProofPath = Write-JsonFixture 'simulation-adj-0003-null-tri-state.json' $nullProofFixture
    $nullProofPaths = New-CasePaths 'adj-0003-null-tri-state'
    $nullProofRun = Invoke-Runner $liveFreezePath $nullProofPaths.Evidence $nullProofPaths.Raw -FixturePath $nullProofPath
    Assert-True ($nullProofRun.ExitCode -eq 2) "null tri-state fixture did not invalidate: $($nullProofRun.Text)"
    $nullProofRecord = Get-Content -LiteralPath (Join-Path $nullProofPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($null -eq $nullProofRecord.scenario_aggregation.s3_clean_reactivation_proof) 'null clean_reactivation_proof was cast to false'

    Write-Host 'SELFTEST_PHASE=adj-0003-wait-state-tamper'
    # ADJ-20260808-0003 J: a final wait state must be complete; tampering it to waiting must be
    # detected as evidence integrity invalid.
    $waitTamperFixture = New-SimulationFixture
    $waitTamperFixture.tamper_wait_state_after_complete = $true
    $waitTamperPath = Write-JsonFixture 'simulation-adj-0003-wait-state-tamper.json' $waitTamperFixture
    $waitTamperPaths = New-CasePaths 'adj-0003-wait-state-tamper'
    $waitTamperRun = Invoke-Runner $liveFreezePath $waitTamperPaths.Evidence $waitTamperPaths.Raw -FixturePath $waitTamperPath
    Assert-True ($waitTamperRun.ExitCode -eq 2) 'wait-state tamper did not exit as integrity invalid'
    $waitTamperRecord = Get-Content -LiteralPath (Join-Path $waitTamperPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($waitTamperRecord.record_status -eq 'invalidated' -and 'operator-wait-state-not-complete' -in @($waitTamperRecord.integrity_violations)) 'wait-state tamper was not detected as integrity invalid'

    Write-Host 'SELFTEST_PHASE=real-repository-gate-and-transcript-integrity'
    $dirtyMarker = Join-Path $repo 'simulation-dirty-marker.txt'
    [IO.File]::WriteAllText($dirtyMarker, 'dirty repository gate fixture', [Text.UTF8Encoding]::new($false))
    try {
        $dirtyBeforePaths = New-CasePaths 'repo-dirty-before'
        $dirtyBeforeRun = Invoke-Runner $liveFreezePath $dirtyBeforePaths.Evidence $dirtyBeforePaths.Raw -FixturePath $baseFixturePath
        Assert-True ($dirtyBeforeRun.ExitCode -ne 0 -and -not (Test-Path -LiteralPath $dirtyBeforePaths.Evidence)) 'simulation bypassed the real clean repository gate'
    } finally {
        Remove-Item -LiteralPath $dirtyMarker -Force -ErrorAction SilentlyContinue
    }

    $tamperFixturePath = Write-JsonFixture 'simulation-chain-tamper.json' (New-SimulationFixture -TamperTranscript $true)
    $tamperPaths = New-CasePaths 'chain-tamper'
    $tamperRun = Invoke-Runner $liveFreezePath $tamperPaths.Evidence $tamperPaths.Raw -FixturePath $tamperFixturePath
    Assert-True ($tamperRun.ExitCode -eq 2) 'simulation transcript tamper did not exit as integrity invalid'
    $tamperRecord = Get-Content -LiteralPath (Join-Path $tamperPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($tamperRecord.record_status -eq 'invalidated' -and $tamperRecord.verdict -eq 'invalid' -and 'transcript-json-invalid' -in @($tamperRecord.integrity_violations)) 'hash-chain tamper was not detected as integrity invalid'
    Assert-True (@($liveRecord.integrity_violations).Count -eq 0) 'baseline live simulation integrity must remain empty after tamper isolation'

    $payloadTamperFixturePath = Write-JsonFixture 'simulation-payload-tamper.json' (New-SimulationFixture -TamperPayload $true)
    $payloadTamperPaths = New-CasePaths 'payload-tamper'
    $payloadTamperRun = Invoke-Runner $liveFreezePath $payloadTamperPaths.Evidence $payloadTamperPaths.Raw -FixturePath $payloadTamperFixturePath
    Assert-True ($payloadTamperRun.ExitCode -eq 2) 'simulation payload tamper did not exit as integrity invalid'
    $payloadTamperRecord = Get-Content -LiteralPath (Join-Path $payloadTamperPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ('transcript-payload-canonical-mismatch' -in @($payloadTamperRecord.integrity_violations) -and $payloadTamperRecord.record_status -eq 'invalidated' -and $payloadTamperRecord.verdict -eq 'invalid') 'human-readable payload tamper was not detected as integrity invalid'
    Assert-True (@($tamperRecord.integrity_violations).Count -gt 0 -and @($payloadTamperRecord.integrity_violations).Count -gt 0) 'tamper cases must still surface integrity_violations'

    Assert-True (-not (Test-Path -LiteralPath $script:HdcLaunchMarker)) 'host-only suite launched the HDC sentinel process'
    $allRecords = Get-ChildItem -LiteralPath $tempRoot -Filter 'scenario-results.json' -File -Recurse
    foreach ($recordFile in $allRecords) {
        $recordText = Get-Content -LiteralPath $recordFile.FullName -Raw
        Assert-True ($recordText -notmatch 'reviewed-pass|reviewed-fail') "runner emitted reviewed state in $($recordFile.FullName)"
    }
    Write-Host "SELFTEST_HDC_SENTINEL=$($script:SentinelHdc)"
    Write-Host 'LIVE_SIMULATION_SCENARIOS=7'
    Write-Host 'RUNNER_SELFTEST_RESULT=pass HDC_PROCESSES=0'
} finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
