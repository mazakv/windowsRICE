# Draws the windowsRICE application icon (the bar's menu mark: four
# rounded squares on a dark rounded tile) and writes assets/windowsrice.ico
# with PNG images at the sizes Windows uses. Run it again after changing
# the design; the build embeds the file through apps/panel/build.rs.
param([string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) 'assets\windowsrice.ico'))
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null

function Round-Rect([System.Drawing.Graphics]$g, [System.Drawing.Brush]$b, [float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = [math]::Max(0.5, $r * 2)
    $p.AddArc($x, $y, $d, $d, 180, 90); $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90); $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure(); $g.FillPath($b, $p); $p.Dispose()
}

$sizes = 16, 20, 24, 32, 40, 48, 64, 96, 128, 256
$images = foreach ($s in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap($s, $s, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'; $g.PixelOffsetMode = 'HighQuality'; $g.Clear([System.Drawing.Color]::Transparent)
    $tile = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0x15, 0x17, 0x1C))
    Round-Rect $g $tile 0 0 $s $s ($s * 0.22)
    # Four squares, the top-right one in the warm status colour, like the bar.
    $blue = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0x5B, 0xA4, 0xE6))
    $warm = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 0xF0, 0xB0, 0x60))
    $pad = $s * 0.22; $gap = $s * 0.08; $cell = ($s - 2 * $pad - $gap) / 2
    foreach ($i in 0, 1) { foreach ($j in 0, 1) {
        $b = if ($i -eq 1 -and $j -eq 0) { $warm } else { $blue }
        Round-Rect $g $b ($pad + $i * ($cell + $gap)) ($pad + $j * ($cell + $gap)) $cell $cell ($cell * 0.22)
    } }
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
    , $ms.ToArray()
}

# ICO container: header, directory entries, then the PNG images.
$fs = [System.IO.File]::Create($Out); $w = New-Object System.IO.BinaryWriter($fs)
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($k = 0; $k -lt $sizes.Count; $k++) {
    $s = $sizes[$k]; $len = $images[$k].Length
    $w.Write([byte]($(if ($s -ge 256) { 0 } else { $s }))); $w.Write([byte]($(if ($s -ge 256) { 0 } else { $s })))
    $w.Write([byte]0); $w.Write([byte]0); $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$len); $w.Write([uint32]$offset); $offset += $len
}
foreach ($img in $images) { $w.Write($img) }
$w.Close()
"wrote $Out ($((Get-Item $Out).Length) bytes, $($sizes.Count) sizes)"
