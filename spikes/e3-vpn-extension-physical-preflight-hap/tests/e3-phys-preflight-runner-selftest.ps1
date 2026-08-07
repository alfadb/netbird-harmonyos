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
        [string]$FixturePath
    )
    $arguments = @(
        '-NoProfile', '-File', $runner,
        '-FreezeManifest', $FreezePath,
        '-EvidenceRoot', $EvidencePath,
        '-RawRoot', $RawPath,
        '-HapA', $script:HapA,
        '-HapB', $script:HapB,
        '-HdcPath', $script:SentinelHdc
    )
    if ($AsDryRun) {
        $arguments += '-DryRun'
    } else {
        $arguments += @('-LiveSimulation', '-SimulationFixture', $FixturePath)
    }
    $output = & pwsh @arguments 2>&1
    return [pscustomobject]@{ ExitCode = $LASTEXITCODE; Text = ($output -join "`n"); Lines = @($output) }
}

function New-Freeze {
    param([string]$PlanStatus = 'ready', [string]$EvidenceId = 'EV-E3-SELFTEST-20990101-0001')
    $head = (& git -C $repo rev-parse HEAD 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-f]{40}$') { throw 'unable to get repository HEAD' }
    return [ordered]@{
        schema_version = 1
        plan_status = $PlanStatus
        exception = 'E3-PHYS-PREFLIGHT'
        evidence_id = $EvidenceId
        campaign_id = 'E3-PHYS-PREFLIGHT-SELFTEST'
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
            ready_delay_seconds = 1
            action_ack_delay_seconds = 5
            fail_scenarios = @()
            confirmations = [ordered]@{
                'AUTH-UI-VISIBLE' = $true
                'DENY-SCREEN-CAPTURED' = $true
                'PATH-ACTUAL-DIRECT-SYSTEM-ACTIVATION' = $true
                'PATH-ACTUAL-SYSTEM-REAUTHORIZATION-UI' = $false
                'SETTINGS-REVOKE-CAPTURED' = $true
                'NO-DUAL-ACTIVE-CAPTURED' = $true
                'FINAL-CLEANUP-CAPTURED' = $true
            }
        }
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
                [ordered]@{ offset_seconds = 1; text = "$stamp STOP_PROMISE_RESOLVED|bundle=$a|requestId=a2" },
                [ordered]@{ offset_seconds = 2; text = "$stamp VPN_ONDESTROY|requestId=a2" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_CLOSED_CONFIRMED" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
            )
            '4' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp UI_START|bundle=$b|requestId=b4" },
                [ordered]@{ offset_seconds = 2; text = "$stamp UI_START|bundle=$a|requestId=a-noise" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_ONCREATE|bundle=$a|requestId=a-noise" },
                [ordered]@{ offset_seconds = 4; text = "$stamp START_PROMISE_REJECTED|bundle=$b|requestId=b4|summary=denied" }
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
                [ordered]@{ offset_seconds = 9; text = "$stamp VPN_CREATE_REJECTED|requestId=b6|phase=conflict|summary=active-A" }
            )
            '7' = @(
                [ordered]@{ offset_seconds = 1; text = "$stamp STOP_PROMISE_RESOLVED|bundle=$a|requestId=a6" },
                [ordered]@{ offset_seconds = 2; text = "$stamp VPN_ONDESTROY|requestId=a6" },
                [ordered]@{ offset_seconds = 3; text = "$stamp VPN_DESTROY_RESOLVED|requestId=a6|fdMarker=FD_CLOSED_CONFIRMED" },
                [ordered]@{ offset_seconds = 4; text = "$stamp VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
            )
        }
    }
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
    Assert-ManifestAndSeal $dryPaths.Evidence
    Assert-ProjectionChain $dryPaths.Evidence

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
    Assert-True ([double]$liveRecord.scenarios[3].observation.measured_coverage_after_ack_seconds -ge 60 -and $liveRecord.scenarios[3].observation.complete_window_observed) 'deny did not measure healthy coverage through ACK plus 60 seconds'
    Assert-True ($liveRecord.scenarios[3].reason -eq 'observable-B-request-rejection') 'cross-bundle pollution changed deny result'
    Assert-True (($liveRecord.scenarios[3].observation.events.text -join "`n") -notmatch '2098-12-31|VPN_ONCREATE.*requestId=b4') 'pre-anchor history entered the deny scenario'
    Assert-True (($liveRecord.scenarios[4].observation.events.text -join "`n") -notmatch '2098-12-31') 'pre-anchor requestId history entered scenario 5'
    Assert-True ($liveRecord.scenarios[4].settings_reallow_path.match -eq $true -and $liveRecord.scenarios[4].settings_reallow_path.actual -eq 'direct-system-activation' -and $liveRecord.scenarios[4].settings_reallow_path.policy -eq 'observation-only') 'scenario 5 path observation contract mismatch on matched path'
    Assert-True ($liveRecord.scenarios[0].first_baseline_query_covered -and $liveRecord.scenarios[0].install_completed_within_60_seconds) 'scenario 1 did not cover baseline query and installation within 60 seconds'
    Assert-True ([double]$liveRecord.scenarios[0].observation.measured_coverage_before_action_prompt_seconds -lt 0.5) 'scenario 1 counted READY latency into the action window'
    Assert-True ([double]$liveRecord.scenarios[0].install_elapsed_seconds -le 60) 'scenario 1 install window exceeded 60s after READY'
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
    Assert-True ($liveRecord.scenarios[5].a_accepted -eq $true -and $liveRecord.scenarios[5].reason -eq 'B-rejected-no-replacement-destroy-required') 'scenario 6 a_accepted or reason mismatch'
    Assert-True ($liveRecord.scenarios[6].post_cleanup_capture -eq $true -and $liveRecord.scenarios[6].post_cleanup_capture_name -eq 'scenario-7-post-cleanup') 'scenario 7 post-cleanup screenshot naming mismatch'
    Assert-True ($liveRecord.reviewer -eq 'pending' -and $liveRecord.reviewed_at -eq 'pending' -and $liveRecord.record_status -notmatch '^reviewed') 'runner wrote reviewed state'
    Assert-ManifestAndSeal $livePaths.Evidence
    Assert-ProjectionChain $livePaths.Evidence

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
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_REJECTED|requestId=b6|phase=conflict|summary=active-A" },
        [ordered]@{ offset_seconds = 10; text = "$('<DEVICE_OBSERVED_AT>') START_PROMISE_LATE_REJECTED|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4|summary=released-request" }
    )
    $m3NewStartPath = Write-JsonFixture 'simulation-m3-new-b-start.json' $m3NewStartFixture
    $m3NewStartPaths = New-CasePaths 'm3-new-b-start'
    $m3NewStartRun = Invoke-Runner $liveFreezePath $m3NewStartPaths.Evidence $m3NewStartPaths.Raw -FixturePath $m3NewStartPath
    Assert-True ($m3NewStartRun.ExitCode -eq 0) "M3 new B UI_START simulation crashed: $($m3NewStartRun.Text)"
    $m3NewStartRecord = Get-Content -LiteralPath (Join-Path $m3NewStartPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m3NewStartRecord.scenarios[5].result -eq 'pass' -and $m3NewStartRecord.scenarios[5].reason -eq 'B-rejected-no-replacement-destroy-required') 'M3 scenario 6 with new B UI_START did not pass with expected reason'
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
    Assert-True ($m3NoNewStartRun.ExitCode -eq 0) "M3 no new B UI_START simulation crashed: $($m3NoNewStartRun.Text)"
    $m3NoNewStartRecord = Get-Content -LiteralPath (Join-Path $m3NoNewStartPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m3NoNewStartRecord.scenarios[5].result -eq 'blocked' -and $m3NoNewStartRecord.scenarios[5].reason -eq 'no-new-B-UI_START') 'M3 scenario 6 without new B UI_START reason mismatch'
    Assert-True ($m3NoNewStartRecord.scenarios[5].a_accepted -eq $true -and $m3NoNewStartRecord.scenarios[5].b_accepted -eq $false) 'M3 scenario 6 a_accepted/b_accepted mismatch'

    Write-Host 'SELFTEST_PHASE=m4-inferred-s7-request-and-screenshot-naming'
    $m4InferredFixture = New-SimulationFixture
    $m4InferredFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpna|reason=operation-pending" }
    )
    $m4InferredFixture.scenario_events.'6' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpna|reason=operation-pending" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') UI_START_SKIPPED|bundle=cn.alfadb.netbird.e3physvpnb|reason=operation-pending" }
    )
    $m4InferredFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') STOP_PROMISE_RESOLVED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONDESTROY|requestId=a5" },
        [ordered]@{ offset_seconds = 4; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 5; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $m4InferredPath = Write-JsonFixture 'simulation-m4-inferred-s7.json' $m4InferredFixture
    $m4InferredPaths = New-CasePaths 'm4-inferred-s7'
    $m4InferredRun = Invoke-Runner $liveFreezePath $m4InferredPaths.Evidence $m4InferredPaths.Raw -FixturePath $m4InferredPath
    Assert-True ($m4InferredRun.ExitCode -eq 0) "M4 inferred S7 simulation crashed: $($m4InferredRun.Text)"
    $m4InferredRecord = Get-Content -LiteralPath (Join-Path $m4InferredPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m4InferredRecord.scenarios[6].request_id -eq 'a5' -and $m4InferredRecord.scenarios[6].reason -eq 'terminal-and-post-destroy-snapshot-confirmed') 'M4 scenario 7 did not infer requestId from real window events'
    Assert-True ($m4InferredRecord.scenarios[6].result -eq 'pass' -and $m4InferredRecord.scenarios[6].reason -notmatch 'requestId-missing') 'M4 inferred scenario 7 must not report requestId-missing'
    Assert-True ($m4InferredRecord.scenarios[6].post_cleanup_capture -eq $true -and $m4InferredRecord.scenarios[6].post_cleanup_capture_name -eq 'scenario-7-post-cleanup') 'M4 post-cleanup screenshot naming mismatch'
    $m4InferredScreens = @($m4InferredRecord.screenshot_reference | Where-Object { $_.name -eq 'scenario-7-post-cleanup' })
    Assert-True ($m4InferredScreens.Count -ge 1) 'M4 post-cleanup screenshot reference missing'

    $m4NoCleanupFixture = New-SimulationFixture
    $m4NoCleanupFixture.scenario_events.'7' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_STOP|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') STOP_PROMISE_RESOLVED|bundle=cn.alfadb.netbird.e3physvpna|requestId=a6" }
    )
    $m4NoCleanupPath = Write-JsonFixture 'simulation-m4-no-cleanup-s7.json' $m4NoCleanupFixture
    $m4NoCleanupPaths = New-CasePaths 'm4-no-cleanup-s7'
    $m4NoCleanupRun = Invoke-Runner $liveFreezePath $m4NoCleanupPaths.Evidence $m4NoCleanupPaths.Raw -FixturePath $m4NoCleanupPath
    Assert-True ($m4NoCleanupRun.ExitCode -eq 0) "M4 no-cleanup S7 simulation crashed: $($m4NoCleanupRun.Text)"
    $m4NoCleanupRecord = Get-Content -LiteralPath (Join-Path $m4NoCleanupPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($m4NoCleanupRecord.scenarios[6].result -eq 'blocked' -and $m4NoCleanupRecord.scenarios[6].reason -eq 'destroy-terminal-or-post-snapshot-missing') 'M4 scenario 7 without destroy terminal classified reason mismatch'
    Assert-True ($m4NoCleanupRecord.scenarios[6].post_cleanup_capture -eq $false -and $m4NoCleanupRecord.scenarios[6].post_cleanup_capture_name -eq 'scenario-7-final-state') 'M4 final-state screenshot naming mismatch'
    $m4NoCleanupScreens = @($m4NoCleanupRecord.screenshot_reference | Where-Object { $_.name -eq 'scenario-7-final-state' })
    $m4NoCleanupPostScreens = @($m4NoCleanupRecord.screenshot_reference | Where-Object { $_.name -eq 'scenario-7-post-cleanup' })
    Assert-True ($m4NoCleanupScreens.Count -ge 1 -and $m4NoCleanupPostScreens.Count -eq 0) 'M4 final-state screenshot naming contract mismatch'

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
    Assert-True ($deadRecord.scenarios[3].result -eq 'blocked' -and $deadRecord.scenarios[3].observation.capture_degraded -and -not $deadRecord.scenarios[3].full_window_after_ack) 'capture death allowed deny to fail open'
    Assert-True ($deadRecord.infrastructure_reason -eq 'hdc-usb-interruption') 'capture death did not set hdc-usb-interruption infrastructure reason'
    Assert-True ($null -eq $deadRecord.scenarios[4].PSObject.Properties['observation']) 'campaign continued into scenario 5 after capture death'

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
    Assert-True ($authCaptureFailRun.ExitCode -eq 0) "authorization capture failure crashed campaign: $($authCaptureFailRun.Text)"
    $authCaptureFailRecord = Get-Content -LiteralPath (Join-Path $authCaptureFailPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($authCaptureFailRecord.scenarios[1].result -eq 'blocked' -and $authCaptureFailRecord.scenarios[1].authorization_capture.status -eq 'degraded' -and $authCaptureFailRecord.scenarios[1].authorization_capture.result -eq 'blocked') 'authorization capture failure did not block scenario 2 with record reference'

    $lateFixture = New-SimulationFixture
    $lateFixture.scenario_events.'4' += [ordered]@{ offset_seconds = 64; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4" }
    $lateFixturePath = Write-JsonFixture 'simulation-late-b-create.json' $lateFixture
    $latePaths = New-CasePaths 'late-b-create'
    $lateRun = Invoke-Runner $liveFreezePath $latePaths.Evidence $latePaths.Raw -FixturePath $lateFixturePath
    Assert-True ($lateRun.ExitCode -eq 0) "late B create simulation crashed: $($lateRun.Text)"
    $lateRecord = Get-Content -LiteralPath (Join-Path $latePaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($lateRecord.scenarios[3].result -eq 'fail' -and $lateRecord.scenarios[3].reason -eq 'deny-created-B-vpn') 'trailing B create was not caught inside the measured window'

    $multiReqFixture = New-SimulationFixture
    $multiReqFixture.scenario_events.'4' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-primary" },
        [ordered]@{ offset_seconds = 2; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-secondary" },
        [ordered]@{ offset_seconds = 3; text = "$('<DEVICE_OBSERVED_AT>') VPN_ONCREATE|bundle=cn.alfadb.netbird.e3physvpnb|requestId=b4-secondary" }
    )
    $multiReqPath = Write-JsonFixture 'simulation-multi-b-requestid.json' $multiReqFixture
    $multiReqPaths = New-CasePaths 'multi-b-requestid'
    $multiReqRun = Invoke-Runner $liveFreezePath $multiReqPaths.Evidence $multiReqPaths.Raw -FixturePath $multiReqPath
    Assert-True ($multiReqRun.ExitCode -eq 0) "multi B requestId simulation crashed: $($multiReqRun.Text)"
    $multiReqRecord = Get-Content -LiteralPath (Join-Path $multiReqPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($multiReqRecord.scenarios[3].result -eq 'fail' -and $multiReqRecord.scenarios[3].reason -eq 'deny-created-B-vpn') 'secondary B requestId create was not failed'

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
    [void]$missingConfirmationFixture.operator.confirmations.Remove('FINAL-CLEANUP-CAPTURED')
    $missingConfirmationPath = Write-JsonFixture 'simulation-missing-confirmation.json' $missingConfirmationFixture
    $missingConfirmationPaths = New-CasePaths 'missing-confirmation'
    $missingConfirmationRun = Invoke-Runner $liveFreezePath $missingConfirmationPaths.Evidence $missingConfirmationPaths.Raw -FixturePath $missingConfirmationPath
    $missingConfirmationRecord = Get-Content -LiteralPath (Join-Path $missingConfirmationPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($missingConfirmationRecord.scenarios[6].result -eq 'blocked' -and -not $missingConfirmationRecord.scenarios[6].visible_cleanup_confirmed) 'missing simulation confirmation did not default false'

    $strictBooleanFixture = New-SimulationFixture
    $strictBooleanFixture.operator.confirmations.'DENY-SCREEN-CAPTURED' = 'true'
    $strictBooleanPath = Write-JsonFixture 'simulation-strict-boolean.json' $strictBooleanFixture
    $strictBooleanPaths = New-CasePaths 'strict-boolean'
    $strictBooleanRun = Invoke-Runner $liveFreezePath $strictBooleanPaths.Evidence $strictBooleanPaths.Raw -FixturePath $strictBooleanPath
    Assert-True ($strictBooleanRun.ExitCode -ne 0) 'string simulation Boolean was accepted'
    $strictBooleanRecord = Get-Content -LiteralPath (Join-Path $strictBooleanPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($strictBooleanRecord.record_status -eq 'blocked' -and $strictBooleanRecord.verdict -eq 'blocked' -and -not $strictBooleanRecord.is_evidence) 'strict Boolean failure escaped simulation blocked contract'

    Write-Host 'SELFTEST_PHASE=settings-reallow-path-observation-only'
    $pathMismatchFixture = New-SimulationFixture
    $pathMismatchFixture.operator.confirmations.'PATH-ACTUAL-DIRECT-SYSTEM-ACTIVATION' = $false
    $pathMismatchFixture.operator.confirmations.'PATH-ACTUAL-SYSTEM-REAUTHORIZATION-UI' = $true
    $pathMismatchPath = Write-JsonFixture 'simulation-path-mismatch.json' $pathMismatchFixture
    $pathMismatchPaths = New-CasePaths 'path-mismatch-pass'
    $pathMismatchRun = Invoke-Runner $liveFreezePath $pathMismatchPaths.Evidence $pathMismatchPaths.Raw -FixturePath $pathMismatchPath
    Assert-True ($pathMismatchRun.ExitCode -eq 0) "path mismatch with complete functional chain crashed: $($pathMismatchRun.Text)"
    $pathMismatchRecord = Get-Content -LiteralPath (Join-Path $pathMismatchPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($pathMismatchRecord.scenarios[4].result -eq 'pass') 'predicted-direct actual-reauth with complete functional chain did not pass'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.match -eq $false) 'path mismatch was not recorded as match=false'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.expected -eq 'direct-system-activation' -and $pathMismatchRecord.scenarios[4].settings_reallow_path.actual -eq 'system-reauthorization-UI') 'path observation expected/actual fields wrong'
    Assert-True ($pathMismatchRecord.scenarios[4].settings_reallow_path.observation -eq 'actual-path-deviated-from-expected-observation-only' -and $pathMismatchRecord.scenarios[4].settings_reallow_path.policy -eq 'observation-only') 'path observation metadata wrong'
    Assert-True ($pathMismatchRecord.scenario_aggregation.measured_scenario_overall -eq 'pass') 'path mismatch alone blocked overall aggregation'

    $missingFunctionalFixture = New-SimulationFixture
    $missingFunctionalFixture.scenario_events.'5' = @(
        [ordered]@{ offset_seconds = 1; text = "$('<DEVICE_OBSERVED_AT>') UI_START|bundle=cn.alfadb.netbird.e3physvpna|requestId=a5" },
        [ordered]@{ offset_seconds = 8; text = "$('<DEVICE_OBSERVED_AT>') VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED" },
        [ordered]@{ offset_seconds = 9; text = "$('<DEVICE_OBSERVED_AT>') VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED" }
    )
    $missingFunctionalPath = Write-JsonFixture 'simulation-missing-functional.json' $missingFunctionalFixture
    $missingFunctionalPaths = New-CasePaths 'missing-functional-marker'
    $missingFunctionalRun = Invoke-Runner $liveFreezePath $missingFunctionalPaths.Evidence $missingFunctionalPaths.Raw -FixturePath $missingFunctionalPath
    Assert-True ($missingFunctionalRun.ExitCode -eq 0) "missing functional marker simulation crashed: $($missingFunctionalRun.Text)"
    $missingFunctionalRecord = Get-Content -LiteralPath (Join-Path $missingFunctionalPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($missingFunctionalRecord.scenarios[4].result -ne 'pass') 'missing VPN_ONCREATE/create-fd markers still passed scenario 5'
    Assert-True ($missingFunctionalRecord.scenarios[4].assertions.vpn_on_create -eq 'blocked' -or $missingFunctionalRecord.scenarios[4].assertions.vpn_connection_create_fd -eq 'blocked') 'missing functional markers were not recorded as blocked assertions'

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
    Assert-True ($tamperRun.ExitCode -eq 0) 'simulation transcript tamper escaped the blocked-only contract'
    $tamperRecord = Get-Content -LiteralPath (Join-Path $tamperPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ($tamperRecord.record_status -eq 'blocked' -and $tamperRecord.verdict -eq 'blocked' -and 'transcript-json-invalid' -in @($tamperRecord.integrity_violations)) 'hash-chain tamper was not detected'
    Assert-True (@($liveRecord.integrity_violations).Count -eq 0) 'baseline live simulation integrity must remain empty after tamper isolation'

    $payloadTamperFixturePath = Write-JsonFixture 'simulation-payload-tamper.json' (New-SimulationFixture -TamperPayload $true)
    $payloadTamperPaths = New-CasePaths 'payload-tamper'
    $payloadTamperRun = Invoke-Runner $liveFreezePath $payloadTamperPaths.Evidence $payloadTamperPaths.Raw -FixturePath $payloadTamperFixturePath
    Assert-True ($payloadTamperRun.ExitCode -eq 0) 'simulation payload tamper escaped the blocked-only contract'
    $payloadTamperRecord = Get-Content -LiteralPath (Join-Path $payloadTamperPaths.Evidence 'scenario-results.json') -Raw | ConvertFrom-Json -Depth 60
    Assert-True ('transcript-payload-canonical-mismatch' -in @($payloadTamperRecord.integrity_violations) -and $payloadTamperRecord.record_status -eq 'blocked') 'human-readable payload tamper was not detected'
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
