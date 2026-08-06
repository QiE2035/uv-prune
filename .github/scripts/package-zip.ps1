param(
    [Parameter(Mandatory = $true)]
    [string]$Source,

    [Parameter(Mandatory = $true)]
    [string]$Destination
)

# Zip a single binary with the classic PKZIP layout, called from the
# publish workflow's bash step. A separate script file keeps the invocation
# free of bash -> powershell.exe quoting traps that plagued inline
# `powershell -Command` strings in the workflow YAML.
Compress-Archive -Path $Source -DestinationPath $Destination -CompressionLevel Optimal