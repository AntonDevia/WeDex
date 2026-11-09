param([Parameter(Mandatory=$true)][string]$Spec)
$ErrorActionPreference='Stop'
function FileHash([string]$Path) {
    $stream=[IO.File]::OpenRead($Path)
    $algorithm=[Security.Cryptography.SHA256]::Create()
    try {return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-','')}
    finally {$stream.Dispose(); $algorithm.Dispose()}
}
$config=Get-Content -LiteralPath $Spec -Raw | ConvertFrom-Json
$target=[IO.Path]::GetFullPath($config.target)
$source=[IO.Path]::GetFullPath($config.source)
if ([IO.Path]::GetFileName($target) -notmatch '\.exe$' -or -not (Test-Path -LiteralPath $source)) {throw 'Invalid WeDex update'}
if ((FileHash $source) -ne $config.sha256) {throw 'WeDex update checksum changed'}
$process=Get-Process -Id $config.pid -ErrorAction SilentlyContinue
if ($process) {$process.WaitForExit()}
$backup=$target+'.previous'
$next=$target+'.update-new'
try {
    Copy-Item -LiteralPath $source -Destination $next -Force
    if ((FileHash $next) -ne $config.sha256) {throw 'Copied update checksum mismatch'}
    for ($attempt=0;$attempt -lt 30;$attempt++) {
        try {Copy-Item -LiteralPath $target -Destination $backup -Force; Move-Item -LiteralPath $next -Destination $target -Force; break}
        catch {if ($attempt -eq 29) {throw}; Start-Sleep -Milliseconds 500}
    }
    Start-Process -FilePath $target
} catch {
    if (Test-Path -LiteralPath $backup) {Copy-Item -LiteralPath $backup -Destination $target -Force}
    $_ | Out-File -LiteralPath ($target+'.update-error.log') -Encoding utf8
}
