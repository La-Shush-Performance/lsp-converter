# Builds icon.ico from La Shush Performance's main shield: the carbon crest
# with its yellow rim, the same picture as the garage door (web/img/crest.png,
# the brand pack's crest-yellow-640, unchanged). Every size is the crest,
# taskbar and title bar too: nothing is redrawn, only scaled. (Until 4 Nov 2026
# the icon was the flat Inverse badge with the monogram below 24 px; the crest
# reads better at icon sizes than the brand's gold master, whose rim goes
# muddy on a dark taskbar.)
#   powershell -ExecutionPolicy Bypass -File make-icon.ps1 [-Out icon.ico] [-Preview out.png]
param([string]$Out = (Join-Path $PSScriptRoot 'icon.ico'), [string]$Preview)

Add-Type -AssemblyName System.Drawing
$ErrorActionPreference = 'Stop'
$crestFile = Join-Path $PSScriptRoot '..\..\web\img\crest.png'
$sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256

function New-Canvas([int]$w, [int]$h) {
    $b = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($b)
    $g.InterpolationMode = 'HighQualityBicubic'
    $g.PixelOffsetMode = 'HighQuality'
    $g.CompositingQuality = 'HighQuality'
    $g.SmoothingMode = 'AntiAlias'
    $g.Clear([System.Drawing.Color]::Transparent)
    return $b, $g
}

$crest = New-Object System.Drawing.Bitmap $crestFile
$crest = $crest.Clone((New-Object System.Drawing.Rectangle 0, 0, $crest.Width, $crest.Height), [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)

# halve in steps before the last resize: one big jump in GDI+ aliases. The
# crest is a little taller than wide: it fills the height, centred.
function Render([int]$px) {
    $src = $crest
    while ($src.Height -gt 2 * $px) {
        $c, $g = New-Canvas ([int]($src.Width / 2)) ([int]($src.Height / 2))
        $g.DrawImage($src, 0, 0, $c.Width, $c.Height); $g.Dispose()
        $src = $c
    }
    $c, $g = New-Canvas $px $px
    $h = $px * 0.98
    $w = $h * $crest.Width / $crest.Height
    $g.DrawImage($src, [single](($px - $w) / 2), [single](($px - $h) / 2), [single]$w, [single]$h); $g.Dispose()
    $c
}

function Png-Bytes($bmp) {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    , $ms.ToArray()
}

# icon.ico: one PNG-compressed image per size
$imgs = foreach ($s in $sizes) { , (Png-Bytes (Render $s)) }
$ico = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter $ico
$w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $d = if ($sizes[$i] -ge 256) { 0 } else { $sizes[$i] }
    $w.Write([byte]$d); $w.Write([byte]$d); $w.Write([byte]0); $w.Write([byte]0)
    $w.Write([uint16]1); $w.Write([uint16]32)
    $w.Write([uint32]$imgs[$i].Length); $w.Write([uint32]$offset)
    $offset += $imgs[$i].Length
}
foreach ($b in $imgs) { $w.Write([byte[]]$b) }
$w.Flush()
[IO.File]::WriteAllBytes($Out, $ico.ToArray())
Write-Host "$Out : $($sizes -join ', ') px"
# and the crest on its own, for the installer's window (.NET's Icon class
# can't read the PNG-compressed images in icon.ico)
$badgePng = Join-Path (Split-Path $Out) 'badge.png'
(Render 256).Save($badgePng, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Host "$badgePng : 256 px"

# optional preview sheet: every size at actual pixels on the grounds an icon
# meets (Windows dark, Shed Black, Windows light, Ivory), then the small ones 6x
if ($Preview) {
    $grounds = '#202020', '#0A0A0A', '#F3F3F3', '#FAF6EC'
    $sheet = New-Object System.Drawing.Bitmap 1000, (4 * 280 + 170)
    $g = [System.Drawing.Graphics]::FromImage($sheet)
    $g.InterpolationMode = 'NearestNeighbor'
    $g.PixelOffsetMode = 'Half'
    for ($r = 0; $r -lt 4; $r++) {
        $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml($grounds[$r]))), 0, $r * 280, 1000, 280)
        $x = 16
        foreach ($s in $sizes) { $g.DrawImage((Render $s), $x, $r * 280 + 270 - $s - 8, $s, $s); $x += $s + 14 }
    }
    $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.ColorTranslator]::FromHtml('#202020'))), 0, 1120, 1000, 170)
    $x = 16
    foreach ($s in 16, 20, 24) { $g.DrawImage((Render $s), $x, 1130, $s * 6, $s * 6); $x += $s * 6 + 24 }
    $g.Dispose()
    $sheet.Save($Preview, [System.Drawing.Imaging.ImageFormat]::Png)
    Write-Host "preview: $Preview"
}
