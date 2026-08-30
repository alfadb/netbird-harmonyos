<#
.SYNOPSIS
G0-PHYS-PROBE campaign runner (g0-phys-probe-campaign.ps1 1.0.0).

.DESCRIPTION
PowerShell campaign runner for the G0 spike: measures whether the frozen stock
(zero-patch) Go 1.25.12 arm64 c-shared probe library (libgoprobe.so inside
cn.alfadb.netbird.g0probe) is accepted by the physical HarmonyOS device
loader, plus a minimal runtime smoke. Behavioral mirror of the normative
Python runner spikes/g0-go-arm64-phys-hap/g0-phys-probe-campaign.py:

  * exact-whitelist HDC invocation (15 operations, audit argv frozen verbatim,
    placeholders <PHYS_1_TARGET> / <HAP_G0> never leak the real target),
  * strict freeze manifest schema (exact key sets, no missing / extra keys),
  * single-use out-of-repository double-file confirmation record
    (JSON tmp + .sha256 tmp recomputed, atomic rename, companion last),
  * host HDC process count via absolute /usr/bin/ps -eo comm=,args=
    comparing the FIRST column only,
  * evidence outputs: scenario-results.json + hash-manifest.json +
    campaign-seal.json + line-chained transcript.redacted.jsonl, raw artifacts
    under RawRoot.

G0 has NO scenarios/operator/layout logic (single S1 flow). The
marker -> verdict mapping is pre-registered in
docs/g0-go-arm64-physical-probe.md and implemented verbatim:

  verdict=PASS & ok=true & stage=complete & hello=42 & runtimeBytes=1048576 -> pass
  verdict=FAIL & stage=dlopen & loaderError non-empty                       -> blocked
    (a VALID measured result: the loader rejected the library; loaderErrno /
     loaderError are recorded verbatim)
  anything else / 0 markers / >1 markers                                    -> blocked
    (reason marker-missing | marker-ambiguous | drift)
  integrity violations (non-whitelist command attempt, freeze mismatch,
  dirty worktree)                                                           -> invalid
    (highest priority)

PowerShell 7 standard library only; no external modules; no network; no real
hdc is ever touched by -SelfTest. -DryRun NEVER executes or hashes the frozen
hdc path: the HDC layer is simulated in-process (host-only sandbox), so no
Python runtime is required on the Windows/Linux host.

Exit codes
    -Version / -SelfTest                         0 (selftest 1 on failure)
    -DryRun / -Live  completed flow (sealed)     0, final line VERDICT=<verdict>
    any pre-flight / validation / runner error   1 (no evidence, no seal)
    -TargetBindingConfirm pass                   0
    -TargetBindingConfirm pre-record gate        1 (no record written)
    -TargetBindingConfirm blocked                2 (blocked record written)
#>

[CmdletBinding(DefaultParameterSetName = 'DryRun')]
param(
    [Parameter(ParameterSetName = 'Version')] [switch] $Version,
    [Parameter(ParameterSetName = 'SelfTest')] [switch] $SelfTest,
    [Parameter(ParameterSetName = 'TargetBindingConfirm')] [switch] $TargetBindingConfirm,
    [Parameter(ParameterSetName = 'DryRun')] [switch] $DryRun,
    [Parameter(ParameterSetName = 'Live')] [switch] $Live,
    [Parameter(ParameterSetName = 'TargetBindingConfirm')]
    [Parameter(ParameterSetName = 'DryRun')]
    [Parameter(ParameterSetName = 'Live')]
    [string] $Freeze,
    [Parameter(ParameterSetName = 'TargetBindingConfirm')] [string] $ConfirmationRecord
)

Set-StrictMode -Version Latest

# =====================================================================
# Runner exception types (RuntimeError / PreRecordGateError / CampaignBlocked)
# =====================================================================

class G0RunnerError : System.Exception {
    G0RunnerError([string] $message) : base($message) { }
}

class G0PreRecordGateError : System.Exception {
    G0PreRecordGateError([string] $message) : base($message) { }
}

class G0BlockedError : System.Exception {
    [string] $Reason
    G0BlockedError([string] $reason) : base($reason) { $this.Reason = $reason }
}

function New-G0OrdinalMap {
    # Case-sensitive generic dictionary (Python dict semantics: exact keys).
    return [System.Collections.Generic.Dictionary[string, object]]::new([System.StringComparer]::Ordinal)
}

function New-G0OrdinalStringMap {
    return [System.Collections.Generic.Dictionary[string, string]]::new([System.StringComparer]::Ordinal)
}

# =====================================================================
# Section 0: Constants and script-scope state
# =====================================================================

$script:RunnerVersion = '1.0.0'
$script:RunnerPath = $PSCommandPath
$script:RunnerDir = $PSScriptRoot
$script:Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$script:Bundle = 'cn.alfadb.netbird.g0probe'
$script:Ability = 'EntryAbility'
$script:Module = 'entry'
$script:Staging = '/data/local/tmp/netbird-g0'
$script:HilogTag = 'G0GoProbe'
$script:HapPlaceholder = '<HAP_G0>'
$script:TargetPlaceholder = '<PHYS_1_TARGET>'
$script:ScenarioWindowSeconds = 60

# The current authorization fixes one AUTH, one candidate triple, and
# attempt=initial (ADJ discipline mirrored from E3 C6); retries need new
# governance and new IDs.
$script:AuthId = 'AUTH-G0PHYS1API26-20260830-0001'
$script:CampaignId = 'G0-PHYS-PROBE-20260830-0001'
$script:EvidenceId = 'EV-G0PHYS1API26-20260830-0001'

$script:HdcTimeoutSeconds = 20
$script:RawHilogGraceSeconds = 10

# DryRun scripted in-process fake hdc behaviours (environment G0_DRYRUN_SCRIPT).
# 'install-fails' drives the InstallHap success-marker-missing path so the
# G0BlockedError -> sealed-blocked ending is exercised (review MAJOR-2).
$script:DryRunScripts = [string[]] @('pass', 'dlopen-rejected', 'install-fails')
$script:DryRunTargetSentinel = 'DRYRUN-LOCAL-TARGET'
$script:DryRunNonEvidenceReason = 'host-only dry-run against the sandboxed fake hdc; no physical-device evidence'
$script:DlopenLoaderError = 'initial-exec TLS resolves to dynamic definition'

# Pre-registered marker -> verdict mapping values.
$script:MarkerToken = 'G0_RESULT'
$script:PassFields = New-G0OrdinalStringMap
foreach ($pair in @(
    @('verdict', 'PASS'), @('ok', 'true'), @('stage', 'complete'),
    @('hello', '42'), @('runtimeBytes', '1048576'))) {
    $script:PassFields[$pair[0]] = $pair[1]
}

# Mutable script-scope state (mirrors the Python module globals).
$script:ActualTarget = $null
$script:FreezeManifest = $null
$script:ExecutionMode = $null
$script:IsEvidence = $false
$script:ProjectionTranscript = $null
$script:TranscriptIndex = 0
$script:TranscriptPreviousHash = ('0' * 64)

# =====================================================================
# Section 1: Base utilities (design unit U1)
# =====================================================================

function Write-G0Out {
    param([string] $Text)
    [System.Console]::Out.WriteLine($Text)
}

function Write-G0Err {
    param([string] $Text)
    [System.Console]::Error.WriteLine($Text)
}

function Get-G0Sha256Text {
    # Lowercase hex SHA-256 of UTF-8 bytes.
    param([string] $Text)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $script:Utf8NoBom.GetBytes($Text)
        $hash = $sha.ComputeHash($bytes)
        return ([System.BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Get-G0Sha256File {
    # Lowercase hex SHA-256 of file bytes.
    param([string] $Path)
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            $hash = $sha.ComputeHash($stream)
            return ([System.BitConverter]::ToString($hash)).Replace('-', '').ToLowerInvariant()
        } finally {
            $stream.Dispose()
        }
    } finally {
        $sha.Dispose()
    }
}

function Get-G0NormalizedPath {
    # Absolute path with trailing separators trimmed (Python normalize_path).
    param([string] $Path)
    $full = [System.IO.Path]::GetFullPath($Path)
    return $full.TrimEnd('/', '\')
}

function Test-G0UnderPath {
    # True when candidate == parent or candidate is inside parent
    # (case-sensitive POSIX semantics, Python is_under_path).
    param([string] $Candidate, [string] $Parent)
    $c = (Get-G0NormalizedPath $Candidate) + [System.IO.Path]::DirectorySeparatorChar
    $p = (Get-G0NormalizedPath $Parent) + [System.IO.Path]::DirectorySeparatorChar
    return $c.StartsWith($p, [System.StringComparison]::Ordinal)
}

function Test-G0Sha256Hex {
    # 64 lowercase hex chars only.
    param($Value)
    if ($null -eq $Value -or -not ($Value -is [string])) { return $false }
    return $Value -cmatch '^[0-9a-f]{64}$'
}

function Test-G0Sha1Hex {
    # 40 lowercase hex chars only (repository commit SHA).
    param($Value)
    if ($null -eq $Value -or -not ($Value -is [string])) { return $false }
    return $Value -cmatch '^[0-9a-f]{40}$'
}

function Test-G0JsonInteger {
    # JSON integer: int but never bool.
    param($Value)
    if ($Value -is [bool]) { return $false }
    return ($Value -is [int] -or $Value -is [long] -or $Value -is [int64] -or $Value -is [int32])
}

function Assert-G0ExactKeys {
    # Strict-schema gate: missing AND extra keys both fail.
    param($Obj, [string[]] $ExpectedKeys, [string] $Label)
    if ($null -eq $Obj -or -not ($Obj -is [System.Management.Automation.PSCustomObject])) {
        throw [G0RunnerError]::new(('{0} must be a JSON object' -f $Label))
    }
    $actualKeys = [string[]] @($Obj.PSObject.Properties.Name)
    $missing = [System.Collections.Generic.List[string]]::new()
    foreach ($key in $ExpectedKeys) {
        if ($actualKeys -cnotcontains $key) { $missing.Add($key) }
    }
    $extra = [System.Collections.Generic.List[string]]::new()
    foreach ($key in $actualKeys) {
        if ($ExpectedKeys -cnotcontains $key) { $extra.Add($key) }
    }
    $missing.Sort([System.StringComparer]::Ordinal)
    $extra.Sort([System.StringComparer]::Ordinal)
    if ($missing.Count -gt 0) {
        throw [G0RunnerError]::new(('{0} missing key(s): {1}' -f $Label, ($missing -join ', ')))
    }
    if ($extra.Count -gt 0) {
        throw [G0RunnerError]::new(('{0} unknown key(s): {1}' -f $Label, ($extra -join ', ')))
    }
}

function Get-G0ObjectValue {
    # Case-SENSITIVE property lookup on a parsed JSON object (Python dict.get).
    # The comma operator keeps single-element array values intact through the
    # pipeline (PowerShell would otherwise unroll them to their lone element).
    param($Obj, [string] $Name)
    if ($null -eq $Obj) { return $null }
    foreach ($property in $Obj.PSObject.Properties) {
        if ([string]::Equals($property.Name, $Name, [System.StringComparison]::Ordinal)) {
            return , $property.Value
        }
    }
    return $null
}

function Assert-G0RequireNonEmptyStr {
    param($Obj, [string] $Key, [string] $Label)
    $value = Get-G0ObjectValue $Obj $Key
    if (-not ($value -is [string]) -or [string]::IsNullOrWhiteSpace($value)) {
        throw [G0RunnerError]::new(('{0}.{1} must be a non-empty string' -f $Label, $Key))
    }
    return $value
}

function Assert-G0RequireSha256 {
    param($Obj, [string] $Key, [string] $Label)
    $value = Get-G0ObjectValue $Obj $Key
    if (-not (Test-G0Sha256Hex $value)) {
        throw [G0RunnerError]::new(('{0}.{1} must be a 64-hex sha256 string' -f $Label, $Key))
    }
    return $value
}

function Assert-G0RequireAbsPath {
    param($Obj, [string] $Key, [string] $Label)
    $value = Get-G0ObjectValue $Obj $Key
    if (-not ($value -is [string]) -or -not [System.IO.Path]::IsPathRooted($value)) {
        throw [G0RunnerError]::new(('{0}.{1} must be an absolute path string' -f $Label, $Key))
    }
    return $value
}

function Assert-G0Constant {
    param($Obj, [string] $Key, [string] $Expected, [string] $Label)
    $value = Get-G0ObjectValue $Obj $Key
    if (([string] $value) -cne $Expected) {
        throw [G0RunnerError]::new(("{0}.{1} must be exactly '{2}' (got '{3}')" -f $Label, $Key, $Expected, [string] $value))
    }
    return $value
}

function Assert-G0FileHash {
    # Hash a real file against an expected sha256 (Python assert_file_hash).
    param([string] $Label, [string] $Path, [string] $Expected)
    if (-not (Test-G0Sha256Hex $Expected)) {
        throw [G0RunnerError]::new(('{0} sha256 must be a final 64-hex SHA-256' -f $Label))
    }
    if (-not [System.IO.File]::Exists($Path)) {
        throw [G0RunnerError]::new(('{0} file missing: hash check impossible' -f $Label))
    }
    $actual = Get-G0Sha256File $Path
    if ($actual -cne $Expected.ToLowerInvariant()) {
        throw [G0RunnerError]::new(('{0} sha256 mismatch: file bytes do not match the frozen value' -f $Label))
    }
}

function Get-G0NowOffset {
    # Fixed +08:00 offset (deterministic, matches the confirmation record rule).
    return [DateTimeOffset]::Now.ToOffset([TimeSpan]::FromHours(8))
}

function Format-G0Iso8601 {
    # ISO-8601 timestamp with the fixed +08:00 offset.
    param([DateTimeOffset] $Stamp)
    return $Stamp.ToString('yyyy-MM-ddTHH:mm:ss.fffzzz', [System.Globalization.CultureInfo]::InvariantCulture)
}

function Get-G0NowIso {
    return Format-G0Iso8601 (Get-G0NowOffset)
}

function ConvertTo-G0JsonString {
    param([string] $Text)
    $builder = [System.Text.StringBuilder]::new($Text.Length + 2)
    [void] $builder.Append('"')
    foreach ($ch in $Text.ToCharArray()) {
        switch ($ch) {
            '"' { [void] $builder.Append('\"') }
            '\' { [void] $builder.Append('\\') }
            "`b" { [void] $builder.Append('\b') }
            "`f" { [void] $builder.Append('\f') }
            "`n" { [void] $builder.Append('\n') }
            "`r" { [void] $builder.Append('\r') }
            "`t" { [void] $builder.Append('\t') }
            default {
                if ([int] $ch -lt 0x20) {
                    [void] $builder.Append(('\u{0:x4}' -f [int] $ch))
                } else {
                    [void] $builder.Append($ch)
                }
            }
        }
    }
    [void] $builder.Append('"')
    return $builder.ToString()
}

function ConvertTo-G0CanonicalJson {
    # Deterministic compact serialization used for transcript chain hashing
    # and record bytes; keys sorted with ordinal (code point) order.
    param($Value)
    if ($null -eq $Value) { return 'null' }
    if ($Value -is [bool]) { if ($Value) { return 'true' } else { return 'false' } }
    if ($Value -is [string]) { return (ConvertTo-G0JsonString $Value) }
    if ($Value -is [char]) { return (ConvertTo-G0JsonString [string] $Value) }
    if ($Value -is [int] -or $Value -is [long] -or $Value -is [int64] -or $Value -is [int32] -or $Value -is [uint32] -or $Value -is [uint64]) {
        return $Value.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    }
    if ($Value -is [double] -or $Value -is [single] -or $Value -is [decimal]) {
        return $Value.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    }
    if ($Value -is [System.Collections.IDictionary]) {
        $keys = [System.Collections.Generic.List[string]]::new()
        foreach ($key in @($Value.Keys)) { $keys.Add([string] $key) }
        $keys.Sort([System.StringComparer]::Ordinal)
        $parts = [System.Collections.Generic.List[string]]::new()
        foreach ($key in $keys) {
            $parts.Add((ConvertTo-G0JsonString $key) + ':' + (ConvertTo-G0CanonicalJson $Value[$key]))
        }
        return '{' + ($parts -join ',') + '}'
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $properties = [System.Collections.Generic.List[System.Management.Automation.PSPropertyInfo]]::new()
        foreach ($property in $Value.PSObject.Properties) { $properties.Add($property) }
        $properties.Sort([System.Comparison[System.Management.Automation.PSPropertyInfo]] {
            param($a, $b)
            return [string]::CompareOrdinal($a.Name, $b.Name)
        })
        $parts = [System.Collections.Generic.List[string]]::new()
        foreach ($property in $properties) {
            $parts.Add((ConvertTo-G0JsonString $property.Name) + ':' + (ConvertTo-G0CanonicalJson $property.Value))
        }
        return '{' + ($parts -join ',') + '}'
    }
    if ($Value -is [System.Collections.IEnumerable]) {
        $parts = [System.Collections.Generic.List[string]]::new()
        foreach ($item in $Value) { $parts.Add((ConvertTo-G0CanonicalJson $item)) }
        return '[' + ($parts -join ',') + ']'
    }
    throw [G0RunnerError]::new(('cannot canonicalize value of type {0}' -f $Value.GetType().FullName))
}

function ConvertTo-G0PrettyJson {
    # json.dumps(obj, indent=2) shape: multiline, sorted keys, no trailing spaces.
    param($Value, [int] $Indent = 0)
    $pad = (' ' * $Indent)
    $padInner = (' ' * ($Indent + 2))
    if ($null -eq $Value) { return 'null' }
    if ($Value -is [bool]) { if ($Value) { return 'true' } else { return 'false' } }
    if ($Value -is [string]) { return (ConvertTo-G0JsonString $Value) }
    if ($Value -is [char]) { return (ConvertTo-G0JsonString [string] $Value) }
    if ($Value -is [int] -or $Value -is [long] -or $Value -is [int64] -or $Value -is [int32] -or $Value -is [uint32] -or $Value -is [uint64]) {
        return $Value.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    }
    if ($Value -is [double] -or $Value -is [single] -or $Value -is [decimal]) {
        return $Value.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    }
    if ($Value -is [System.Collections.IDictionary]) {
        if (@($Value.Keys).Count -eq 0) { return '{}' }
        $keys = [System.Collections.Generic.List[string]]::new()
        foreach ($key in @($Value.Keys)) { $keys.Add([string] $key) }
        $keys.Sort([System.StringComparer]::Ordinal)
        $parts = [System.Collections.Generic.List[string]]::new()
        foreach ($key in $keys) {
            $parts.Add($padInner + (ConvertTo-G0JsonString $key) + ': ' + (ConvertTo-G0PrettyJson $Value[$key] ($Indent + 2)))
        }
        return "{`n" + ($parts -join ",`n") + "`n" + $pad + '}'
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $properties = [System.Collections.Generic.List[System.Management.Automation.PSPropertyInfo]]::new()
        foreach ($property in $Value.PSObject.Properties) { $properties.Add($property) }
        $properties.Sort([System.Comparison[System.Management.Automation.PSPropertyInfo]] {
            param($a, $b)
            return [string]::CompareOrdinal($a.Name, $b.Name)
        })
        if ($properties.Count -eq 0) { return '{}' }
        $parts = [System.Collections.Generic.List[string]]::new()
        foreach ($property in $properties) {
            $parts.Add($padInner + (ConvertTo-G0JsonString $property.Name) + ': ' + (ConvertTo-G0PrettyJson $property.Value ($Indent + 2)))
        }
        return "{`n" + ($parts -join ",`n") + "`n" + $pad + '}'
    }
    if ($Value -is [System.Collections.IEnumerable]) {
        $items = [System.Collections.Generic.List[string]]::new()
        foreach ($item in $Value) { $items.Add($padInner + (ConvertTo-G0PrettyJson $item ($Indent + 2))) }
        if ($items.Count -eq 0) { return '[]' }
        return "[`n" + ($items -join ",`n") + "`n" + $pad + ']'
    }
    throw [G0RunnerError]::new(('cannot serialize value of type {0}' -f $Value.GetType().FullName))
}

function Write-G0TextUtf8NoBom {
    param([string] $Path, [string] $Text)
    [System.IO.File]::WriteAllText($Path, $Text, $script:Utf8NoBom)
}

function Write-G0TextUtf8NoBomSynced {
    # UTF-8 without BOM + flush-to-disk (fsync parity for the record pair).
    param([string] $Path, [string] $Text)
    $bytes = $script:Utf8NoBom.GetBytes($Text)
    $stream = [System.IO.FileStream]::new($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write, [System.IO.FileShare]::None)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
}

function Read-G0TextUtf8Sig {
    # Read with BOM detection (File.ReadAllText auto-detects/strips BOM).
    param([string] $Path)
    return [System.IO.File]::ReadAllText($Path)
}

function Write-G0JsonFile {
    # Indented UTF-8 JSON + trailing newline (Python write_json_file).
    param([string] $Path, $Value)
    Write-G0TextUtf8NoBom $Path ((ConvertTo-G0PrettyJson $Value) + "`n")
}

function Get-G0TextLines {
    # Python str.splitlines() parity via StringReader (no trailing empty line).
    param([string] $Text)
    $lines = [System.Collections.Generic.List[string]]::new()
    if ([string]::IsNullOrEmpty($Text)) { return $lines }
    $reader = [System.IO.StringReader]::new($Text)
    try {
        while ($true) {
            $line = $reader.ReadLine()
            if ($null -eq $line) { break }
            $lines.Add($line)
        }
    } finally {
        $reader.Dispose()
    }
    return $lines
}

function Get-G0Utf8ByteCount {
    param([string] $Text)
    return [System.Text.Encoding]::UTF8.GetByteCount($Text)
}

function Resolve-G0RepositoryRoot {
    # git -C <runner dir> rev-parse --show-toplevel; best-effort: returns
    # $null when git is unavailable so host-only DryRun keeps working. -Live
    # hard-requires the root in its own preflight.
    try {
        $output = & git -C $script:RunnerDir 'rev-parse' '--show-toplevel' 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        $rootText = ((@($output) | Select-Object -First 1) -as [string])
        if ([string]::IsNullOrWhiteSpace($rootText)) { return $null }
        return Get-G0NormalizedPath $rootText
    } catch {
        return $null
    }
}

function Get-G0GitStatusPorcelain {
    # Exactly `git status --porcelain` (empty output == clean worktree).
    param([string] $RepoRootPath)
    try {
        $output = & git -C $RepoRootPath 'status' '--porcelain' 2>$null
    } catch {
        throw [G0RunnerError]::new('unable to read repository state')
    }
    if ($LASTEXITCODE -ne 0) {
        throw [G0RunnerError]::new('unable to read repository state')
    }
    return (@($output) -join "`n")
}

# =====================================================================
# Section 2: Sensitive information protection (design unit U1)
# =====================================================================

function Protect-G0SensitiveText {
    # The real target token never enters any in-repository or evidence
    # output: every free-text projection replaces it (case-insensitive).
    param([string] $Text)
    if ($null -eq $Text) { return '' }
    $safe = [string] $Text
    if ($null -ne $script:ActualTarget -and -not [string]::IsNullOrWhiteSpace($script:ActualTarget)) {
        $pattern = [System.Text.RegularExpressions.Regex]::Escape($script:ActualTarget)
        $safe = [System.Text.RegularExpressions.Regex]::Replace(
            $safe, $pattern, '<REDACTED_TARGET>',
            [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    return $safe
}

function Protect-G0SensitiveData {
    # Recursive dict/list walk applying Protect-G0SensitiveText to strings.
    # Array returns are wrapped with the comma operator: PowerShell unwraps
    # pipeline/return values, which collapsed single-element arrays to scalars
    # and empty arrays to $null, breaking evidence-structure parity with the
    # Python runner (review MAJOR-1). `,$out` hands the caller the List itself.
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) { return (Protect-G0SensitiveText $Value) }
    if ($Value -is [System.Collections.IDictionary]) {
        $out = @{}
        foreach ($key in @($Value.Keys)) {
            $out[[string] $key] = Protect-G0SensitiveData $Value[$key]
        }
        return $out
    }
    if ($Value -is [System.Collections.IEnumerable]) {
        $out = [System.Collections.Generic.List[object]]::new()
        foreach ($item in $Value) { $out.Add((Protect-G0SensitiveData $item)) }
        return , $out
    }
    return $Value
}

# =====================================================================
# Section 3: Freeze validation (design unit U2) - strict schema
# =====================================================================

$script:FreezeTopLevelKeys = [string[]] @(
    'schema_version', 'authorization_id', 'campaign_id', 'evidence_id', 'attempt', 'plan_status',
    'code_sha', 'runner_py_sha256', 'runner_ps1_sha256', 'selftest_py_sha256',
    'selftest_ps1_sha256', 'hdc', 'bundle', 'ability', 'module', 'staging',
    'hilog_tag', 'scenario_window_seconds', 'target_tuple', 'artifacts',
    'elf_profile', 'evidence_roots', 'raw_roots', 'confirmation', 'review',
    'operator', 'orchestrator'
)

$script:ExpectedTargetTuple = New-G0OrdinalStringMap
foreach ($pair in @(
    @('distribution', 'HarmonyOS'),
    @('device_model', 'PLA-AL10'),
    @('full_system_build', 'PLA-AL10 7.0.0.102(SP8C00E102R7P3)'),
    @('api', '26'),
    @('kernel_architecture', 'aarch64'),
    @('app_abi', 'arm64-v8a'))) {
    $script:ExpectedTargetTuple[$pair[0]] = $pair[1]
}

$script:ArtifactKeys = [string[]] @(
    'hap_path', 'hap_sha256', 'profile_sha256', 'certificate_sha256',
    'libgoprobe_sha256', 'libgoloader_sha256'
)

$script:ConfirmationKeys = [string[]] @('status', 'record_path', 'record_sha256', 'authorization_id')
$script:ReviewKeys = [string[]] @('status', 'record_path', 'record_sha256')
$script:GovernanceStatuses = [string[]] @('pass', 'pending')

function Assert-G0RequireSiblingHash {
    # Required 64-hex hash recomputed against the sibling file in this
    # checkout. A freeze may no longer leave the parity artifacts unbound
    # (review round-2 NEW-MINOR-1): both runners enforce all four
    # implementation hashes before any mode runs.
    param($Obj, [string] $Key, [string] $SiblingPath, [string] $Label)
    $declared = Assert-G0RequireSha256 $Obj $Key $Label
    if (-not [System.IO.File]::Exists($SiblingPath)) {
        throw [G0RunnerError]::new(('{0}.{1}: parity sibling file missing for recompute' -f $Label, $Key))
    }
    $actual = (Get-G0Sha256File $SiblingPath).ToLowerInvariant()
    if ($actual -cne $declared.ToLowerInvariant()) {
        throw [G0RunnerError]::new(('{0}.{1} does not match the sibling file in this checkout (recomputed {2})' -f $Label, $Key, $actual))
    }
    return $declared
}

function Assert-G0FreezeSchema {
    # Strict freeze schema: exact key sets everywhere; wrong type, missing
    # key, or extra key fails. -Live only accepts plan_status=ready; -DryRun
    # and -TargetBindingConfirm accept blocked or ready.
    param($Freeze, [string] $Mode, $RepoRootPath)
    Assert-G0ExactKeys -Obj $Freeze -ExpectedKeys $script:FreezeTopLevelKeys -Label 'freeze'
    if ((Get-G0ObjectValue $Freeze 'schema_version') -ne 1) {
        throw [G0RunnerError]::new('freeze.schema_version must be the JSON integer 1')
    }
    Assert-G0Constant $Freeze 'authorization_id' $script:AuthId 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'campaign_id' $script:CampaignId 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'evidence_id' $script:EvidenceId 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'attempt' 'initial' 'freeze' | Out-Null
    $planStatus = Get-G0ObjectValue $Freeze 'plan_status'
    if (([string] $planStatus) -cne 'blocked' -and ([string] $planStatus) -cne 'ready') {
        throw [G0RunnerError]::new('freeze.plan_status must be blocked or ready')
    }
    if ($Mode -ceq 'live' -and ([string] $planStatus) -cne 'ready') {
        throw [G0RunnerError]::new('Live requires plan_status ready')
    }
    $codeSha = Get-G0ObjectValue $Freeze 'code_sha'
    if (-not (Test-G0Sha1Hex $codeSha)) {
        throw [G0RunnerError]::new('freeze.code_sha must be a 40-hex repository commit sha')
    }
    # All four implementation hashes are REQUIRED and recomputed against
    # their files in this checkout (review round-2 NEW-MINOR-1): no
    # empty-string escape on this runner either. runner_ps1_sha256 is this
    # runner's own bytes; runner_py/selftest hashes are Python siblings.
    $spikeDir = [System.IO.Path]::GetDirectoryName([string] $script:RunnerPath)
    Assert-G0RequireSiblingHash $Freeze 'runner_py_sha256' ([System.IO.Path]::Combine($spikeDir, 'g0-phys-probe-campaign.py')) 'freeze' | Out-Null
    Assert-G0RequireSiblingHash $Freeze 'runner_ps1_sha256' ([System.IO.Path]::Combine($spikeDir, 'g0-phys-probe-campaign.ps1')) 'freeze' | Out-Null
    Assert-G0RequireSiblingHash $Freeze 'selftest_py_sha256' ([System.IO.Path]::Combine($spikeDir, 'tests', 'g0-runner-selftest.py')) 'freeze' | Out-Null
    Assert-G0RequireSiblingHash $Freeze 'selftest_ps1_sha256' ([System.IO.Path]::Combine($spikeDir, 'tests', 'g0-runner-selftest.ps1')) 'freeze' | Out-Null

    $hdc = Get-G0ObjectValue $Freeze 'hdc'
    Assert-G0ExactKeys -Obj $hdc -ExpectedKeys ([string[]] @('path', 'sha256', 'version')) -Label 'freeze.hdc'
    Assert-G0RequireAbsPath $hdc 'path' 'freeze.hdc' | Out-Null
    Assert-G0RequireSha256 $hdc 'sha256' 'freeze.hdc' | Out-Null
    Assert-G0RequireNonEmptyStr $hdc 'version' 'freeze.hdc' | Out-Null

    Assert-G0Constant $Freeze 'bundle' $script:Bundle 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'ability' $script:Ability 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'module' $script:Module 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'staging' $script:Staging 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'hilog_tag' $script:HilogTag 'freeze' | Out-Null
    $window = Get-G0ObjectValue $Freeze 'scenario_window_seconds'
    if (-not (Test-G0JsonInteger $window) -or [int64] $window -ne [int64] $script:ScenarioWindowSeconds) {
        throw [G0RunnerError]::new('freeze.scenario_window_seconds must be the JSON integer 60')
    }

    $tuple = Get-G0ObjectValue $Freeze 'target_tuple'
    Assert-G0ExactKeys -Obj $tuple -ExpectedKeys ([string[]] @($script:ExpectedTargetTuple.Keys)) -Label 'freeze.target_tuple'
    foreach ($key in @($script:ExpectedTargetTuple.Keys)) {
        Assert-G0Constant $tuple $key $script:ExpectedTargetTuple[$key] 'freeze.target_tuple' | Out-Null
    }

    $artifacts = Get-G0ObjectValue $Freeze 'artifacts'
    Assert-G0ExactKeys -Obj $artifacts -ExpectedKeys $script:ArtifactKeys -Label 'freeze.artifacts'
    $hapPath = Assert-G0RequireAbsPath $artifacts 'hap_path' 'freeze.artifacts'
    foreach ($key in $script:ArtifactKeys[1..($script:ArtifactKeys.Count - 1)]) {
        Assert-G0RequireSha256 $artifacts $key 'freeze.artifacts' | Out-Null
    }
    if ($null -ne $RepoRootPath) {
        $normalizedRepo = Get-G0NormalizedPath $RepoRootPath
        if ((Test-G0UnderPath $hapPath $normalizedRepo) -or ((Get-G0NormalizedPath $hapPath) -ceq $normalizedRepo)) {
            throw [G0RunnerError]::new('freeze.artifacts.hap_path must be outside the git repository')
        }
    }

    $elf = Get-G0ObjectValue $Freeze 'elf_profile'
    Assert-G0ExactKeys -Obj $elf -ExpectedKeys ([string[]] @('pt_tls', 'tprel64_count', 'static_tls_flag', 'needed')) -Label 'freeze.elf_profile'
    $ptTls = Get-G0ObjectValue $elf 'pt_tls'
    if (-not ($ptTls -is [bool] -and $ptTls -eq $true)) {
        throw [G0RunnerError]::new('freeze.elf_profile.pt_tls must be the JSON boolean true')
    }
    $tprel = Get-G0ObjectValue $elf 'tprel64_count'
    if (-not (Test-G0JsonInteger $tprel) -or [int64] $tprel -ne 1) {
        throw [G0RunnerError]::new('freeze.elf_profile.tprel64_count must be the JSON integer 1')
    }
    $staticTls = Get-G0ObjectValue $elf 'static_tls_flag'
    if (-not ($staticTls -is [bool] -and $staticTls -eq $false)) {
        throw [G0RunnerError]::new('freeze.elf_profile.static_tls_flag must be the JSON boolean false')
    }
    $needed = Get-G0ObjectValue $elf 'needed'
    $neededOk = $false
    if ($null -ne $needed -and ($needed -is [System.Collections.IEnumerable]) -and -not ($needed -is [string])) {
        $neededList = [object[]] @($needed)
        if ($neededList.Count -eq 1 -and ([string] $neededList[0]) -ceq 'libc.so') { $neededOk = $true }
    }
    if (-not $neededOk) {
        throw [G0RunnerError]::new("freeze.elf_profile.needed must be exactly ['libc.so']")
    }

    foreach ($group in @(@{ label = 'freeze.evidence_roots'; key = 'evidence_roots' },
                         @{ label = 'freeze.raw_roots'; key = 'raw_roots' })) {
        $groupObj = Get-G0ObjectValue $Freeze $group['key']
        Assert-G0ExactKeys -Obj $groupObj -ExpectedKeys ([string[]] @('dry_run', 'live')) -Label $group['label']
        Assert-G0RequireAbsPath $groupObj 'dry_run' $group['label'] | Out-Null
        Assert-G0RequireAbsPath $groupObj 'live' $group['label'] | Out-Null
    }

    # Governance role literals are pinned by the freeze schema (spec values).
    Assert-G0Constant $Freeze 'operator' 'authorized user' 'freeze' | Out-Null
    Assert-G0Constant $Freeze 'orchestrator' 'main agent' 'freeze' | Out-Null

    Assert-G0GovernanceEntries -Freeze $Freeze
    return ([string] $planStatus)
}

function Assert-G0GovernanceRecord {
    # A declared pass is hash-anchored: the record file must exist, its bytes
    # must hash to record_sha256, and it must be parseable JSON.
    param([string] $Label, $Entry)
    $recordPath = Get-G0NormalizedPath ([string] (Get-G0ObjectValue $Entry 'record_path'))
    if (-not [System.IO.File]::Exists($recordPath)) {
        throw [G0RunnerError]::new(('{0} record file missing (status=pass requires the bound record)' -f $Label))
    }
    $actual = Get-G0Sha256File $recordPath
    $expected = [string] (Get-G0ObjectValue $Entry 'record_sha256')
    if ($actual -cne $expected.ToLowerInvariant()) {
        throw [G0RunnerError]::new(('{0} record_sha256 does not match the record file bytes' -f $Label))
    }
    try {
        $null = Read-G0TextUtf8Sig $recordPath | ConvertFrom-Json
    } catch {
        throw [G0RunnerError]::new(('{0} record is not parseable JSON' -f $Label))
    }
}

function Assert-G0GovernanceEntries {
    # confirmation/review schema + binding rules:
    #   * plan_status=ready requires confirmation.status=pass AND review.status=pass
    #     (double binding); blocked allows pending;
    #   * review.status=pass with a pending/absent machine confirmation is rejected
    #     (a declared-pass review can never ride on a pending machine side);
    #   * any declared pass is hash-verified against the record file.
    param($Freeze)
    $confirmation = Get-G0ObjectValue $Freeze 'confirmation'
    Assert-G0ExactKeys -Obj $confirmation -ExpectedKeys $script:ConfirmationKeys -Label 'freeze.confirmation'
    Assert-G0RequireAbsPath $confirmation 'record_path' 'freeze.confirmation' | Out-Null
    Assert-G0RequireSha256 $confirmation 'record_sha256' 'freeze.confirmation' | Out-Null
    Assert-G0RequireNonEmptyStr $confirmation 'authorization_id' 'freeze.confirmation' | Out-Null
    $review = Get-G0ObjectValue $Freeze 'review'
    Assert-G0ExactKeys -Obj $review -ExpectedKeys $script:ReviewKeys -Label 'freeze.review'
    Assert-G0RequireAbsPath $review 'record_path' 'freeze.review' | Out-Null
    Assert-G0RequireSha256 $review 'record_sha256' 'freeze.review' | Out-Null
    foreach ($pair in @(@('freeze.confirmation', $confirmation), @('freeze.review', $review))) {
        $status = [string] (Get-G0ObjectValue $pair[1] 'status')
        if ($status -cne 'pass' -and $status -cne 'pending') {
            throw [G0RunnerError]::new(('{0}.status must be pass or pending' -f $pair[0]))
        }
    }
    $confirmationPass = ([string] (Get-G0ObjectValue $confirmation 'status')) -ceq 'pass'
    $reviewPass = ([string] (Get-G0ObjectValue $review 'status')) -ceq 'pass'
    if ($reviewPass -and -not $confirmationPass) {
        throw [G0RunnerError]::new('review.status=pass requires confirmation.status=pass; a pending machine confirmation cannot anchor a declared-pass review')
    }
    if (([string] (Get-G0ObjectValue $Freeze 'plan_status')) -ceq 'ready' -and -not ($confirmationPass -and $reviewPass)) {
        throw [G0RunnerError]::new('plan_status ready requires confirmation.status=pass and review.status=pass (double binding)')
    }
    if ($confirmationPass) {
        Assert-G0GovernanceRecord -Label 'confirmation' -Entry $confirmation
        if (([string] (Get-G0ObjectValue $confirmation 'authorization_id')) -cne ([string] (Get-G0ObjectValue $Freeze 'authorization_id'))) {
            throw [G0RunnerError]::new('confirmation.authorization_id does not match freeze.authorization_id')
        }
    }
    if ($reviewPass) {
        Assert-G0GovernanceRecord -Label 'review' -Entry $review
    }
}

function Load-G0Freeze {
    # Parse + validate the freeze manifest for the given mode. Returns the
    # freeze object or throws (pre-campaign validation error -> exit 1).
    param([string] $FreezePath, [string] $Mode, $RepoRootPath)
    if (-not [System.IO.File]::Exists($FreezePath)) {
        throw [G0RunnerError]::new(('Freeze file missing: {0}' -f $FreezePath))
    }
    $freeze = $null
    try {
        $freeze = (Read-G0TextUtf8Sig $FreezePath) | ConvertFrom-Json
    } catch {
        throw [G0RunnerError]::new(('Freeze file is not valid JSON: {0}' -f [string] $_.Exception.Message))
    }
    Assert-G0FreezeSchema -Freeze $freeze -Mode $Mode -RepoRootPath $RepoRootPath | Out-Null
    return $freeze
}

# =====================================================================
# Section 4: HDC whitelist and process execution (design unit U4)
# =====================================================================

# The 15 frozen operations; audit-form argv is built verbatim in
# Get-G0HdcInvocation. Parameter lists name the required parameters.
$script:HdcWhitelist = New-G0OrdinalMap
foreach ($pair in @(
    @('Version', [string[]] @()), @('TupleModel', [string[]] @()), @('TupleBuild', [string[]] @()),
    @('MkdirStaging', [string[]] @()), @('SendHap', [string[]] @()), @('InstallHap', [string[]] @()),
    @('HilogStream', [string[]] @()), @('FaultProbe', [string[]] @()), @('RemoveStaging', [string[]] @()), @('StagingProbe', [string[]] @()),
    @('BundleDump', [string[]] @('Bundle')), @('PidOf', [string[]] @('Bundle')), @('StartEntry', [string[]] @('Bundle')),
    @('Uninstall', [string[]] @('Bundle')), @('ForceStop', [string[]] @('Bundle', 'Reason')))) {
    $script:HdcWhitelist[$pair[0]] = $pair[1]
}

# PS-equivalent case-insensitive operation/parameter lookup (MINOR-2 style).
$script:HdcOperationAliases = New-G0OrdinalStringMap
foreach ($name in [string[]] @($script:HdcWhitelist.Keys)) {
    $script:HdcOperationAliases[$name.ToLowerInvariant()] = $name
}
$script:HdcParameterAliases = New-G0OrdinalStringMap
foreach ($name in [string[]] @('Bundle', 'Reason')) {
    $script:HdcParameterAliases[$name.ToLowerInvariant()] = $name
}

$script:ForceStopReasons = [string[]] @('exception-cleanup', 'final-cleanup')

function ConvertTo-G0HdcOperation {
    # ToLowerInvariant -> canonical PascalCase; unknown names pass through so
    # the whitelist rejection keeps the caller's spelling.
    param([string] $Operation)
    $lower = ([string] $Operation).ToLowerInvariant()
    if ($script:HdcOperationAliases.ContainsKey($lower)) {
        return [string] $script:HdcOperationAliases[$lower]
    }
    return $Operation
}

function ConvertTo-G0HdcParameters {
    # casefold -> canonical parameter names; unknown names pass through.
    param([System.Collections.IDictionary] $Parameters)
    $out = New-G0OrdinalMap
    if ($null -eq $Parameters) { return $out }
    foreach ($key in @($Parameters.Keys)) {
        $canonical = [string] $key
        $lower = $canonical.ToLowerInvariant()
        if ($script:HdcParameterAliases.ContainsKey($lower)) {
            $canonical = [string] $script:HdcParameterAliases[$lower]
        }
        $out[$canonical] = $Parameters[$key]
    }
    return $out
}

function Assert-G0ExactCommandParameters {
    # Unknown operation / extra parameter / missing parameter / empty value
    # -> rejected. Mirrors E3 Assert-ExactCommandParameters.
    param([string] $Operation, [System.Collections.Generic.Dictionary[string, object]] $Parameters)
    if (-not $script:HdcWhitelist.ContainsKey($Operation)) {
        throw [G0RunnerError]::new(("command rejected: operation '{0}' is not allowlisted" -f $Operation))
    }
    $required = [string[]] $script:HdcWhitelist[$Operation]
    foreach ($name in $required) {
        if (-not $Parameters.ContainsKey($name) -or [string]::IsNullOrWhiteSpace([string] $Parameters[$name])) {
            throw [G0RunnerError]::new(("command rejected: operation '{0}' requires parameter '{1}'" -f $Operation, $name))
        }
    }
    foreach ($name in @($Parameters.Keys)) {
        if ($required -cnotcontains $name) {
            throw [G0RunnerError]::new(("command rejected: operation '{0}' does not accept parameter '{1}'" -f $Operation, $name))
        }
    }
}

function Get-G0HdcInvocation {
    # Whitelisted audit argv construction. The audit (projection/transcript)
    # form ALWAYS keeps the placeholders <PHYS_1_TARGET>/<HAP_G0>; the real
    # target never enters the projection. Live substitution happens only in
    # ConvertTo-G0LiveHdcArguments. G0 pidof targets the bundle UI process (no
    # :vpn suffix); the Bundle parameter must equal the frozen bundle.
    param([string] $Operation, [System.Collections.IDictionary] $Parameters = $null)
    $canonical = ConvertTo-G0HdcOperation $Operation
    $params = ConvertTo-G0HdcParameters $Parameters
    Assert-G0ExactCommandParameters -Operation $canonical -Parameters $params
    $bundle = ''
    if ($params.ContainsKey('Bundle')) { $bundle = [string] $params['Bundle'] }
    if ($bundle -and $bundle -cne $script:Bundle) {
        throw [G0RunnerError]::new('command rejected: bundle outside the frozen G0 bundle')
    }
    if ($canonical -ceq 'ForceStop') {
        $reason = [string] $params['Reason']
        if (-not $script:ForceStopReasons.Contains($reason)) {
            throw [G0RunnerError]::new('command rejected: force-stop is cleanup-only (Reason must be exception-cleanup or final-cleanup)')
        }
    }
    $argv = [System.Collections.Generic.List[string]]::new()
    switch ($canonical) {
        'Version' {
            $argv.Add('version')
            return $argv.ToArray()
        }
        'TupleModel' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'param', 'get', 'const.product.model'))
            return $argv.ToArray()
        }
        'TupleBuild' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'param', 'get', 'const.product.software.version'))
            return $argv.ToArray()
        }
        'BundleDump' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'bm', 'dump', '-n', $bundle))
            return $argv.ToArray()
        }
        'PidOf' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'pidof', $bundle))
            return $argv.ToArray()
        }
        'MkdirStaging' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'mkdir', '-p', ($script:Staging + '/hap')))
            return $argv.ToArray()
        }
        'SendHap' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'file', 'send', $script:HapPlaceholder, ($script:Staging + '/hap/g0.hap')))
            return $argv.ToArray()
        }
        'InstallHap' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'bm', 'install', '-p', ($script:Staging + '/hap')))
            return $argv.ToArray()
        }
        'StartEntry' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'aa', 'start', '-a', $script:Ability, '-b', $bundle, '-m', $script:Module))
            return $argv.ToArray()
        }
        'HilogStream' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'hilog', '-T', $script:HilogTag, '-v', 'year', '-v', 'zone'))
            return $argv.ToArray()
        }
        'FaultProbe' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', ('*{0}*' -f $script:Bundle), '-print'))
            return $argv.ToArray()
        }
        'ForceStop' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'aa', 'force-stop', $bundle))
            return $argv.ToArray()
        }
        'Uninstall' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'bm', 'uninstall', '-n', $bundle))
            return $argv.ToArray()
        }
        'RemoveStaging' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'rm', '-rf', $script:Staging))
            return $argv.ToArray()
        }
        'StagingProbe' {
            $argv.AddRange([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'ls', '-ld', $script:Staging))
            return $argv.ToArray()
        }
        default {
            throw [G0RunnerError]::new(("command rejected: operation '{0}' is not allowlisted" -f $canonical))
        }
    }
}

function ConvertTo-G0LiveHdcArguments {
    # Placeholder substitution for the execution path only. The audit /
    # transcript form always keeps the placeholders.
    param([string[]] $AuditArguments, [string] $TargetToken, [string] $HapPath)
    $live = [System.Collections.Generic.List[string]]::new()
    foreach ($arg in $AuditArguments) {
        if ($arg -ceq $script:TargetPlaceholder) {
            if (-not $TargetToken) {
                throw [G0RunnerError]::new('live target substitution requires a real target token')
            }
            $live.Add($TargetToken)
        } elseif ($arg -ceq $script:HapPlaceholder) {
            if (-not $HapPath) {
                throw [G0RunnerError]::new('live hap substitution requires the frozen HAP path')
            }
            $live.Add($HapPath)
        } else {
            $live.Add($arg)
        }
    }
    return $live.ToArray()
}

function Test-G0PhysicalTargetToken {
    # Python Test-PhysicalTargetToken (E3-verbatim logic): non-empty,
    # no leading/trailing whitespace, no whitespace/comma/semicolon, not PHYS-1
    # or a placeholder, and never a flag-shaped token (leading '-').
    param([string] $Target)
    if ($null -eq $Target -or -not ([string] $Target).Trim()) { return $false }
    $value = [string] $Target
    if ($value -cne $value.Trim()) { return $false }
    if ($value -cmatch '[\s,;]') { return $false }
    if ($value.StartsWith('-')) { return $false }
    if ($value -imatch '^(PHYS-1|<.+>)$') { return $false }
    return $true
}

function Assert-G0TargetEnvironment {
    # PHYS_1_TARGET is a process-scope environment variable.
    $target = [string] [System.Environment]::GetEnvironmentVariable('PHYS_1_TARGET')
    if (-not (Test-G0PhysicalTargetToken $target)) {
        throw [G0RunnerError]::new('PHYS_1_TARGET must contain exactly one real target token')
    }
    $script:ActualTarget = $target
}

function Get-G0HdcCountFromPsOutput {
    # Count host hdc processes from `ps -eo comm=,args=` output by comparing
    # ONLY the first column; argv text can never make an unrelated process
    # match (fake-hdc / python3 never match).
    param([string] $Text)
    $count = 0
    if ($null -eq $Text) { return 0 }
    foreach ($line in [string] $Text -split "`n") {
        $first = ((([string] $line).Trim()) -split '\s+')[0]
        if ($first -ceq 'hdc') { $count++ }
    }
    return $count
}

function Get-G0HdcProcessCount {
    # Fixed absolute host probe: /usr/bin/ps -eo comm=,args=. Returns -1 if
    # the probe is unavailable or fails (E3 count_hdc_processes).
    try {
        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = '/usr/bin/ps'
        [void] $psi.ArgumentList.Add('-eo')
        [void] $psi.ArgumentList.Add('comm=,args=')
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.UseShellExecute = $false
        $proc = [System.Diagnostics.Process]::Start($psi)
        $outTask = $proc.StandardOutput.ReadToEndAsync()
        if (-not $proc.WaitForExit(10000)) {
            try { $proc.Kill() } catch { }
            return -1
        }
        if ($proc.ExitCode -ne 0) { return -1 }
        return Get-G0HdcCountFromPsOutput ($outTask.GetAwaiter().GetResult())
    } catch {
        return -1
    }
}

function Invoke-G0ChildProcess {
    # Execute a process with verbatim ArgumentList (no shell interpolation)
    # and file redirection; wait up to TimeoutSeconds, kill the whole tree on
    # timeout. Returns @{timed_out; exit_code}.
    param([string] $FilePath, [string[]] $ArgumentList, [int] $TimeoutSeconds, [string] $StdoutFile, [string] $StderrFile)
    $proc = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -NoNewWindow `
        -RedirectStandardOutput $StdoutFile -RedirectStandardError $StderrFile -PassThru
    $timedOut = -not $proc.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
        try { $proc.Kill($true) } catch { try { $proc.Kill() } catch { } }
    }
    [void] $proc.WaitForExit()
    return @{
        timed_out = [bool] $timedOut
        exit_code = $(if ($timedOut) { $null } else { [int] $proc.ExitCode })
    }
}

function Invoke-G0CaptureProcess {
    # Run a child process capturing stdout/stderr into caller-owned text.
    param([string] $FilePath, [string[]] $ArgumentList, [int] $TimeoutSeconds)
    $stamp = [System.Guid]::NewGuid().ToString('N')
    $stdoutFile = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), ('g0-out-{0}.txt' -f $stamp))
    $stderrFile = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), ('g0-err-{0}.txt' -f $stamp))
    try {
        $proc = Invoke-G0ChildProcess -FilePath $FilePath -ArgumentList $ArgumentList `
            -TimeoutSeconds $TimeoutSeconds -StdoutFile $stdoutFile -StderrFile $stderrFile
        $stdout = ''
        $stderr = ''
        if ([System.IO.File]::Exists($stdoutFile)) { $stdout = [System.IO.File]::ReadAllText($stdoutFile) }
        if ([System.IO.File]::Exists($stderrFile)) { $stderr = [System.IO.File]::ReadAllText($stderrFile) }
        return @{
            timed_out = [bool] $proc['timed_out']
            exit_code = $proc['exit_code']
            stdout    = $stdout
            stderr    = $stderr
        }
    } finally {
        foreach ($file in @($stdoutFile, $stderrFile)) {
            try { if ([System.IO.File]::Exists($file)) { [System.IO.File]::Delete($file) } } catch { }
        }
    }
}

# =====================================================================
# Section 5: Marker parsing and pre-registered verdict mapping
# =====================================================================

function ConvertFrom-G0MarkerLine {
    # Parse the pre-registered pipe-delimited marker body
    # (`G0_RESULT|k=v|k=v|...`, docs/g0-go-arm64-physical-probe.md). Values
    # may contain spaces (the C/ArkTS layers sanitize '|' to space), so each
    # '|' segment is one field; a segment without '=' continues the previous
    # value.
    param([string] $Line)
    $text = [string] $Line
    $idx = $text.IndexOf($script:MarkerToken, [System.StringComparison]::Ordinal)
    if ($idx -lt 0) { return New-G0OrdinalMap }
    $body = $text.Substring($idx + $script:MarkerToken.Length)
    # Ordinal (case-sensitive) map: PowerShell's default hashtable is
    # case-insensitive, which aliased `Verdict=` onto `verdict=` and could
    # judge a drifting marker as pass (review BLOCKER-2). With Ordinal keys
    # a different-case key stays a distinct entry and exact-case lookups
    # fail closed, matching Python dict semantics.
    $fields = New-G0OrdinalMap
    $current = $null
    foreach ($segment in $body.Split('|')) {
        $seg = $segment.Trim()
        if (-not $seg) { continue }
        if ($seg -cmatch '^([A-Za-z_][A-Za-z0-9_]*)=(.*)$') {
            $current = [string] $Matches[1]
            $fields[$current] = [string] $Matches[2]
        } elseif ($null -ne $current) {
            $fields[$current] = (($fields[$current] + ' ' + $seg).Trim())
        }
    }
    return $fields
}

function Get-G0MarkerMapping {
    # Pre-registered mapping (fail-closed). Returns
    # @{verdict; fail_reason; fields}.
    param([object[]] $MarkerLines)
    $lines = [object[]] @($MarkerLines)
    if ($null -eq $MarkerLines -or $lines.Count -eq 0) {
        return @{ verdict = 'blocked'; fail_reason = 'marker-missing'; fields = @{} }
    }
    if ($lines.Count -gt 1) {
        return @{ verdict = 'blocked'; fail_reason = 'marker-ambiguous'; fields = @{} }
    }
    $fields = ConvertFrom-G0MarkerLine -Line ([string] $lines[0])
    $allPass = $true
    foreach ($key in [string[]] @($script:PassFields.Keys)) {
        $expected = [string] $script:PassFields[$key]
        $actual = $null
        if ($fields.ContainsKey($key)) { $actual = [string] $fields[$key] }
        if ($null -eq $actual -or $actual -cne $expected) { $allPass = $false }
    }
    if ($allPass) {
        return @{ verdict = 'pass'; fail_reason = $null; fields = $fields }
    }
    $verdictField = ''
    $stageField = ''
    $loaderError = ''
    if ($fields.ContainsKey('verdict')) { $verdictField = [string] $fields['verdict'] }
    if ($fields.ContainsKey('stage')) { $stageField = [string] $fields['stage'] }
    if ($fields.ContainsKey('loaderError')) { $loaderError = [string] $fields['loaderError'] }
    if ($verdictField -ceq 'FAIL' -and $stageField -ceq 'dlopen' -and -not [string]::IsNullOrWhiteSpace($loaderError)) {
        # A valid measured result: the loader rejected the library.
        return @{ verdict = 'blocked'; fail_reason = 'dlopen-blocked'; fields = $fields }
    }
    return @{ verdict = 'blocked'; fail_reason = 'drift'; fields = $fields }
}

# =====================================================================
# Section 6: Transcript (line-chained JSONL) and integrity verification
# =====================================================================

function Add-G0TranscriptRecord {
    # One line per runner step: {seq, ts, event, details, prev_line_sha256,
    # line_sha256} with line_sha256 = sha256(prev_line_sha256 + canonical JSON
    # of the line WITHOUT line_sha256). The last line is the chain head.
    # No-op until the projection transcript is initialized.
    param([string] $Event, [hashtable] $Details)
    if ($null -eq $script:ProjectionTranscript) { return }
    $entry = @{
        seq               = [int64] ($script:TranscriptIndex + 1)
        ts                = Get-G0NowIso
        event             = [string] $Event
        details           = Protect-G0SensitiveData $(if ($null -eq $Details) { @{} } else { $Details })
        prev_line_sha256  = $script:TranscriptPreviousHash
    }
    $canonical = ConvertTo-G0CanonicalJson $entry
    $lineSha = Get-G0Sha256Text ($script:TranscriptPreviousHash + $canonical)
    $record = @{
        seq              = $entry['seq']
        ts               = $entry['ts']
        event            = $entry['event']
        details          = $entry['details']
        prev_line_sha256 = $entry['prev_line_sha256']
        line_sha256      = $lineSha
    }
    [System.IO.File]::AppendAllText($script:ProjectionTranscript, (ConvertTo-G0CanonicalJson $record) + "`n", $script:Utf8NoBom)
    $script:TranscriptIndex++
    $script:TranscriptPreviousHash = $lineSha
}

function Get-G0TranscriptLineFields {
    # Parse one transcript line into its RAW JSON field texts. System.Text.Json
    # keeps the original token bytes; ConvertFrom-Json would coerce the
    # ISO-8601 ts string into a DateTime and break the canonical re-hash.
    # Throws when a required property is missing (caller: json-invalid).
    param([string] $Line)
    $doc = [System.Text.Json.JsonDocument]::Parse($Line)
    try {
        $root = $doc.RootElement
        return @{
            seq_value   = [int64] ($root.GetProperty('seq').GetInt64())
            seq_raw     = $root.GetProperty('seq').GetRawText()
            ts_raw      = $root.GetProperty('ts').GetRawText()
            event_raw   = $root.GetProperty('event').GetRawText()
            details_raw = $root.GetProperty('details').GetRawText()
            prev_raw    = $root.GetProperty('prev_line_sha256').GetRawText()
            line_raw    = $root.GetProperty('line_sha256').GetRawText()
        }
    } finally {
        $doc.Dispose()
    }
}

function ConvertFrom-G0JsonQuotedString {
    # Strip the surrounding quotes of a raw JSON string token (values written
    # by this runner never contain escaped quotes in these fields).
    param([string] $Raw)
    $text = [string] $Raw
    if ($text.Length -ge 2 -and $text.StartsWith('"') -and $text.EndsWith('"')) {
        return $text.Substring(1, $text.Length - 2)
    }
    return $text
}

function Test-G0TranscriptIntegrity {
    # Recompute the per-line chain (canonical bytes, previous hash, seq
    # order, line hash). Returns the unique violation list in order.
    param([string] $TranscriptPath)
    $violations = [System.Collections.Generic.List[string]]::new()
    if (-not [System.IO.File]::Exists($TranscriptPath)) {
        $violations.Add('transcript-missing')
        return $violations
    }
    $previousHash = ('0' * 64)
    $expectedSeq = 1
    foreach ($raw in [System.IO.File]::ReadLines($TranscriptPath)) {
        $line = ([string] $raw).TrimEnd("`n").TrimEnd("`r")
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try {
            $fields = Get-G0TranscriptLineFields -Line $line
            $storedPrev = (ConvertFrom-G0JsonQuotedString $fields['prev_raw'])
            $storedLineSha = (ConvertFrom-G0JsonQuotedString $fields['line_raw'])
            # The writer emits canonical (sorted, compact) JSON, so the raw
            # token bytes ARE the canonical bytes of each field.
            $core = '{"details":' + $fields['details_raw'] `
                + ',"event":' + $fields['event_raw'] `
                + ',"prev_line_sha256":' + $fields['prev_raw'] `
                + ',"seq":' + $fields['seq_raw'] `
                + ',"ts":' + $fields['ts_raw'] + '}'
        } catch {
            $violations.Add('transcript-json-invalid')
            continue
        }
        if ([int64] $fields['seq_value'] -ne [int64] $expectedSeq) { $violations.Add('transcript-order-invalid') }
        if ($storedPrev -cne $previousHash) { $violations.Add('transcript-previous-hash-invalid') }
        $recomputed = Get-G0Sha256Text ($storedPrev + $core)
        if ($recomputed -cne $storedLineSha) { $violations.Add('transcript-line-hash-invalid') }
        $previousHash = $storedLineSha
        $expectedSeq++
    }
    $unique = [System.Collections.Generic.List[string]]::new()
    foreach ($violation in $violations) {
        if (-not $unique.Contains($violation)) { $unique.Add($violation) }
    }
    return $unique
}

function Get-G0TranscriptChainHead {
    # Chain head = line_sha256 of the final line ('0'*64 when empty).
    param([string] $TranscriptPath)
    $head = ('0' * 64)
    if (-not [System.IO.File]::Exists($TranscriptPath)) { return $head }
    foreach ($raw in [System.IO.File]::ReadLines($TranscriptPath)) {
        $line = ([string] $raw).TrimEnd("`n").TrimEnd("`r")
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try {
            $fields = Get-G0TranscriptLineFields -Line $line
            $head = (ConvertFrom-G0JsonQuotedString $fields['line_raw'])
        } catch {
            continue
        }
    }
    return $head
}

# =====================================================================
# Section 7: Out-of-repo double-file confirmation record (design unit U3)
# =====================================================================

function New-G0ConfirmationRecord {
    # The single-use confirmation record: kind g0-target-binding-confirmation,
    # is_evidence=false, target_redacted=true, projected (public) model/build
    # values, created_at in the fixed +08:00 zone.
    param($Freeze, [DateTimeOffset] $StartedAt, [DateTimeOffset] $EndedAt, [string] $Verdict, [string] $Reason,
        [string] $ObservedVersion, [string] $ObservedModel, [string] $ObservedBuild,
        [int] $CommandAttempted, [int] $CommandCompleted)
    $hdc = Get-G0ObjectValue $Freeze 'hdc'
    $tuple = Get-G0ObjectValue $Freeze 'target_tuple'
    return @{
        schema_version      = 1
        record_kind         = 'g0-target-binding-confirmation'
        is_evidence         = $false
        authorization_id    = [string] (Get-G0ObjectValue $Freeze 'authorization_id')
        campaign_id         = [string] (Get-G0ObjectValue $Freeze 'campaign_id')
        evidence_id         = [string] (Get-G0ObjectValue $Freeze 'evidence_id')
        attempt             = [string] (Get-G0ObjectValue $Freeze 'attempt')
        plan_status         = [string] (Get-G0ObjectValue $Freeze 'plan_status')
        target_redacted     = $true
        code_sha            = [string] (Get-G0ObjectValue $Freeze 'code_sha')
        runner_py_sha256    = [string] (Get-G0ObjectValue $Freeze 'runner_py_sha256')
        hdc_version_expected = [string] (Get-G0ObjectValue $hdc 'version')
        hdc_sha256          = [string] (Get-G0ObjectValue $hdc 'sha256')
        expected_model      = [string] (Get-G0ObjectValue $tuple 'device_model')
        expected_build      = [string] (Get-G0ObjectValue $tuple 'full_system_build')
        observed_version    = $ObservedVersion
        observed_model      = $ObservedModel
        observed_build      = $ObservedBuild
        command_attempted   = [int] $CommandAttempted
        command_completed   = [int] $CommandCompleted
        started_at          = (Format-G0Iso8601 $StartedAt)
        ended_at            = (Format-G0Iso8601 $EndedAt)
        created_at          = (Get-G0NowIso)
        execution_mode      = 'target-binding-confirm'
        verdict             = $Verdict
        reason              = $(if ([string]::IsNullOrEmpty($Reason)) { 'N/A' } else { [string] $Reason })
    }
}

function Write-G0ConfirmationRecordPair {
    # Double-file completion marker: JSON tmp + .sha256 tmp (hash recomputed
    # over the tmp bytes), then atomic rename; the companion is renamed LAST as
    # the completion marker. Single-use is enforced by the pre-record gates
    # (record/companion must not exist); an orphan JSON from a failed companion
    # rename is never overwritten and never consumable.
    param([string] $RecordPath, [hashtable] $Record)
    $companionPath = $RecordPath + '.sha256'
    foreach ($candidate in @($RecordPath, $companionPath)) {
        if ([System.IO.File]::Exists($candidate) -or [System.IO.Directory]::Exists($candidate)) {
            throw [G0PreRecordGateError]::new(('confirmation output already exists and is immutable: {0}' -f $candidate))
        }
    }
    $tmpJson = $RecordPath + '.tmp-' + [System.Guid]::NewGuid().ToString('N')
    $tmpSha = $companionPath + '.tmp-' + [System.Guid]::NewGuid().ToString('N')
    $sha = $null
    try {
        $jsonText = (ConvertTo-G0PrettyJson $Record) + "`n"
        Write-G0TextUtf8NoBomSynced $tmpJson $jsonText
        $sha = Get-G0Sha256File $tmpJson
        if ((Get-G0Sha256File $tmpJson) -cne $sha) {
            throw [G0RunnerError]::new('confirmation record hash recompute mismatch')
        }
        Write-G0TextUtf8NoBomSynced $tmpSha ($sha + "`n")
        # Atomic rename: JSON first, companion LAST (completion marker).
        [System.IO.File]::Move($tmpJson, $RecordPath)
        [System.IO.File]::Move($tmpSha, $companionPath)
    } finally {
        foreach ($tmp in @($tmpJson, $tmpSha)) {
            try { if ([System.IO.File]::Exists($tmp)) { [System.IO.File]::Delete($tmp) } } catch { }
        }
    }
    return $sha
}

function Invoke-G0TargetBindingConfirm {
    # Host-governed machine fresh target binding: exactly the three frozen
    # probes (Version/TupleModel/TupleBuild) against the REAL frozen hdc
    # (path + sha256 from the freeze, sha recomputed before execution), then a
    # single-use out-of-repo double-file record. Exit codes: pass=0, pre-record
    # gate=1 (no record), probe/tuple/write failure=2 (blocked record).
    param($Freeze, [string] $FreezePath, [string] $ConfirmationRecordArg, $RepoRootPath)
    $freezePathUnused = $FreezePath # binding context only; the record binds hashes, not paths
    $recordPath = Get-G0NormalizedPath $ConfirmationRecordArg
    if ($null -ne $RepoRootPath) {
        $normalizedRepo = Get-G0NormalizedPath $RepoRootPath
        if (($recordPath -ceq $normalizedRepo) -or (Test-G0UnderPath $recordPath $normalizedRepo)) {
            throw [G0PreRecordGateError]::new('ConfirmationRecord must be outside the git repository')
        }
    }
    $companionPath = $recordPath + '.sha256'
    if ([System.IO.File]::Exists($recordPath)) {
        throw [G0PreRecordGateError]::new('ConfirmationRecord already exists; target-binding confirmation is single-use')
    }
    if ([System.IO.File]::Exists($companionPath)) {
        throw [G0PreRecordGateError]::new('ConfirmationRecord .sha256 companion already exists; target-binding confirmation is single-use')
    }
    $startedAt = Get-G0NowOffset
    $verdict = 'blocked'
    $reason = $null
    $observedVersion = ''
    $observedModel = ''
    $observedBuild = ''
    $commandAttempted = 0
    $commandCompleted = 0
    try {
        $hdc = Get-G0ObjectValue $Freeze 'hdc'
        $hdcPath = Get-G0NormalizedPath ([string] (Get-G0ObjectValue $hdc 'path'))
        # Recompute the frozen hdc hash immediately before execution.
        Assert-G0FileHash -Label 'frozen hdc executable' -Path $hdcPath -Expected ([string] (Get-G0ObjectValue $hdc 'sha256'))
        Assert-G0TargetEnvironment
        $hapLive = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'artifacts') 'hap_path'))
        foreach ($operation in [string[]] @('Version', 'TupleModel', 'TupleBuild')) {
            $commandAttempted++
            [string[]] $auditArgv = Get-G0HdcInvocation -Operation $operation
            [string[]] $liveArgv = ConvertTo-G0LiveHdcArguments -AuditArguments $auditArgv -TargetToken $script:ActualTarget -HapPath $hapLive
            $proc = Invoke-G0CaptureProcess -FilePath $hdcPath -ArgumentList $liveArgv -TimeoutSeconds $script:HdcTimeoutSeconds
            $commandCompleted++
            switch ($operation) {
                'Version' { $observedVersion = ([string] $proc['stdout']).Trim() }
                'TupleModel' { $observedModel = ([string] $proc['stdout']).Trim() }
                'TupleBuild' { $observedBuild = ([string] $proc['stdout']).Trim() }
            }
        }
        if ($commandAttempted -ne 3 -or $commandCompleted -ne 3) {
            throw [G0RunnerError]::new('target-binding confirmation requires exactly three HDC probes')
        }
        $expectedVersion = [string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'hdc') 'version')
        if (([string] $observedVersion).IndexOf($expectedVersion, [System.StringComparison]::Ordinal) -lt 0) {
            throw [G0RunnerError]::new('frozen HDC version mismatch')
        }
        if (([string] $observedModel) -cne ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'target_tuple') 'device_model'))) {
            throw [G0RunnerError]::new('frozen device model mismatch')
        }
        if (([string] $observedBuild) -cne ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'target_tuple') 'full_system_build'))) {
            throw [G0RunnerError]::new('frozen full system build mismatch')
        }
        $verdict = 'pass'
    } catch {
        # Probe / environment / tuple failure: still write a best-effort
        # blocked record + companion (exit 2).
        $reason = Protect-G0SensitiveText ([string] $_.Exception.Message)
        $verdict = 'blocked'
    }
    $endedAt = Get-G0NowOffset
    # Device-observed values are protected before entering the record; the
    # real target never appears in any field.
    $record = New-G0ConfirmationRecord -Freeze $Freeze -StartedAt $startedAt -EndedAt $endedAt -Verdict $verdict -Reason $reason `
        -ObservedVersion (Protect-G0SensitiveText $observedVersion) -ObservedModel (Protect-G0SensitiveText $observedModel) `
        -ObservedBuild (Protect-G0SensitiveText $observedBuild) -CommandAttempted $commandAttempted -CommandCompleted $commandCompleted
    $recordSha = $null
    try {
        $null = Write-G0ConfirmationRecordPair -RecordPath $recordPath -Record $record
        # Return and disk stay the same source: recompute over the final file.
        $recordSha = Get-G0Sha256File $recordPath
    } catch [G0PreRecordGateError] {
        throw
    } catch {
        # A companion failure may leave an orphan JSON: never deleted, never
        # overwritten, never consumable. Downgrade to blocked (exit 2).
        $writeFailure = Protect-G0SensitiveText ([string] $_.Exception.Message)
        $verdict = 'blocked'
        $recordSha = $null
        if ([string]::IsNullOrEmpty($reason)) {
            $reason = ('record-write-failed: {0}' -f $writeFailure)
        } else {
            $reason = ('{0}; record-write-failed: {1}' -f $reason, $writeFailure)
        }
        $record = New-G0ConfirmationRecord -Freeze $Freeze -StartedAt $startedAt -EndedAt $endedAt -Verdict $verdict -Reason $reason `
            -ObservedVersion (Protect-G0SensitiveText $observedVersion) -ObservedModel (Protect-G0SensitiveText $observedModel) `
            -ObservedBuild (Protect-G0SensitiveText $observedBuild) -CommandAttempted $commandAttempted -CommandCompleted $commandCompleted
        try {
            $null = Write-G0ConfirmationRecordPair -RecordPath $recordPath -Record $record
            $recordSha = Get-G0Sha256File $recordPath
        } catch {
            $recordSha = $null
        }
    }
    $suffix = ''
    if (-not [string]::IsNullOrEmpty($recordSha)) { $suffix = (' RECORD_SHA256={0}' -f $recordSha) }
    Write-G0Out ('RUNNER_RESULT={0} MODE=target-binding-confirm RECORD_KIND=g0-target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED={1} COMMAND_COMPLETED={2} RECORD={3}{4}' -f $verdict, $commandAttempted, $commandCompleted, $recordPath, $suffix)
    if ($verdict -ceq 'pass') { return 0 }
    return 2
}

# =====================================================================
# Section 8: Campaign flow (design unit U6/U7) - single S1 scenario
# =====================================================================

function Get-G0RootKeyForMode {
    # Freeze root keys are `dry_run`/`live`; modes are `dry-run`/`live`.
    param([string] $Mode)
    if ($Mode -ceq 'dry-run') { return 'dry_run' }
    return 'live'
}

function New-G0CampaignContext {
    # Script-scope campaign state (E3-style $script: variables, contained).
    param($Freeze, [string] $Mode, $RepoRootPath, [string] $FreezePath)
    $ctx = @{}
    $ctx['freeze'] = $Freeze
    $ctx['mode'] = $Mode
    $ctx['is_evidence'] = ($Mode -ceq 'live')
    $ctx['repo_root'] = $RepoRootPath
    $ctx['freeze_path'] = Get-G0NormalizedPath $FreezePath
    $ctx['freeze_sha256'] = Get-G0Sha256File $ctx['freeze_path']
    $ctx['evidence_path'] = $null
    $ctx['raw_path'] = $null
    $ctx['transcript_path'] = $null
    $ctx['executable'] = $null
    $ctx['simulated'] = ($Mode -ceq 'dry-run')
    $ctx['dry_run_script'] = [string] [System.Environment]::GetEnvironmentVariable('G0_DRYRUN_SCRIPT')
    $ctx['sim_state'] = @{ staging = $false; hap_sent = $false; installed = $false; entry_started = $false; running_pid = $null }
    $ctx['hap_live'] = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'artifacts') 'hap_path'))
    $ctx['command_seq'] = 0
    $ctx['hdc_logical_calls'] = 0
    $ctx['hdc_process_starts'] = 0
    $ctx['hdc_operations'] = @{}
    $ctx['command_attempted'] = 0
    $ctx['command_completed'] = 0
    $ctx['integrity_violations'] = [System.Collections.Generic.List[string]]::new()
    $ctx['installed'] = $false
    $ctx['entry_started'] = $false
    $ctx['staging_prepared'] = $false
    $ctx['cleanup_actions'] = [System.Collections.Generic.List[object]]::new()
    $ctx['cleanup_status'] = 'not-run'
    $ctx['absent_probes'] = @{}
    $ctx['marker_lines'] = [string[]] @()
    $ctx['markers'] = @{}
    $ctx['marker_mapping'] = $null
    $ctx['loader_error'] = $null
    $ctx['loader_errno'] = $null
    $ctx['observed_tuple'] = @{}
    $ctx['fault_lines'] = $null
    $ctx['steps'] = [System.Collections.Generic.List[object]]::new()
    $ctx['started_at'] = $null
    $ctx['ended_at'] = $null
    return $ctx
}

function Initialize-G0OutputRoots {
    # EvidenceRoot/RawRoot from the freeze (mode-specific pair): must be
    # absent, independent sibling trees, and outside the repository
    # (E3 Initialize-OutputRoots discipline).
    param($Ctx)
    $freeze = $Ctx['freeze']
    $rootKey = Get-G0RootKeyForMode ([string] $Ctx['mode'])
    $evidence = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'evidence_roots') $rootKey))
    $raw = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'raw_roots') $rootKey))
    if ([System.IO.File]::Exists($evidence) -or [System.IO.Directory]::Exists($evidence)) {
        throw [G0RunnerError]::new('EvidenceRoot already exists; existing evidence is immutable and selective rerun is forbidden')
    }
    if ([System.IO.File]::Exists($raw) -or [System.IO.Directory]::Exists($raw)) {
        throw [G0RunnerError]::new('RawRoot already exists; existing raw collection is immutable and selective rerun is forbidden')
    }
    if (($evidence -ceq $raw) -or (Test-G0UnderPath $raw $evidence) -or (Test-G0UnderPath $evidence $raw)) {
        throw [G0RunnerError]::new('EvidenceRoot and RawRoot must be independent sibling trees')
    }
    if ($null -ne $Ctx['repo_root']) {
        $normalizedRepo = Get-G0NormalizedPath ([string] $Ctx['repo_root'])
        if ((Test-G0UnderPath $evidence $normalizedRepo) -or (Test-G0UnderPath $raw $normalizedRepo)) {
            throw [G0RunnerError]::new('EvidenceRoot and RawRoot must be outside the git repository')
        }
    }
    [void] [System.IO.Directory]::CreateDirectory($evidence)
    [void] [System.IO.Directory]::CreateDirectory($raw)
    $Ctx['evidence_path'] = $evidence
    $Ctx['raw_path'] = $raw
    $Ctx['transcript_path'] = [System.IO.Path]::Combine($evidence, 'transcript.redacted.jsonl')
    $script:ProjectionTranscript = $Ctx['transcript_path']
    $script:TranscriptIndex = 0
    $script:TranscriptPreviousHash = ('0' * 64)
    Write-G0TextUtf8NoBom $Ctx['transcript_path'] ''
}

function Invoke-G0SimulatedHdc {
    # In-process DryRun HDC layer (the PS host has no Python guarantee, so the
    # fake-hdc.py fixture lifecycle is mirrored as a stateful simulation).
    # Never touches the filesystem outside the caller-provided state and never
    # executes (or hashes) the frozen hdc path. Any argv outside the frozen
    # whitelist shapes is rejected with exit 5, mirroring fake-hdc.py.
    param([hashtable] $State, [string[]] $Argv, $Ctx)
    $av = [string[]] $Argv
    if ($av.Count -gt 0 -and $av[0] -ceq '-t') {
        if ($av.Count -lt 2) {
            return @{ exit_code = 5; stdout = ''; stderr = 'fake-hdc: -t requires a target value' }
        }
        $rest = [System.Collections.Generic.List[string]]::new()
        for ($i = 2; $i -lt $av.Count; $i++) { $rest.Add($av[$i]) }
        $av = $rest.ToArray()
    }
    $version = [string] (Get-G0ObjectValue (Get-G0ObjectValue $Ctx['freeze'] 'hdc') 'version')
    $scriptEnv = [string] $Ctx['dry_run_script']

    # ---- fixed-argv operations (exact tuple matches) --------------------
    if ($av.Count -eq 1 -and $av[0] -ceq 'version') {
        return @{ exit_code = 0; stdout = ($version + "`n"); stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'param', 'get', 'const.product.model')))) {
        return @{ exit_code = 0; stdout = "PLA-AL10`n"; stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'param', 'get', 'const.product.software.version')))) {
        return @{ exit_code = 0; stdout = "PLA-AL10 7.0.0.102(SP8C00E102R7P3)`n"; stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'mkdir', '-p', ($script:Staging + '/hap'))))) {
        $State['staging'] = $true
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 5 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'bm', 'install', '-p', ($script:Staging + '/hap'))))) {
        if (-not $State['hap_sent']) {
            return @{ exit_code = 6; stdout = ''; stderr = "fake-hdc: bm install without a sent hap`n" }
        }
        if ($scriptEnv -ceq 'install-fails') {
            # Exit 0 with no 'success' text and no installed state: drives the
            # runner's installhap-success-marker-missing branch specifically
            # (MAJOR-2 coverage of the sealed blocked ending).
            return @{ exit_code = 0; stdout = ''; stderr = "error: failed to install bundle`n" }
        }
        $State['installed'] = $true
        return @{ exit_code = 0; stdout = "install bundle successfully`n"; stderr = '' }
    }
    if ($av.Count -eq 8 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'hilog', '-T', $script:HilogTag, '-v', 'year', '-v', 'zone')))) {
        if ($scriptEnv -cne 'pass' -and $scriptEnv -cne 'dlopen-rejected') {
            return @{ exit_code = 3; stdout = ''; stderr = "fake-hdc: G0_DRYRUN_SCRIPT must be one of 'pass'|'dlopen-rejected'|'install-fails'`n" }
        }
        if (-not $State['entry_started']) {
            return @{ exit_code = 6; stdout = ''; stderr = "fake-hdc: hilog collected before the entry started`n" }
        }
        $line1 = ('2026-08-30 12:00:00.000  12345  67890 I {0}: g0 probe entry aboutToAppear' -f $script:HilogTag)
        $line2 = $null
        if ($scriptEnv -ceq 'pass') {
            $line2 = ('2026-08-30 12:00:00.100  12345  67890 I {0}: G0_RESULT|verdict=PASS|ok=true|pid=12345|stage=complete|dlopenLoaded=true|loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576' -f $script:HilogTag)
        } else {
            $line2 = ('2026-08-30 12:00:00.100  12345  67890 E {0}: G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|loaderErrno=2|loaderError={1}|hello=0|runtimeBytes=0' -f $script:HilogTag, $script:DlopenLoaderError)
        }
        return @{ exit_code = 0; stdout = ($line1 + "`n" + $line2 + "`n"); stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'rm', '-rf', $script:Staging)))) {
        $State['staging'] = $false
        $State['hap_sent'] = $false
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvEqual $av ([string[]] @('shell', 'ls', '-ld', $script:Staging)))) {
        if ($State['staging']) {
            return @{ exit_code = 0; stdout = ('drwxrwxr-x root root 4096 2026-08-30 12:00:00 ' + $script:Staging + "`n"); stderr = '' }
        }
        return @{ exit_code = 1; stdout = ''; stderr = ('ls: {0}: No such file or directory' -f $script:Staging) }
    }

    # ---- parameterized operations (exact arity, frozen values) ----------
    if ($av.Count -eq 5 -and $av[0] -ceq 'shell' -and $av[1] -ceq 'bm' -and $av[2] -ceq 'dump' -and $av[3] -ceq '-n') {
        if ($av[4] -cne $script:Bundle) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: bm dump outside the frozen G0 bundle`n" }
        }
        if ($State['installed']) {
            return @{ exit_code = 0; stdout = ('{"bundleName": "' + $script:Bundle + '", "installTime": "2026-08-30 12:00:00"}' + "`n"); stderr = '' }
        }
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 3 -and $av[0] -ceq 'shell' -and $av[1] -ceq 'pidof') {
        if ($av[2] -cne $script:Bundle) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: pidof outside the frozen G0 bundle`n" }
        }
        if ($null -ne $State['running_pid'] -and [string] $State['running_pid'] -ne '') {
            return @{ exit_code = 0; stdout = (([string] $State['running_pid']) + "`n"); stderr = '' }
        }
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 4 -and $av[0] -ceq 'file' -and $av[1] -ceq 'send' -and $av[3] -ceq ($script:Staging + '/hap/g0.hap')) {
        $State['staging'] = $true
        $State['hap_sent'] = $true
        return @{ exit_code = 0; stdout = "FileTransfer finish`n"; stderr = '' }
    }
    if ($av.Count -eq 9 -and (Test-G0ArgvPrefix $av ([string[]] @('shell', 'aa', 'start', '-a', $script:Ability))) `
            -and $av[5] -ceq '-b' -and $av[7] -ceq '-m') {
        if ($av[6] -cne $script:Bundle -or $av[8] -cne $script:Module) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: aa start outside the frozen G0 entry`n" }
        }
        if (-not $State['installed']) {
            return @{ exit_code = 6; stdout = ''; stderr = "fake-hdc: aa start on a bundle that is not installed`n" }
        }
        $State['running_pid'] = 12345
        $State['entry_started'] = $true
        return @{ exit_code = 0; stdout = "start bundle successfully`n"; stderr = '' }
    }
    if ($av.Count -eq 10 -and $av[0] -ceq 'shell' -and $av[1] -ceq 'find' `
            -and $av[2] -ceq '/data/log/faultlog/faultlogger' -and $av[3] -ceq '-maxdepth' `
            -and $av[4] -ceq '1' -and $av[5] -ceq '-type' -and $av[6] -ceq 'f' -and $av[7] -ceq '-name' `
            -and $av[9] -ceq '-print') {
        if ($av[8] -cne ('*{0}*' -f $script:Bundle)) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: find outside the frozen G0 bundle pattern`n" }
        }
        if ($scriptEnv -ceq 'dlopen-rejected' -and $State['entry_started']) {
            return @{ exit_code = 0; stdout = ('/data/log/faultlog/faultlogger/faultlogger-0-{0}-20260830120000' -f $script:Bundle); stderr = '' }
        }
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 4 -and (Test-G0ArgvPrefix $av ([string[]] @('shell', 'aa', 'force-stop')))) {
        if ($av[3] -cne $script:Bundle) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: aa force-stop outside the frozen G0 bundle`n" }
        }
        $State['running_pid'] = $null
        return @{ exit_code = 0; stdout = ''; stderr = '' }
    }
    if ($av.Count -eq 5 -and (Test-G0ArgvPrefix $av ([string[]] @('shell', 'bm', 'uninstall'))) -and $av[3] -ceq '-n') {
        if ($av[4] -cne $script:Bundle) {
            return @{ exit_code = 5; stdout = ''; stderr = "fake-hdc: bm uninstall outside the frozen G0 bundle`n" }
        }
        $State['installed'] = $false
        $State['running_pid'] = $null
        return @{ exit_code = 0; stdout = "uninstall bundle successfully`n"; stderr = '' }
    }

    # Anything else (missing parameters, extra parameters, unknown operation)
    # is rejected: the simulation mirrors the whitelist rejection contract.
    return @{ exit_code = 5; stdout = ''; stderr = ('fake-hdc: unsupported argv: ' + ($av -join ' ')) }
}

function Test-G0ArgvEqual {
    param([string[]] $Actual, [string[]] $Expected)
    if ($null -eq $Actual) { $Actual = [string[]] @() }
    if ($null -eq $Expected) { $Expected = [string[]] @() }
    if ($Actual.Count -ne $Expected.Count) { return $false }
    for ($i = 0; $i -lt $Actual.Count; $i++) {
        if ($Actual[$i] -cne $Expected[$i]) { return $false }
    }
    return $true
}

function Test-G0ArgvPrefix {
    param([string[]] $Actual, [string[]] $Prefix)
    if ($null -eq $Actual -or $Actual.Count -lt $Prefix.Count) { return $false }
    for ($i = 0; $i -lt $Prefix.Count; $i++) {
        if ($Actual[$i] -cne $Prefix[$i]) { return $false }
    }
    return $true
}

function Get-G0ContextTargetToken {
    param($Ctx)
    if ($Ctx['is_evidence']) { return $script:ActualTarget }
    return $script:DryRunTargetSentinel
}

function Write-G0RawText {
    param($Ctx, [string] $Name, [string] $Text)
    $path = [System.IO.Path]::Combine([string] $Ctx['raw_path'], $Name)
    Write-G0TextUtf8NoBom $path (Protect-G0SensitiveText $Text)
    return [System.IO.Path]::GetFileName($path)
}

function Invoke-G0Command {
    # One whitelisted one-shot hdc operation. Counts attempted/completed,
    # writes per-command stdout/stderr raw artifacts, and appends one
    # transcript line. Non-whitelist attempts are integrity violations.
    param($Ctx, [string] $Operation, [System.Collections.IDictionary] $Parameters = @{}, [switch] $AllowFailure)
    $canonical = ConvertTo-G0HdcOperation $Operation
    $Ctx['hdc_logical_calls'] = [int] $Ctx['hdc_logical_calls'] + 1
    if ($Ctx['hdc_operations'].ContainsKey($canonical)) {
        $Ctx['hdc_operations'][$canonical] = [int] $Ctx['hdc_operations'][$canonical] + 1
    } else {
        $Ctx['hdc_operations'][$canonical] = 1
    }
    $Ctx['command_attempted'] = [int] $Ctx['command_attempted'] + 1
    try {
        [string[]] $auditArgv = Get-G0HdcInvocation -Operation $Operation -Parameters $Parameters
    } catch {
        $Ctx['integrity_violations'].Add('nonwhitelist-command-attempt')
        Add-G0TranscriptRecord -Event 'command-rejected' -Details @{ operation = [string] $Operation; error = [string] $_.Exception.Message }
        throw [G0BlockedError]::new('nonwhitelist-command-attempt')
    }
    [string[]] $liveArgv = ConvertTo-G0LiveHdcArguments -AuditArguments $auditArgv -TargetToken (Get-G0ContextTargetToken $Ctx) -HapPath ([string] $Ctx['hap_live'])
    $Ctx['command_seq'] = [int] $Ctx['command_seq'] + 1
    $tag = '{0:d2}-{1}' -f [int] $Ctx['command_seq'], ($canonical.ToLowerInvariant())
    $stdoutName = $tag + '.stdout.txt'
    $stderrName = $tag + '.stderr.txt'
    $stdoutPath = [System.IO.Path]::Combine([string] $Ctx['raw_path'], $stdoutName)
    $stderrPath = [System.IO.Path]::Combine([string] $Ctx['raw_path'], $stderrName)
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $timedOut = $false
    $exitCode = 0
    $stdout = ''
    $stderr = ''
    if ([bool] $Ctx['simulated']) {
        $Ctx['hdc_process_starts'] = [int] $Ctx['hdc_process_starts'] + 1
        $sim = Invoke-G0SimulatedHdc -State $Ctx['sim_state'] -Argv $liveArgv -Ctx $Ctx
        $exitCode = [int] $sim['exit_code']
        $stdout = [string] $sim['stdout']
        $stderr = [string] $sim['stderr']
    } else {
        $proc = Invoke-G0ChildProcess -FilePath ([string] $Ctx['executable']) -ArgumentList $liveArgv `
            -TimeoutSeconds $script:HdcTimeoutSeconds -StdoutFile $stdoutPath -StderrFile $stderrPath
        $Ctx['hdc_process_starts'] = [int] $Ctx['hdc_process_starts'] + 1
        if ([bool] $proc['timed_out']) {
            $timedOut = $true
            $exitCode = 124
            $stdout = ''
            $stderr = ''
            if ([System.IO.File]::Exists($stdoutPath)) { $stdout = [System.IO.File]::ReadAllText($stdoutPath) }
            if ([System.IO.File]::Exists($stderrPath)) { $stderr = [System.IO.File]::ReadAllText($stderrPath) }
            if ([string]::IsNullOrEmpty($stderr)) { $stderr = 'hdc operation timeout' }
        } else {
            $exitCode = [int] $proc['exit_code']
            $stdout = ''
            $stderr = ''
            if ([System.IO.File]::Exists($stdoutPath)) { $stdout = [System.IO.File]::ReadAllText($stdoutPath) }
            if ([System.IO.File]::Exists($stderrPath)) { $stderr = [System.IO.File]::ReadAllText($stderrPath) }
        }
        # Raw artifacts are rewritten through the redaction projection.
        Write-G0TextUtf8NoBom $stdoutPath (Protect-G0SensitiveText $stdout)
        Write-G0TextUtf8NoBom $stderrPath (Protect-G0SensitiveText $stderr)
    }
    if ([bool] $Ctx['simulated']) {
        $null = Write-G0RawText $Ctx $stdoutName $stdout
        $null = Write-G0RawText $Ctx $stderrName $stderr
    }
    $stopwatch.Stop()
    $Ctx['command_completed'] = [int] $Ctx['command_completed'] + 1
    $result = @{
        exit_code = [int] $exitCode
        stdout    = $stdout
        stderr    = $stderr
        simulated = [bool] (-not $Ctx['is_evidence'])
    }
    $result['combined'] = (($stdout + "`n" + $stderr).Trim())
    Add-G0TranscriptRecord -Event 'hdc-command' -Details @{
        operation    = $canonical
        executable   = '<HDC_PATH>'
        arguments    = [string[]] $auditArgv
        exit_code    = [int] $exitCode
        duration_ms  = [int64] [math]::Floor($stopwatch.Elapsed.TotalMilliseconds)
        stdout_bytes = [int64] (Get-G0Utf8ByteCount $stdout)
        stderr_bytes = [int64] (Get-G0Utf8ByteCount $stderr)
        stdout_raw   = $stdoutName
        stderr_raw   = $stderrName
        simulated    = [bool] $result['simulated']
    }
    if ($timedOut) {
        throw [G0BlockedError]::new(($canonical.ToLowerInvariant() + '-command-timeout'))
    }
    if ($exitCode -ne 0 -and -not $AllowFailure) {
        throw [G0BlockedError]::new(($canonical.ToLowerInvariant() + '-command-failed'))
    }
    return $result
}

function Invoke-G0HilogCollect {
    # HilogStream: start the stream, collect until the window elapses (then
    # process-group kill) or until the stream EOFs early (the DryRun
    # simulation emits its scripted lines). Returns the analysis payload;
    # every raw line is preserved under RawRoot.
    param($Ctx, [int] $WindowSeconds)
    $canonical = 'HilogStream'
    $Ctx['hdc_logical_calls'] = [int] $Ctx['hdc_logical_calls'] + 1
    if ($Ctx['hdc_operations'].ContainsKey($canonical)) {
        $Ctx['hdc_operations'][$canonical] = [int] $Ctx['hdc_operations'][$canonical] + 1
    } else {
        $Ctx['hdc_operations'][$canonical] = 1
    }
    $Ctx['command_attempted'] = [int] $Ctx['command_attempted'] + 1
    [string[]] $auditArgv = Get-G0HdcInvocation -Operation $canonical
    [string[]] $liveArgv = ConvertTo-G0LiveHdcArguments -AuditArguments $auditArgv -TargetToken (Get-G0ContextTargetToken $Ctx) -HapPath ([string] $Ctx['hap_live'])
    $Ctx['command_seq'] = [int] $Ctx['command_seq'] + 1
    $stderrName = '{0:d2}-hilogstream.stderr.txt' -f [int] $Ctx['command_seq']
    $stderrPath = [System.IO.Path]::Combine([string] $Ctx['raw_path'], $stderrName)
    $stdoutTmpPath = $stderrPath + '.stdout-tmp'
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $timedOut = $false
    $stdout = ''
    if ([bool] $Ctx['simulated']) {
        $Ctx['hdc_process_starts'] = [int] $Ctx['hdc_process_starts'] + 1
        $sim = Invoke-G0SimulatedHdc -State $Ctx['sim_state'] -Argv $liveArgv -Ctx $Ctx
        $stdout = [string] $sim['stdout']
        $null = Write-G0RawText $Ctx $stderrName ([string] $sim['stderr'])
    } else {
        $proc = Invoke-G0ChildProcess -FilePath ([string] $Ctx['executable']) -ArgumentList $liveArgv `
            -TimeoutSeconds $WindowSeconds -StdoutFile $stdoutTmpPath -StderrFile $stderrPath
        $Ctx['hdc_process_starts'] = [int] $Ctx['hdc_process_starts'] + 1
        if ([bool] $proc['timed_out']) {
            $timedOut = $true
        }
        if ([System.IO.File]::Exists($stdoutTmpPath)) { $stdout = [System.IO.File]::ReadAllText($stdoutTmpPath) }
        try { if ([System.IO.File]::Exists($stdoutTmpPath)) { [System.IO.File]::Delete($stdoutTmpPath) } } catch { }
    }
    $stopwatch.Stop()
    $Ctx['command_completed'] = [int] $Ctx['command_completed'] + 1
    $hilogName = Write-G0RawText $Ctx 'hilog-raw.txt' $stdout
    $lines = [string[]] @(Get-G0TextLines $stdout)
    $markerLines = [System.Collections.Generic.List[string]]::new()
    $tagLines = [System.Collections.Generic.List[string]]::new()
    foreach ($line in $lines) {
        if ($line.IndexOf($script:MarkerToken, [System.StringComparison]::Ordinal) -ge 0) { $markerLines.Add($line) }
        if ($line.IndexOf($script:HilogTag, [System.StringComparison]::Ordinal) -ge 0) { $tagLines.Add($line) }
    }
    Add-G0TranscriptRecord -Event 'hdc-command' -Details @{
        operation           = $canonical
        executable          = '<HDC_PATH>'
        arguments           = [string[]] $auditArgv
        exit_code           = 0
        duration_ms         = [int64] [math]::Floor($stopwatch.Elapsed.TotalMilliseconds)
        window_seconds      = [int] $WindowSeconds
        window_elapsed_full = [bool] $timedOut
        lines_total         = [int] $lines.Count
        marker_lines        = [int] $markerLines.Count
        tag_lines           = [int] $tagLines.Count
        stdout_raw          = $hilogName
        stderr_raw          = $stderrName
        simulated           = [bool] (-not $Ctx['is_evidence'])
    }
    return @{
        stdout              = $stdout
        lines_total         = [int] $lines.Count
        tag_lines           = $tagLines.ToArray()
        marker_lines        = $markerLines.ToArray()
        window_seconds      = [int] $WindowSeconds
        window_elapsed_full = [bool] $timedOut
    }
}

function Invoke-G0WindDown {
    # State-gated best-effort cleanup: ForceStop(reason) + Uninstall when
    # anything may be installed, RemoveStaging when staging may exist.
    param($Ctx, [string] $ForceStopReason)
    $actions = [System.Collections.Generic.List[object]]::new()
    if ($Ctx['installed'] -or $Ctx['entry_started']) {
        foreach ($spec in @(
                @{ operation = 'ForceStop'; parameters = @{ Bundle = $script:Bundle; Reason = $ForceStopReason } },
                @{ operation = 'Uninstall'; parameters = @{ Bundle = $script:Bundle } })) {
            try {
                $result = Invoke-G0Command -Ctx $Ctx -Operation ([string] $spec['operation']) -Parameters $spec['parameters'] -AllowFailure
                $actions.Add(@{ operation = [string] $spec['operation']; reason = $ForceStopReason; exit_code = [int] $result['exit_code']; ok = ([int] $result['exit_code'] -eq 0) })
            } catch [G0BlockedError] {
                $actions.Add(@{ operation = [string] $spec['operation']; reason = $ForceStopReason; exit_code = $null; ok = $false })
                throw
            }
        }
        $Ctx['installed'] = $false
        $Ctx['entry_started'] = $false
    }
    if ($Ctx['staging_prepared']) {
        try {
            $result = Invoke-G0Command -Ctx $Ctx -Operation 'RemoveStaging' -AllowFailure
            $actions.Add(@{ operation = 'RemoveStaging'; reason = $ForceStopReason; exit_code = [int] $result['exit_code']; ok = ([int] $result['exit_code'] -eq 0) })
        } catch [G0BlockedError] {
            $actions.Add(@{ operation = 'RemoveStaging'; reason = $ForceStopReason; exit_code = $null; ok = $false })
            throw
        }
        $Ctx['staging_prepared'] = $false
    }
    foreach ($action in $actions) { $Ctx['cleanup_actions'].Add($action) }
}

function Invoke-G0AbsentProbes {
    # Post-cleanup directed absent probes: bundle dump, pidof, staging.
    param($Ctx)
    $dump = Invoke-G0Command -Ctx $Ctx -Operation 'BundleDump' -Parameters @{ Bundle = $script:Bundle } -AllowFailure
    $pidof = Invoke-G0Command -Ctx $Ctx -Operation 'PidOf' -Parameters @{ Bundle = $script:Bundle } -AllowFailure
    $staging = Invoke-G0Command -Ctx $Ctx -Operation 'StagingProbe' -AllowFailure
    $dumpAbsent = [string]::IsNullOrWhiteSpace([string] $dump['stdout']) -or (([string] $dump['stdout']).IndexOf($script:Bundle, [System.StringComparison]::Ordinal) -lt 0)
    $pidofAbsent = [string]::IsNullOrWhiteSpace([string] $pidof['stdout'])
    $stagingAbsent = (([string] $staging['combined']).IndexOf('no such file', [System.StringComparison]::OrdinalIgnoreCase) -ge 0)
    $Ctx['absent_probes'] = @{
        bundle_dump = $(if ($dumpAbsent) { 'absent' } else { 'present' })
        pidof       = $(if ($pidofAbsent) { 'absent' } else { 'present' })
        staging     = $(if ($stagingAbsent) { 'absent' } else { 'present' })
    }
    Add-G0TranscriptRecord -Event 'absent-probes' -Details @{ bundle_dump = $Ctx['absent_probes']['bundle_dump']; pidof = $Ctx['absent_probes']['pidof']; staging = $Ctx['absent_probes']['staging'] }
    $allAbsent = $true
    foreach ($value in [string[]] @($Ctx['absent_probes'].Values)) {
        if ($value -cne 'absent') { $allAbsent = $false }
    }
    $Ctx['cleanup_status'] = $(if ($allAbsent) { 'verified-clean' } else { 'incomplete' })
}

function Invoke-G0PreflightLive {
    # -Live preflight: clean worktree, ready freeze + double binding
    # (already enforced at load), real target token, hdc + HAP hash recompute.
    # Any failure raises before any evidence root is created (exit 1).
    param($Freeze, $RepoRootPath)
    if ($null -eq $RepoRootPath) {
        throw [G0RunnerError]::new('Live requires the git repository root')
    }
    $status = Get-G0GitStatusPorcelain ([string] $RepoRootPath)
    if (-not [string]::IsNullOrWhiteSpace($status)) {
        throw [G0RunnerError]::new('Live requires an empty `git status --porcelain` (worktree is dirty)')
    }
    Assert-G0TargetEnvironment
    $hdc = Get-G0ObjectValue $Freeze 'hdc'
    $hdcPath = Get-G0NormalizedPath ([string] (Get-G0ObjectValue $hdc 'path'))
    if (-not [System.IO.File]::Exists($hdcPath)) {
        throw [G0RunnerError]::new('frozen hdc executable missing')
    }
    Assert-G0FileHash -Label 'frozen hdc executable' -Path $hdcPath -Expected ([string] (Get-G0ObjectValue $hdc 'sha256'))
    $hapPath = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'artifacts') 'hap_path'))
    if (-not [System.IO.File]::Exists($hapPath)) {
        throw [G0RunnerError]::new('frozen HAP file missing')
    }
    Assert-G0FileHash -Label 'frozen HAP' -Path $hapPath -Expected ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'artifacts') 'hap_sha256'))
    return $true
}

function Invoke-G0VerifyTuple {
    # Step 2: Version/TupleModel/TupleBuild re-verification; literal tuple
    # match (version output must CONTAIN the frozen hdc version; model/build
    # compare verbatim after strip). Drift -> blocked ending.
    param($Ctx)
    $freeze = $Ctx['freeze']
    $versionResult = Invoke-G0Command -Ctx $Ctx -Operation 'Version' -AllowFailure
    $modelResult = Invoke-G0Command -Ctx $Ctx -Operation 'TupleModel' -AllowFailure
    $buildResult = Invoke-G0Command -Ctx $Ctx -Operation 'TupleBuild' -AllowFailure
    $observedVersion = ([string] $versionResult['stdout']).Trim()
    $observedModel = ([string] $modelResult['stdout']).Trim()
    $observedBuild = ([string] $buildResult['stdout']).Trim()
    $Ctx['observed_tuple'] = @{
        hdc_version_observed    = (Protect-G0SensitiveText $observedVersion)
        device_model_observed   = (Protect-G0SensitiveText $observedModel)
        full_system_build_observed = (Protect-G0SensitiveText $observedBuild)
    }
    $drift = [System.Collections.Generic.List[string]]::new()
    $expectedVersion = [string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'hdc') 'version')
    if (([string] $versionResult['stdout']).IndexOf($expectedVersion, [System.StringComparison]::Ordinal) -lt 0) {
        $drift.Add('hdc-version')
    }
    if ($observedModel -cne ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'target_tuple') 'device_model'))) {
        $drift.Add('device-model')
    }
    if ($observedBuild -cne ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'target_tuple') 'full_system_build'))) {
        $drift.Add('full-system-build')
    }
    Add-G0TranscriptRecord -Event 'tuple-verify' -Details @{ drift = [string[]] @($drift); observed = $Ctx['observed_tuple'] }
    $Ctx['steps'].Add(@{ step = 'tuple-verify'; result = $(if ($drift.Count -gt 0) { 'drift' } else { 'ok' }); drift = [string[]] @($drift) })
    if ($drift.Count -gt 0) {
        throw [G0BlockedError]::new('target-tuple-drift')
    }
}

function Invoke-G0VerifyBaseline {
    # Step 3: BundleDump (no install info) + PidOf (empty).
    param($Ctx)
    $dump = Invoke-G0Command -Ctx $Ctx -Operation 'BundleDump' -Parameters @{ Bundle = $script:Bundle } -AllowFailure
    $pidof = Invoke-G0Command -Ctx $Ctx -Operation 'PidOf' -Parameters @{ Bundle = $script:Bundle } -AllowFailure
    $dumpAbsent = [string]::IsNullOrWhiteSpace([string] $dump['stdout']) -or (([string] $dump['stdout']).IndexOf($script:Bundle, [System.StringComparison]::Ordinal) -lt 0)
    $pidofEmpty = [string]::IsNullOrWhiteSpace([string] $pidof['stdout'])
    Add-G0TranscriptRecord -Event 'baseline-probe' -Details @{ bundle_dump_absent = [bool] $dumpAbsent; pidof_empty = [bool] $pidofEmpty }
    $result = $(if ($dumpAbsent -and $pidofEmpty) { 'ok' } else { 'drift' })
    $Ctx['steps'].Add(@{ step = 'baseline-probe'; result = $result })
    if (-not $dumpAbsent) {
        throw [G0BlockedError]::new('baseline-bundle-present')
    }
    if (-not $pidofEmpty) {
        throw [G0BlockedError]::new('baseline-process-present')
    }
}

function Invoke-G0InstallAndStart {
    # Steps 4-5: MkdirStaging -> SendHap -> InstallHap (success marker) ->
    # StartEntry.
    param($Ctx)
    Invoke-G0Command -Ctx $Ctx -Operation 'MkdirStaging' | Out-Null
    $Ctx['staging_prepared'] = $true
    Invoke-G0Command -Ctx $Ctx -Operation 'SendHap' | Out-Null
    $install = Invoke-G0Command -Ctx $Ctx -Operation 'InstallHap'
    if (([string] $install['combined']).IndexOf('success', [System.StringComparison]::OrdinalIgnoreCase) -lt 0) {
        throw [G0BlockedError]::new('installhap-success-marker-missing')
    }
    $Ctx['installed'] = $true
    Invoke-G0Command -Ctx $Ctx -Operation 'StartEntry' -Parameters @{ Bundle = $script:Bundle } | Out-Null
    $Ctx['entry_started'] = $true
    Add-G0TranscriptRecord -Event 'install-and-start' -Details @{ installed = $true; entry_started = $true }
    $Ctx['steps'].Add(@{ step = 'install-and-start'; result = 'ok' })
}

function Resolve-G0Verdict {
    # Step 10: marker mapping + cleanup/integrity overrides.
    # Priority: integrity invalid > blocked reason > pass.
    param($Ctx, [string] $BlockedReason)
    $mapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($Ctx['marker_lines']))
    $Ctx['marker_mapping'] = $mapping
    $fields = $mapping['fields']
    if ($fields.ContainsKey('loaderError')) {
        $Ctx['loader_error'] = Protect-G0SensitiveText ([string] $fields['loaderError'])
    } else {
        $Ctx['loader_error'] = $null
    }
    if ($fields.ContainsKey('loaderErrno')) {
        $Ctx['loader_errno'] = [string] $fields['loaderErrno']
    } else {
        $Ctx['loader_errno'] = $null
    }
    $verdict = [string] $mapping['verdict']
    $failReason = $mapping['fail_reason']
    if (-not [string]::IsNullOrEmpty($BlockedReason)) {
        $verdict = 'blocked'
        $failReason = $BlockedReason
    }
    if (($Ctx['cleanup_status'] -cne 'verified-clean') -and ($verdict -ceq 'pass')) {
        $verdict = 'blocked'
        $failReason = 'cleanup-incomplete'
    }
    if ($Ctx['integrity_violations'].Count -gt 0) {
        $verdict = 'invalid'
        $failReason = 'integrity-violations'
    }
    return @{ verdict = $verdict; fail_reason = $failReason }
}

function Invoke-G0EndOfRunIntegrity {
    # Freeze-mismatch / worktree-drift re-checks at the end of the flow.
    param($Ctx)
    if ((Get-G0Sha256File ([string] $Ctx['freeze_path'])) -cne ([string] $Ctx['freeze_sha256'])) {
        $Ctx['integrity_violations'].Add('freeze-file-drift')
    }
    if ($Ctx['is_evidence']) {
        $freeze = $Ctx['freeze']
        try {
            Assert-G0FileHash -Label 'frozen hdc executable' `
                -Path (Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'hdc') 'path'))) `
                -Expected ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'hdc') 'sha256'))
        } catch {
            $Ctx['integrity_violations'].Add('hdc-sha256-mismatch')
        }
        try {
            Assert-G0FileHash -Label 'frozen HAP' `
                -Path ([string] $Ctx['hap_live']) `
                -Expected ([string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'artifacts') 'hap_sha256'))
        } catch {
            $Ctx['integrity_violations'].Add('hap-sha256-mismatch')
        }
        try {
            if (-not [string]::IsNullOrWhiteSpace((Get-G0GitStatusPorcelain ([string] $Ctx['repo_root'])))) {
                $Ctx['integrity_violations'].Add('worktree-dirty')
            }
        } catch {
            $Ctx['integrity_violations'].Add('repository-state-after-unavailable')
        }
    }
}

function Invoke-G0EmergencyCleanup {
    # Any runner exception: ForceStop(exception-cleanup) + Uninstall +
    # RemoveStaging best-effort; recorded where possible, never fatal.
    param($Ctx)
    foreach ($spec in @(
            @{ operation = 'ForceStop'; parameters = @{ Bundle = $script:Bundle; Reason = 'exception-cleanup' } },
            @{ operation = 'Uninstall'; parameters = @{ Bundle = $script:Bundle } },
            @{ operation = 'RemoveStaging'; parameters = @{} })) {
        try {
            [string[]] $auditArgv = Get-G0HdcInvocation -Operation ([string] $spec['operation']) -Parameters $spec['parameters']
            [string[]] $liveArgv = ConvertTo-G0LiveHdcArguments -AuditArguments $auditArgv -TargetToken (Get-G0ContextTargetToken $Ctx) -HapPath ([string] $Ctx['hap_live'])
            if ([bool] $Ctx['simulated']) {
                $null = Invoke-G0SimulatedHdc -State $Ctx['sim_state'] -Argv $liveArgv -Ctx $Ctx
            } else {
                $null = Invoke-G0CaptureProcess -FilePath ([string] $Ctx['executable']) -ArgumentList $liveArgv -TimeoutSeconds $script:HdcTimeoutSeconds
            }
            $Ctx['cleanup_actions'].Add(@{ operation = [string] $spec['operation']; reason = 'exception-cleanup'; exit_code = 0; ok = $true })
        } catch {
            $Ctx['cleanup_actions'].Add(@{ operation = [string] $spec['operation']; reason = 'exception-cleanup'; exit_code = $null; ok = $false; error = (Protect-G0SensitiveText ([string] $_.Exception.Message)) })
        }
    }
}

function Write-G0HashManifest {
    # hash-manifest.json: sha256 of every produced file. EvidenceRoot files
    # under `files` (the three seal-family files are bound by the seal itself,
    # never self-referential); RawRoot files under `external_raw_files`.
    param($Ctx)
    $manifestPath = [System.IO.Path]::Combine([string] $Ctx['evidence_path'], 'hash-manifest.json')
    $excluded = @('hash-manifest.json', 'scenario-results.json', 'campaign-seal.json')
    $files = [System.Collections.Generic.List[hashtable]]::new()
    foreach ($file in @(Get-ChildItem -LiteralPath ([string] $Ctx['evidence_path']) -Recurse -File -Force | Sort-Object -Property FullName)) {
        if ($excluded -ccontains $file.Name) { continue }
        $rel = [System.IO.Path]::GetRelativePath([string] $Ctx['evidence_path'], $file.FullName).Replace('\', '/')
        $files.Add(@{ path = $rel; sha256 = (Get-G0Sha256File $file.FullName); bytes = [int64] $file.Length })
    }
    $files.Sort([System.Comparison[hashtable]] { param($a, $b) [string]::CompareOrdinal([string] $a['path'], [string] $b['path']) })
    $external = [System.Collections.Generic.List[hashtable]]::new()
    foreach ($file in @(Get-ChildItem -LiteralPath ([string] $Ctx['raw_path']) -Recurse -File -Force | Sort-Object -Property FullName)) {
        $rel = [System.IO.Path]::GetRelativePath([string] $Ctx['raw_path'], $file.FullName).Replace('\', '/')
        $external.Add(@{ path = $rel; sha256 = (Get-G0Sha256File $file.FullName); bytes = [int64] $file.Length })
    }
    $external.Sort([System.Comparison[hashtable]] { param($a, $b) [string]::CompareOrdinal([string] $a['path'], [string] $b['path']) })
    Write-G0JsonFile $manifestPath @{
        schema_version        = 1
        algorithm             = 'SHA-256'
        generated_at          = (Get-G0NowIso)
        transcript_chain_head = (Get-G0TranscriptChainHead ([string] $Ctx['transcript_path']))
        scope                 = 'all produced evidence/raw files; scenario-results.json is bound by campaign-seal.json to avoid a self-reference cycle'
        files                 = $files.ToArray()
        external_raw_files    = $external.ToArray()
    }
    return $manifestPath
}

function New-G0ScenarioResults {
    # scenario-results.json: every measured field (verdict, markers,
    # hdc_execution counts, cleanup result, integrity violations).
    # NOTE: $FailReason must stay untyped so a null reason (pass) is written
    # as JSON null, mirroring the Python runner.
    param($Ctx, [string] $Verdict, $FailReason, [string] $ManifestPath, $RepositoryCleanBefore)
    $freeze = $Ctx['freeze']
    $deduped = [System.Collections.Generic.List[string]]::new()
    foreach ($violation in $Ctx['integrity_violations']) {
        if (-not $deduped.Contains($violation)) { $deduped.Add($violation) }
    }
    $faultStatus = 'not-run'
    if ($null -ne $Ctx['fault_lines']) {
        if ([int] $Ctx['fault_lines'] -gt 0) { $faultStatus = 'fault-lines-present' } else { $faultStatus = 'no-fault-lines' }
    }
    $repositoryCleanAfter = $null
    if ($Ctx['is_evidence']) {
        $repositoryCleanAfter = (-not [string]::IsNullOrWhiteSpace((Get-G0GitStatusPorcelain ([string] $Ctx['repo_root']))))
    }
    # Pre-compute array-valued evidence fields as real object[] WITHOUT the
    # $(if ...) subexpression: $() enumerates pipeline output and collapses a
    # single-element array back to a scalar (review MAJOR-1 residue here).
    $markerRawLines = [object[]] @()
    if ($Ctx['markers'].ContainsKey('raw_lines')) {
        $markerRawLines = [object[]] @($Ctx['markers']['raw_lines'])
    }
    $integrityList = [object[]] @($deduped.ToArray())
    return @{
        schema_version           = 1
        record_kind              = 'g0-scenario-results'
        evidence_id              = [string] (Get-G0ObjectValue $freeze 'evidence_id')
        campaign_id              = [string] (Get-G0ObjectValue $freeze 'campaign_id')
        authorization_id         = [string] (Get-G0ObjectValue $freeze 'authorization_id')
        attempt                  = [string] (Get-G0ObjectValue $freeze 'attempt')
        plan_status              = [string] (Get-G0ObjectValue $freeze 'plan_status')
        execution_mode           = [string] $Ctx['mode']
        is_evidence              = [bool] $Ctx['is_evidence']
        non_evidence_reason      = $(if ($Ctx['is_evidence']) { 'N/A' } else { $script:DryRunNonEvidenceReason })
        bundle                   = $script:Bundle
        ability                  = $script:Ability
        module                   = $script:Module
        staging                  = $script:Staging
        hilog_tag                = $script:HilogTag
        scenario_window_seconds  = $script:ScenarioWindowSeconds
        marker_format            = 'G0_RESULT|k=v|... (pipe-delimited, pre-registered)'
        target_tuple_expected    = (Get-G0ObjectValue $freeze 'target_tuple')
        target_tuple_observed    = $Ctx['observed_tuple']
        hdc                      = @{
            version = [string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'hdc') 'version')
            sha256  = [string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'hdc') 'sha256')
        }
        hap_sha256               = [string] (Get-G0ObjectValue (Get-G0ObjectValue $freeze 'artifacts') 'hap_sha256')
        elf_profile              = (Get-G0ObjectValue $freeze 'elf_profile')
        freeze_file_sha256       = [string] $Ctx['freeze_sha256']
        runner_py_sha256         = [string] (Get-G0ObjectValue $freeze 'runner_py_sha256')
        runner_ps1_sha256        = [string] (Get-G0ObjectValue $freeze 'runner_ps1_sha256')
        started_at               = $Ctx['started_at']
        ended_at                 = $Ctx['ended_at']
        verdict                  = $Verdict
        fail_reason              = $FailReason
        markers                  = @{
            count                = $(if ($Ctx['markers'].ContainsKey('count')) { $Ctx['markers']['count'] } else { 0 })
            raw_lines            = $markerRawLines
            fields               = $(if ($Ctx['markers'].ContainsKey('fields')) { $Ctx['markers']['fields'] } else { @{} })
            hilog_lines_total    = $(if ($Ctx['markers'].ContainsKey('hilog_lines_total')) { $Ctx['markers']['hilog_lines_total'] } else { 0 })
            hilog_tag_lines_total = $(if ($Ctx['markers'].ContainsKey('hilog_tag_lines_total')) { $Ctx['markers']['hilog_tag_lines_total'] } else { 0 })
            window_seconds       = $(if ($Ctx['markers'].ContainsKey('window_seconds')) { $Ctx['markers']['window_seconds'] } else { 0 })
            window_elapsed_full  = $(if ($Ctx['markers'].ContainsKey('window_elapsed_full')) { $Ctx['markers']['window_elapsed_full'] } else { $false })
        }
        loader_error             = $Ctx['loader_error']
        loader_errno             = $Ctx['loader_errno']
        fault_probe              = @{
            fault_lines = $Ctx['fault_lines']
            status      = $faultStatus
        }
        hdc_execution            = @{
            logical_calls     = [int] $Ctx['hdc_logical_calls']
            process_starts    = [int] $Ctx['hdc_process_starts']
            operations        = $Ctx['hdc_operations']
            command_attempted = [int] $Ctx['command_attempted']
            command_completed = [int] $Ctx['command_completed']
        }
        steps                    = $Ctx['steps'].ToArray()
        cleanup                  = @{
            actions = $Ctx['cleanup_actions'].ToArray()
            status  = [string] $Ctx['cleanup_status']
        }
        absent_probes            = $Ctx['absent_probes']
        integrity_violations     = $integrityList
        host_hdc_processes_after = (Get-G0HdcProcessCount)
        repository_clean_before  = $RepositoryCleanBefore
        repository_clean_after   = $repositoryCleanAfter
        transcript_reference     = @{
            path       = 'transcript.redacted.jsonl'
            sha256     = (Get-G0Sha256File ([string] $Ctx['transcript_path']))
            chain_head = (Get-G0TranscriptChainHead ([string] $Ctx['transcript_path']))
        }
        hash_manifest_reference  = @{
            path   = 'hash-manifest.json'
            sha256 = (Get-G0Sha256File $ManifestPath)
        }
        scope_statement          = 'Exact frozen G0 stock-Go arm64 c-shared loadability reachability only; no E4-E7, product, data-plane, or E8 OPEN conclusion.'
        reviewers                = 'pending'
    }
}

function Write-G0CampaignSeal {
    # campaign-seal.json: binds scenario-results.json + hash-manifest.json
    # with their sha256, plus sealed_at / final_exit_code / run_status /
    # fail_reason / verdict / chain head. $FailReason stays untyped so a null
    # reason (pass) is written as JSON null.
    param($Ctx, [string] $Verdict, $FailReason)
    $recordPath = [System.IO.Path]::Combine([string] $Ctx['evidence_path'], 'scenario-results.json')
    $manifestPath = [System.IO.Path]::Combine([string] $Ctx['evidence_path'], 'hash-manifest.json')
    $freeze = $Ctx['freeze']
    Write-G0JsonFile ([System.IO.Path]::Combine([string] $Ctx['evidence_path'], 'campaign-seal.json')) @{
        schema_version        = 1
        algorithm             = 'SHA-256'
        campaign_id           = [string] (Get-G0ObjectValue $freeze 'campaign_id')
        evidence_id           = [string] (Get-G0ObjectValue $freeze 'evidence_id')
        execution_mode        = [string] $Ctx['mode']
        is_evidence           = [bool] $Ctx['is_evidence']
        record                = @{ path = 'scenario-results.json'; sha256 = (Get-G0Sha256File $recordPath) }
        manifest              = @{ path = 'hash-manifest.json'; sha256 = (Get-G0Sha256File $manifestPath) }
        sealed_at             = (Get-G0NowIso)
        final_exit_code       = 0
        run_status            = 'completed'
        fail_reason           = $FailReason
        verdict               = $Verdict
        transcript_chain_head = (Get-G0TranscriptChainHead ([string] $Ctx['transcript_path']))
    }
}

function Invoke-G0Campaign {
    # Full single-scenario S1 flow. Completed flows (pass/blocked/invalid
    # verdicts) write scenario-results + hash-manifest + campaign-seal and exit
    # 0 printing `VERDICT=<verdict>`. Any runner exception triggers the
    # exception-cleanup chain and exits 1 WITHOUT a seal.
    param($Freeze, [string] $Mode, $RepoRootPath, [string] $FreezePath)
    $script:IsEvidence = ($Mode -ceq 'live')
    $script:ExecutionMode = $Mode
    $script:FreezeManifest = $freeze

    $repositoryCleanBefore = $null
    if ($script:IsEvidence) {
        Invoke-G0PreflightLive -Freeze $Freeze -RepoRootPath $RepoRootPath | Out-Null
        $repositoryCleanBefore = $true
    } else {
        $scriptEnv = [string] [System.Environment]::GetEnvironmentVariable('G0_DRYRUN_SCRIPT')
        if (-not $script:DryRunScripts.Contains($scriptEnv)) {
            throw [G0RunnerError]::new("DryRun requires environment G0_DRYRUN_SCRIPT in {'pass','dlopen-rejected','install-fails'}")
        }
    }

    $Ctx = New-G0CampaignContext -Freeze $Freeze -Mode $Mode -RepoRootPath $RepoRootPath -FreezePath $FreezePath
    if (-not [bool] $Ctx['simulated']) {
        $Ctx['executable'] = Get-G0NormalizedPath ([string] (Get-G0ObjectValue (Get-G0ObjectValue $Freeze 'hdc') 'path'))
    }
    Initialize-G0OutputRoots -Ctx $Ctx
    $Ctx['started_at'] = Get-G0NowIso
    Add-G0TranscriptRecord -Event 'campaign-start' -Details @{
        campaign_id        = [string] (Get-G0ObjectValue $Freeze 'campaign_id')
        evidence_id        = [string] (Get-G0ObjectValue $Freeze 'evidence_id')
        attempt            = [string] (Get-G0ObjectValue $Freeze 'attempt')
        plan_status        = [string] (Get-G0ObjectValue $Freeze 'plan_status')
        execution_mode     = $Mode
        is_evidence        = [bool] $Ctx['is_evidence']
        freeze_file_sha256 = [string] $Ctx['freeze_sha256']
    }
    try {
        $blockedReason = $null
        try {
            if ([bool] $Ctx['is_evidence']) {
                Add-G0TranscriptRecord -Event 'preflight' -Details @{ repository_clean = $true; hdc_sha256_verified = $true; hap_sha256_verified = $true }
                $Ctx['steps'].Add(@{ step = 'preflight'; result = 'ok' })
            }
            Invoke-G0VerifyTuple -Ctx $Ctx
            Invoke-G0VerifyBaseline -Ctx $Ctx
            Invoke-G0InstallAndStart -Ctx $Ctx
            $stream = Invoke-G0HilogCollect -Ctx $Ctx -WindowSeconds $script:ScenarioWindowSeconds
            $Ctx['marker_lines'] = [string[]] @($stream['marker_lines'])
            $protectedMarkerLines = [System.Collections.Generic.List[object]]::new()
            foreach ($markerLine in @($stream['marker_lines'])) {
                $protectedMarkerLines.Add((Protect-G0SensitiveText ([string] $markerLine)))
            }
            $Ctx['markers'] = @{
                count                = [int] @($stream['marker_lines']).Count
                raw_lines            = $protectedMarkerLines.ToArray()
                fields               = @{}
                hilog_lines_total    = [int] $stream['lines_total']
                hilog_tag_lines_total = [int] @($stream['tag_lines']).Count
                window_seconds       = [int] $stream['window_seconds']
                window_elapsed_full  = [bool] $stream['window_elapsed_full']
            }
            Add-G0TranscriptRecord -Event 'hilog-collect' -Details @{ count = [int] $Ctx['markers']['count']; window_elapsed_full = [bool] $Ctx['markers']['window_elapsed_full'] }
            $Ctx['steps'].Add(@{ step = 'hilog-collect'; result = 'ok'; marker_count = [int] $Ctx['markers']['count'] })
            $fault = Invoke-G0Command -Ctx $Ctx -Operation 'FaultProbe' -AllowFailure
            $faultLines = 0
            foreach ($line in (Get-G0TextLines ([string] $fault['stdout']))) {
                if (-not [string]::IsNullOrWhiteSpace($line)) { $faultLines++ }
            }
            $Ctx['fault_lines'] = $faultLines
            Add-G0TranscriptRecord -Event 'fault-probe' -Details @{ fault_lines = $Ctx['fault_lines'] }
            $Ctx['steps'].Add(@{ step = 'fault-probe'; result = 'ok' })
        } catch [G0BlockedError] {
            $blockedReason = [string] $_.Exception.Reason
            Add-G0TranscriptRecord -Event 'campaign-blocked' -Details @{ reason = $blockedReason }
            $Ctx['steps'].Add(@{ step = 'flow'; result = 'blocked'; reason = $blockedReason })
        }
        # Wind-down + directed absent probes (final-cleanup), for every ending.
        try {
            Invoke-G0WindDown -Ctx $Ctx -ForceStopReason 'final-cleanup'
            Invoke-G0AbsentProbes -Ctx $Ctx
        } catch [G0BlockedError] {
            $Ctx['cleanup_status'] = 'incomplete'
        }
        Add-G0TranscriptRecord -Event 'cleanup' -Details @{ status = [string] $Ctx['cleanup_status']; actions = (Protect-G0SensitiveData $Ctx['cleanup_actions'].ToArray()) }
        Invoke-G0EndOfRunIntegrity -Ctx $Ctx
        $Ctx['ended_at'] = Get-G0NowIso
        $resolved = Resolve-G0Verdict -Ctx $Ctx -BlockedReason $blockedReason
        $verdict = [string] $resolved['verdict']
        $failReason = $resolved['fail_reason']
        if ($null -ne $Ctx['marker_mapping']) {
            $Ctx['markers']['fields'] = $Ctx['marker_mapping']['fields']
        } else {
            $Ctx['markers']['fields'] = @{}
        }
        Add-G0TranscriptRecord -Event 'verdict-resolved' -Details @{ verdict = $verdict; fail_reason = $failReason; integrity_violations = [string[]] @($Ctx['integrity_violations'].ToArray()) }
        $manifestPath = Write-G0HashManifest -Ctx $Ctx
        $record = New-G0ScenarioResults -Ctx $Ctx -Verdict $verdict -FailReason $failReason -ManifestPath $manifestPath -RepositoryCleanBefore $repositoryCleanBefore
        Write-G0JsonFile ([System.IO.Path]::Combine([string] $Ctx['evidence_path'], 'scenario-results.json')) $record
        Write-G0CampaignSeal -Ctx $Ctx -Verdict $verdict -FailReason $failReason
        $failText = $(if ($null -eq $failReason) { 'None' } else { [string] $failReason })
        Write-G0Out ('G0_CAMPAIGN_RESULT mode={0} verdict={1} fail_reason={2} evidence_root={3} raw_root={4} hdc_process_starts={5}' -f $Mode, $verdict, $failText, [string] $Ctx['evidence_path'], [string] $Ctx['raw_path'], [int] $Ctx['hdc_process_starts'])
        Write-G0Out ('VERDICT={0}' -f $verdict)
        return 0
    } catch {
        # Runner-level failure: best-effort exception cleanup, no seal, exit 1.
        try {
            Invoke-G0EmergencyCleanup -Ctx $Ctx
        } catch { }
        throw
    }
}

# =====================================================================
# Section 9: Embedded -SelfTest (basic import-level checks)
# =====================================================================

function Test-G0SelfTestRaises {
    param([scriptblock] $Fn)
    try {
        & $Fn
        return $false
    } catch {
        return $true
    }
}

function Invoke-G0EmbeddedSelfTest {
    # Basic import-level self-check: pure functions only - no hdc, no device,
    # no evidence files. Prints SELFTEST_PASS/FAIL lines and ends with
    # SELFTEST_RESULT=pass|fail HDC_PROCESSES=<n>. Returns 0/1.
    $failures = [System.Collections.Generic.List[string]]::new()
    $hdcProcessCount = Get-G0HdcProcessCount

    function Check {
        param([string] $Name, [bool] $Condition)
        if ($Condition) {
            Write-G0Out ('SELFTEST_PASS={0}' -f $Name)
        } else {
            $script:G0SelfTestFailures.Add($Name)
            Write-G0Out ('SELFTEST_FAIL={0}' -f $Name)
        }
    }

    $script:G0SelfTestFailures = $failures

    if ($hdcProcessCount -lt 0) {
        Write-G0Out 'SELFTEST_WARN=hdc-process-count-unknown'
    }
    Check 'sha256-empty-vector' ((Get-G0Sha256Text '') -ceq 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')
    Check 'whitelist-operation-count' (@($script:HdcWhitelist.Keys).Count -eq 15)
    [string[]] $argvVersion = Get-G0HdcInvocation -Operation 'Version'
    Check 'invocation-version' (Test-G0ArgvEqual $argvVersion ([string[]] @('version')))
    [string[]] $argvTupleModel = Get-G0HdcInvocation -Operation 'TupleModel'
    Check 'invocation-tuple-model' (Test-G0ArgvEqual $argvTupleModel ([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'param', 'get', 'const.product.model')))
    [string[]] $argvBundleDump = Get-G0HdcInvocation -Operation 'BundleDump' -Parameters @{ Bundle = $script:Bundle }
    Check 'invocation-bundledump' (Test-G0ArgvEqual $argvBundleDump ([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'bm', 'dump', '-n', $script:Bundle)))
    [string[]] $argvPidOf = Get-G0HdcInvocation -Operation 'PidOf' -Parameters @{ Bundle = $script:Bundle }
    Check 'invocation-pidof-ui-process' (Test-G0ArgvEqual $argvPidOf ([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'pidof', $script:Bundle)))
    [string[]] $argvSendHap = Get-G0HdcInvocation -Operation 'SendHap'
    Check 'invocation-sendhap' (Test-G0ArgvEqual $argvSendHap ([string[]] @('-t', $script:TargetPlaceholder, 'file', 'send', $script:HapPlaceholder, ($script:Staging + '/hap/g0.hap'))))
    [string[]] $argvHilog = Get-G0HdcInvocation -Operation 'HilogStream'
    Check 'invocation-hilogstream' (Test-G0ArgvEqual $argvHilog ([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'hilog', '-T', $script:HilogTag, '-v', 'year', '-v', 'zone')))
    [string[]] $argvForceStop = Get-G0HdcInvocation -Operation 'ForceStop' -Parameters @{ Bundle = $script:Bundle; Reason = 'final-cleanup' }
    Check 'invocation-forcestop' (Test-G0ArgvEqual $argvForceStop ([string[]] @('-t', $script:TargetPlaceholder, 'shell', 'aa', 'force-stop', $script:Bundle)))
    Check 'reject-unknown-operation' (Test-G0SelfTestRaises { Get-G0HdcInvocation -Operation 'Screenshot' })
    Check 'reject-extra-parameter' (Test-G0SelfTestRaises { Get-G0HdcInvocation -Operation 'Version' -Parameters @{ Bundle = $script:Bundle } })
    Check 'reject-missing-parameter' (Test-G0SelfTestRaises { Get-G0HdcInvocation -Operation 'PidOf' })
    Check 'reject-foreign-bundle' (Test-G0SelfTestRaises { Get-G0HdcInvocation -Operation 'PidOf' -Parameters @{ Bundle = 'cn.example.other' } })
    Check 'reject-forcestop-bad-reason' (Test-G0SelfTestRaises { Get-G0HdcInvocation -Operation 'ForceStop' -Parameters @{ Bundle = $script:Bundle; Reason = 'reboot' } })
    [string[]] $argvLower = Get-G0HdcInvocation -Operation 'bundledump' -Parameters @{ bundle = $script:Bundle }
    [string[]] $argvUpper = Get-G0HdcInvocation -Operation 'BundleDump' -Parameters @{ Bundle = $script:Bundle }
    Check 'case-insensitive-aliases' (Test-G0ArgvEqual $argvLower $argvUpper)
    Check 'target-token-positive' (Test-G0PhysicalTargetToken '192.168.1.100:5555')
    $negativeTokens = [string[]] @('', '   ', ' x', 'x ', 'a b', 'a,b', 'a;b', '-t', 'PHYS-1', 'phys-1', '<PHYS_1_TARGET>')
    $allNegative = $true
    foreach ($token in $negativeTokens) {
        if (Test-G0PhysicalTargetToken $token) { $allNegative = $false }
    }
    if (Test-G0PhysicalTargetToken ([string] $null)) { $allNegative = $false }
    Check 'target-token-negative-placeholder' $allNegative
    $passMarker = 'x G0GoProbe: G0_RESULT|verdict=PASS|ok=true|pid=1|stage=complete|dlopenLoaded=true|loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576'
    Check 'marker-pass-mapping' ((Get-G0MarkerMapping -MarkerLines ([object[]] @($passMarker)))['verdict'] -ceq 'pass')
    $dlopenMarker = 'G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|loaderErrno=2|loaderError=initial-exec TLS resolves to dynamic definition'
    $dlopenMapping = Get-G0MarkerMapping -MarkerLines ([object[]] @($dlopenMarker))
    $dlopenFields = $dlopenMapping['fields']
    Check 'marker-dlopen-mapping' (($dlopenMapping['verdict'] -ceq 'blocked') -and ($dlopenMapping['fail_reason'] -ceq 'dlopen-blocked') -and ([string] $dlopenFields['loaderError']) -ceq $script:DlopenLoaderError -and ([string] $dlopenFields['loaderErrno']) -ceq '2')
    Check 'marker-missing-mapping' ((Get-G0MarkerMapping -MarkerLines ([object[]] @()))['fail_reason'] -ceq 'marker-missing')
    Check 'marker-ambiguous-mapping' ((Get-G0MarkerMapping -MarkerLines ([object[]] @('G0_RESULT|verdict=PASS', 'G0_RESULT|verdict=PASS')))['fail_reason'] -ceq 'marker-ambiguous')
    Check 'marker-drift-mapping' ((Get-G0MarkerMapping -MarkerLines ([object[]] @('G0_RESULT|verdict=PASS|ok=true|stage=complete|hello=7|runtimeBytes=1')))['fail_reason'] -ceq 'drift')
    Check 'ps-count-first-column-only' ((Get-G0HdcCountFromPsOutput "hdc -t foo`nfake-hdc -t bar`npython3 x.py`nhdcx y`nsshd: /usr/sbin/sshd -D") -eq 1)
    if ($hdcProcessCount -ge 0) {
        Check 'host-hdc-processes-zero' ($hdcProcessCount -eq 0)
    }
    $resultText = 'fail'
    if ($failures.Count -eq 0) { $resultText = 'pass' }
    Write-G0Out ('SELFTEST_RESULT={0} HDC_PROCESSES={1}' -f $resultText, $hdcProcessCount)
    if ($failures.Count -gt 0) { return 1 }
    return 0
}

# =====================================================================
# Section 10: Main flow (parameter-set entry, mode dispatch)
# =====================================================================

function Invoke-G0RunnerMain {
    # Gate order: parameter sets (pwsh binding) -> mode exclusivity ->
    # version/selftest early exits -> freeze load/validate -> dispatch.
    # Exit codes: 0=pass/completed, 1=pre-record gate or any pre-campaign/
    # runner validation error, 2=probe blocked (TargetBindingConfirm).
    param(
        [switch] $VersionFlag,
        [switch] $SelfTestFlag,
        [switch] $TargetBindingConfirmFlag,
        [switch] $DryRunFlag,
        [switch] $LiveFlag,
        [string] $FreezeArg,
        [string] $ConfirmationRecordArg
    )
    try {
        if ($VersionFlag) {
            Write-G0Out ('g0-phys-probe-campaign.ps1 {0}' -f $script:RunnerVersion)
            return 0
        }
        if ($SelfTestFlag) {
            return (Invoke-G0EmbeddedSelfTest)
        }
        $mode = $null
        if ($TargetBindingConfirmFlag) { $mode = 'target-binding-confirm' }
        elseif ($DryRunFlag) { $mode = 'dry-run' }
        elseif ($LiveFlag) { $mode = 'live' }
        if ([string]::IsNullOrEmpty($mode)) {
            throw [G0RunnerError]::new('no run mode selected (one of -Version, -SelfTest, -TargetBindingConfirm, -DryRun, -Live is required)')
        }
        if ($mode -cne 'target-binding-confirm' -and -not [string]::IsNullOrWhiteSpace($ConfirmationRecordArg)) {
            throw [G0RunnerError]::new('-ConfirmationRecord is only valid with -TargetBindingConfirm')
        }
        if ($mode -ceq 'target-binding-confirm' -and [string]::IsNullOrWhiteSpace($ConfirmationRecordArg)) {
            throw [G0RunnerError]::new('-TargetBindingConfirm requires -ConfirmationRecord')
        }
        if ([string]::IsNullOrWhiteSpace($FreezeArg)) {
            throw [G0RunnerError]::new(('-Freeze is required for {0}' -f $mode))
        }
        $repoRootPath = Resolve-G0RepositoryRoot
        $freezePath = Get-G0NormalizedPath $FreezeArg
        $freeze = Load-G0Freeze -FreezePath $freezePath -Mode $mode -RepoRootPath $repoRootPath
        if ($mode -ceq 'target-binding-confirm') {
            return (Invoke-G0TargetBindingConfirm -Freeze $freeze -FreezePath $freezePath -ConfirmationRecordArg $ConfirmationRecordArg -RepoRootPath $repoRootPath)
        }
        return (Invoke-G0Campaign -Freeze $freeze -Mode $mode -RepoRootPath $RepoRootPath -FreezePath $freezePath)
    } catch [G0PreRecordGateError] {
        Write-G0Err (Protect-G0SensitiveText ([string] $_.Exception.Message))
        return 1
    } catch {
        Write-G0Err ('RUNNER_FAILURE={0}' -f (Protect-G0SensitiveText ([string] $_.Exception.Message)))
        return 1
    }
}

# Dot-source guard: when this file is dot-sourced (e.g. by the selftest, which
# imports the pure functions), the main dispatch is skipped; when executed
# (-File / call operator), it runs and exits with the runner's exit code.
if ($MyInvocation.InvocationName -ne '.') {
    exit (Invoke-G0RunnerMain -VersionFlag:$Version -SelfTestFlag:$SelfTest `
        -TargetBindingConfirmFlag:$TargetBindingConfirm -DryRunFlag:$DryRun `
        -LiveFlag:$Live -FreezeArg:$Freeze -ConfirmationRecordArg:$ConfirmationRecord)
}


