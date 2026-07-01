[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory = $true)]
    [string]$ExePath,

    [Parameter(Mandatory = $true)]
    [string]$ConfigPath,

    [string]$TaskName = "EEW Quake Notify System"
)

$ErrorActionPreference = "Stop"

$resolvedExe = Resolve-Path -LiteralPath $ExePath
$resolvedConfig = Resolve-Path -LiteralPath $ConfigPath
$workingDirectory = Split-Path -Parent $resolvedExe.Path
$arguments = "--config `"$($resolvedConfig.Path)`""

$action = New-ScheduledTaskAction `
    -Execute $resolvedExe.Path `
    -Argument $arguments `
    -WorkingDirectory $workingDirectory
$trigger = New-ScheduledTaskTrigger -AtLogOn
$settings = New-ScheduledTaskSettingsSet `
    -MultipleInstances IgnoreNew `
    -StartWhenAvailable `
    -ExecutionTimeLimit (New-TimeSpan -Days 365)

if ($PSCmdlet.ShouldProcess($TaskName, "Register scheduled task")) {
    Register-ScheduledTask `
        -TaskName $TaskName `
        -Action $action `
        -Trigger $trigger `
        -Settings $settings `
        -Description "Run EEW Quake Notify System when the current user logs on." `
        -Force | Out-Null

    Write-Host "Registered scheduled task: $TaskName"
    Write-Host "Executable: $($resolvedExe.Path)"
    Write-Host "Config: $($resolvedConfig.Path)"
}
