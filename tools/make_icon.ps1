# Generates the agent icon (ICO with an embedded 256x256 PNG).
# No external dependencies: uses System.Drawing from Windows PowerShell.

Add-Type -AssemblyName System.Drawing

$outDir = Join-Path $PSScriptRoot '..\agent\src-tauri\icons'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$icoPath = Join-Path $outDir 'icon.ico'
$pngPath = Join-Path $env:TEMP 'macrokey-icon.png'

$size = 256
$bmp = New-Object System.Drawing.Bitmap $size, $size
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

function RoundedRect([int]$x, [int]$y, [int]$w, [int]$h, [int]$r) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

$body = RoundedRect 24 24 208 208 46
$bodyBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 20, 22, 28))
$g.FillPath($bodyBrush, $body)
$border = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 48, 54, 66), 3)
$g.DrawPath($border, $body)

# knob
$accent = [System.Drawing.Color]::FromArgb(255, 125, 211, 252)
$accentBrush = New-Object System.Drawing.SolidBrush $accent
$g.FillEllipse($accentBrush, 98, 62, 60, 60)
$innerBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 20, 22, 28))
$g.FillEllipse($innerBrush, 118, 82, 20, 20)
$notch = New-Object System.Drawing.Pen ([System.Drawing.Color]::FromArgb(255, 20, 22, 28), 5)
$notch.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$notch.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$g.DrawLine($notch, 128, 74, 128, 92)

# three keys
$keyBrush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 226, 231, 240))
foreach ($x in 58, 106, 154) {
    $key = RoundedRect $x 158 44 34 10
    $g.FillPath($keyBrush, $key)
    $key.Dispose()
}

$g.Dispose()
$bmp.Save($pngPath, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()

$png = [System.IO.File]::ReadAllBytes($pngPath)
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter $fs

# ICONDIR
$bw.Write([uint16]0)
$bw.Write([uint16]1)
$bw.Write([uint16]1)
# ICONDIRENTRY
$bw.Write([byte]0)    # width 256
$bw.Write([byte]0)    # height 256
$bw.Write([byte]0)
$bw.Write([byte]0)
$bw.Write([uint16]1)
$bw.Write([uint16]32)
$bw.Write([uint32]$png.Length)
$bw.Write([uint32]22)
$bw.Write($png)
$bw.Close()

Write-Output "wrote $icoPath ($((Get-Item $icoPath).Length) bytes)"
