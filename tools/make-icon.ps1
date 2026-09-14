<#
.SYNOPSIS
    Renders the AeternaVault icon (vault door with an infinity handle) to PNG
    and multi-resolution ICO files.

.DESCRIPTION
    Uses System.Drawing, which ships with Windows PowerShell 5.1, so no extra
    tools are required. Small sizes use a simplified drawing so the mark stays
    legible at 16 px.

    Output:
      assets/icon/aeterna-vault.ico      (16..256 px, PNG-compressed entries)
      assets/icon/aeterna-vault-256.png  (window icon, embedded by the app)
      assets/icon/aeterna-vault-512.png  (README / social preview)

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File tools/make-icon.ps1
#>

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$outDir = Join-Path $PSScriptRoot '..\assets\icon'
New-Item -ItemType Directory -Force $outDir | Out-Null

function New-Color([string]$hex) {
    return [System.Drawing.ColorTranslator]::FromHtml($hex)
}

$bg     = New-Color '#1C1F24'
$border = New-Color '#2E3239'
$gold   = New-Color '#C9A227'
$goldHi = New-Color '#E0B93A'

function New-RoundedRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function New-Pen([System.Drawing.Color]$color, [float]$width) {
    $pen = New-Object System.Drawing.Pen($color, $width)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
    return $pen
}

function Render-Icon([int]$size) {
    $s = [float]$size
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    $detailed = $size -ge 48

    # Vault body: a calm, dark rounded square.
    $m = [Math]::Max(0.5, $s * 0.035)
    $body = New-RoundedRect $m $m ($s - 2 * $m) ($s - 2 * $m) ($s * 0.18)
    $g.FillPath((New-Object System.Drawing.SolidBrush($bg)), $body)
    if ($detailed) {
        $g.DrawPath((New-Pen $border ([Math]::Max(1.0, $s * 0.012))), $body)
    }

    $cx = $s * 0.5
    $cy = $s * 0.5

    # Hinges on the left edge.
    if ($size -ge 32) {
        $hb = New-Object System.Drawing.SolidBrush($gold)
        $hw = $s * 0.045
        $hh = $s * 0.13
        $hx = $s * 0.075
        foreach ($hy in @(($s * 0.27), ($s * 0.60))) {
            $hinge = New-RoundedRect $hx $hy $hw $hh ($hw * 0.45)
            $g.FillPath($hb, $hinge)
        }
    }

    # Door ring.
    $r1 = $s * 0.33
    $w1 = [Math]::Max(1.4, $s * 0.034)
    $g.DrawEllipse((New-Pen $gold $w1), ($cx - $r1), ($cy - $r1), (2 * $r1), (2 * $r1))

    if ($detailed) {
        # Inner ring and dial ticks.
        $r2 = $s * 0.245
        $g.DrawEllipse((New-Pen $gold ([Math]::Max(1.0, $s * 0.014))), ($cx - $r2), ($cy - $r2), (2 * $r2), (2 * $r2))
        $tickPen = New-Pen $gold ([Math]::Max(1.0, $s * 0.016))
        for ($i = 0; $i -lt 12; $i++) {
            $a = $i * [Math]::PI / 6
            $ra = $r2 + $s * 0.022
            $rb = $r1 - $s * 0.030
            $g.DrawLine($tickPen,
                [float]($cx + [Math]::Cos($a) * $ra), [float]($cy + [Math]::Sin($a) * $ra),
                [float]($cx + [Math]::Cos($a) * $rb), [float]($cy + [Math]::Sin($a) * $rb))
        }
    }

    # Handle: a lemniscate (infinity sign).
    $a = if ($detailed) { $s * 0.165 } else { $s * 0.21 }
    $w = if ($detailed) { [Math]::Max(1.5, $s * 0.036) } else { [Math]::Max(1.3, $s * 0.075) }
    $pts = New-Object 'System.Collections.Generic.List[System.Drawing.PointF]'
    $n = 96
    for ($i = 0; $i -lt $n; $i++) {
        $t = 2 * [Math]::PI * $i / $n
        $den = 1 + [Math]::Pow([Math]::Sin($t), 2)
        $x = $a * [Math]::Cos($t) / $den
        $y = $a * [Math]::Sin($t) * [Math]::Cos($t) / $den
        $pts.Add((New-Object System.Drawing.PointF([float]($cx + $x), [float]($cy + $y))))
    }
    $g.DrawPolygon((New-Pen $goldHi $w), $pts.ToArray())

    $g.Dispose()
    return $bmp
}

function Get-PngBytes([System.Drawing.Bitmap]$bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    return , $ms.ToArray()
}

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$images = @()
foreach ($size in $sizes) {
    $bmp = Render-Icon $size
    $images += , (Get-PngBytes $bmp)
    $bmp.Dispose()
}

# ICO container with PNG-compressed entries (supported since Windows Vista).
$icoPath = Join-Path $outDir 'aeterna-vault.ico'
$fs = [System.IO.File]::Create($icoPath)
$bw = New-Object System.IO.BinaryWriter($fs)
$bw.Write([UInt16]0)
$bw.Write([UInt16]1)
$bw.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $dim = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $bw.Write([Byte]$dim)
    $bw.Write([Byte]$dim)
    $bw.Write([Byte]0)
    $bw.Write([Byte]0)
    $bw.Write([UInt16]1)
    $bw.Write([UInt16]32)
    $bw.Write([UInt32]$images[$i].Length)
    $bw.Write([UInt32]$offset)
    $offset += $images[$i].Length
}
foreach ($img in $images) { $bw.Write($img) }
$bw.Flush()
$fs.Dispose()

foreach ($size in @(256, 512)) {
    $bmp = Render-Icon $size
    $bmp.Save((Join-Path $outDir "aeterna-vault-$size.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
}

Write-Host "Icon written to $((Resolve-Path $outDir).Path)"
