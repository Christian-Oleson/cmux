# cmux-rpc.ps1 — minimal PowerShell client for the cmux JSON-RPC API.
#
# Usage:
#   . .\scripts\cmux-rpc.ps1
#   $c = Connect-CmuxRpc
#   Invoke-CmuxRpc $c "session.create" @{ name = "bot" }
#   Invoke-CmuxRpc $c "surface.send_text" @{ pane_id = 0; text = "echo hello`r`n" }
#   Start-Sleep -Milliseconds 500
#   Invoke-CmuxRpc $c "surface.read_output" @{ pane_id = 0 }
#   Disconnect-CmuxRpc $c

function Connect-CmuxRpc {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream(
        '.',            # server
        'cmux-rpc',     # pipe name (no \\.\pipe\ prefix for this API)
        [System.IO.Pipes.PipeDirection]::InOut,
        [System.IO.Pipes.PipeOptions]::None
    )
    $pipe.Connect(5000)
    $state = [pscustomobject]@{
        Pipe  = $pipe
        NextId = 1
    }
    return $state
}

function Disconnect-CmuxRpc {
    param($State)
    if ($State -and $State.Pipe) {
        $State.Pipe.Dispose()
    }
}

function Invoke-CmuxRpc {
    param(
        [Parameter(Mandatory)] $State,
        [Parameter(Mandatory)] [string]$Method,
                               [hashtable]$Params = @{}
    )

    $req = @{
        jsonrpc = "2.0"
        method  = $Method
        params  = $Params
        id      = $State.NextId
    }
    $State.NextId++

    $json = $req | ConvertTo-Json -Compress -Depth 10
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
    $len = [BitConverter]::GetBytes([uint32]$bytes.Length)

    $State.Pipe.Write($len, 0, 4)
    $State.Pipe.Write($bytes, 0, $bytes.Length)
    $State.Pipe.Flush()

    # Read 4-byte little-endian length prefix
    $lenBuf = New-Object byte[] 4
    $read = 0
    while ($read -lt 4) {
        $n = $State.Pipe.Read($lenBuf, $read, 4 - $read)
        if ($n -eq 0) { throw "pipe closed while reading length" }
        $read += $n
    }
    $respLen = [BitConverter]::ToUInt32($lenBuf, 0)

    # Read the JSON payload
    $respBuf = New-Object byte[] $respLen
    $read = 0
    while ($read -lt $respLen) {
        $n = $State.Pipe.Read($respBuf, $read, $respLen - $read)
        if ($n -eq 0) { throw "pipe closed while reading body" }
        $read += $n
    }

    $respJson = [System.Text.Encoding]::UTF8.GetString($respBuf)
    return ($respJson | ConvertFrom-Json)
}

function Read-CmuxOutput {
    param(
        [Parameter(Mandatory)] $State,
        [Parameter(Mandatory)] [int]$PaneId
    )
    $resp = Invoke-CmuxRpc $State "surface.read_output" @{ pane_id = $PaneId }
    if ($resp.error) {
        Write-Error $resp.error.message
        return
    }
    $resp.result.lines | ForEach-Object { Write-Host $_ }
}
