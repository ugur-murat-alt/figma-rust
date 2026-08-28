param(
    [Parameter(Mandatory = $true)][int]$DpiPercent,
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$OutputDirectory
)

$ErrorActionPreference = 'Stop'
if ($DpiPercent -notin @(100, 125, 150)) { throw 'DPI must be 100, 125, or 150' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class NativeWindow {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr dpiContext);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int index);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool GetClientRect(IntPtr hwnd, out RECT rect);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool ClientToScreen(IntPtr hwnd, ref POINT point);
  [DllImport("user32.dll")] [return: MarshalAs(UnmanagedType.Bool)]
  public static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wParam, IntPtr lParam);
}
'@

function Wait-ForJsonLine([string]$Path, [int]$LineIndex) {
    for ($attempt = 0; $attempt -lt 300; $attempt++) {
        if ((Test-Path $Path) -and @((Get-Content $Path)).Count -gt $LineIndex) {
            $lines = @((Get-Content $Path))
            try {
                return ($lines[$LineIndex] | ConvertFrom-Json)
            } catch {
                # Redirected stdout can be observed before the complete JSON line is flushed.
            }
        }
        Start-Sleep -Milliseconds 100
    }
    throw "Timed out waiting for JSON line $($LineIndex + 1) in $Path"
}

function Capture-Probe([int]$Run) {
    $stdout = Join-Path $OutputDirectory "run-$Run.stdout.jsonl"
    $stderr = Join-Path $OutputDirectory "run-$Run.stderr"
    $png = Join-Path $OutputDirectory "run-$Run.png"
    $process = Start-Process -FilePath $Executable `
        -ArgumentList @('--dpi-profile', "$DpiPercent") -PassThru `
        -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $previousDpiContext = [IntPtr]::Zero
    try {
        $ready = Wait-ForJsonLine $stdout 0
        if ($ready.event -ne 'ready' -or $ready.pid -ne $process.Id -or
            $ready.dpi_percent -ne $DpiPercent -or
            $ready.logical_width -ne 160 -or $ready.logical_height -ne 80 -or
            $ready.window_title -ne 'figma-rust-windows-probe-ready') {
            throw 'Readiness record does not match the launched probe'
        }
        $process.Refresh()
        $hwnd = $process.MainWindowHandle
        if ($hwnd -eq [IntPtr]::Zero) { throw 'Probe has no main window handle' }
        $previousDpiContext = [NativeWindow]::SetThreadDpiAwarenessContext([IntPtr](-4))
        if ($previousDpiContext -eq [IntPtr]::Zero) { throw 'Could not enable Per-Monitor V2 DPI awareness' }
        $dpi = [NativeWindow]::GetDpiForWindow($hwnd)
        $expectedDpi = [uint32](96 * $DpiPercent / 100)
        if ($dpi -ne $expectedDpi) { throw "Window DPI $dpi does not match $expectedDpi" }
        $rect = New-Object NativeWindow+RECT
        if (-not [NativeWindow]::GetClientRect($hwnd, [ref]$rect)) { throw 'GetClientRect failed' }
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        $expectedWidth = [int](160 * $DpiPercent / 100)
        $expectedHeight = [int](80 * $DpiPercent / 100)
        if ($width -ne $expectedWidth -or $height -ne $expectedHeight) {
            throw "Physical client ${width}x${height} does not match ${expectedWidth}x${expectedHeight}"
        }
        $origin = New-Object NativeWindow+POINT
        if (-not [NativeWindow]::ClientToScreen($hwnd, [ref]$origin)) { throw 'ClientToScreen failed' }
        $bitmap = New-Object System.Drawing.Bitmap($width, $height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($origin.X, $origin.Y, 0, 0, $bitmap.Size)
            $bitmap.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $graphics.Dispose()
            $bitmap.Dispose()
        }
        $clickX = [int]($width / 2)
        $clickY = [int]($height / 2)
        $lParam = [IntPtr](($clickY -shl 16) -bor ($clickX -band 0xffff))
        [NativeWindow]::PostMessage($hwnd, 0x0201, [IntPtr]1, $lParam) | Out-Null
        [NativeWindow]::PostMessage($hwnd, 0x0202, [IntPtr]0, $lParam) | Out-Null
        $inputEvent = Wait-ForJsonLine $stdout 1
        if ($inputEvent.event -ne 'input' -or $inputEvent.pid -ne $process.Id) {
            throw 'Primary click was not observed by GPUI'
        }
        $process.WaitForExit(10000) | Out-Null
        return [ordered]@{ pid = $process.Id; dpi = $dpi; width = $width; height = $height; png = $png; input = $true }
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
        if ($previousDpiContext -ne [IntPtr]::Zero -and
            [NativeWindow]::SetThreadDpiAwarenessContext($previousDpiContext) -eq [IntPtr]::Zero) {
            throw 'Could not restore the PowerShell thread DPI awareness context'
        }
    }
}

$run1 = Capture-Probe 1
$run2 = Capture-Probe 2
$hash1 = (Get-FileHash $run1.png -Algorithm SHA256).Hash.ToLowerInvariant()
$hash2 = (Get-FileHash $run2.png -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash1 -ne $hash2) { throw 'Two Windows captures are not byte-identical' }
$font = "$env:WINDIR\Fonts\segoeui.ttf"
if (-not (Test-Path $font)) { throw "Segoe UI font file is missing: $font" }
$video = @(Get-CimInstance Win32_VideoController | Select-Object -ExpandProperty Name)
$displayWidth = [NativeWindow]::GetSystemMetrics(0)
$displayHeight = [NativeWindow]::GetSystemMetrics(1)
$evidence = [ordered]@{
    schema_version = 1
    platform = 'windows'
    dpi_percent = $DpiPercent
    window_dpi = $run1.dpi
    logical_size = [ordered]@{ width = 160; height = 80 }
    physical_size = [ordered]@{ width = $run1.width; height = $run1.height }
    display_size = [ordered]@{ width = $displayWidth; height = $displayHeight }
    gpui_revision = '5631830c564afa89b3aba679f45d9c3345f9460f'
    font = [ordered]@{ family = 'Segoe UI'; path = $font; sha256 = (Get-FileHash $font -Algorithm SHA256).Hash.ToLowerInvariant() }
    gpu = $video
    input_smoke = [ordered]@{ run_1 = $run1.input; run_2 = $run2.input }
    image_sha256 = $hash1
    passed = $true
}
$evidence | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'windows-platform-evidence.json')
