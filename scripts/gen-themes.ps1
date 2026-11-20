# Generates the Phase 8 built-in themes from seed colours.
#
# Each theme is derived from a background, an accent and a text colour per
# mode, using the same structure as the hand-written built-in themes. The
# theme validator (cargo test -p windowsrice-theme) then checks every
# contrast pair; seeds were chosen so that all checks pass.
param([string]$Dir = (Join-Path (Split-Path $PSScriptRoot -Parent) 'crates\windowsrice-theme\themes'))
$ErrorActionPreference = 'Stop'

function Parse([string]$h) { $h = $h.TrimStart('#'); @([Convert]::ToInt32($h.Substring(0, 2), 16), [Convert]::ToInt32($h.Substring(2, 2), 16), [Convert]::ToInt32($h.Substring(4, 2), 16)) }
function Hex($c) { '#{0:X2}{1:X2}{2:X2}' -f [int]$c[0], [int]$c[1], [int]$c[2] }
# Colour `a` at fraction `f` over colour `b`.
function Mix([string]$a, [double]$f, [string]$b) { $x = Parse $a; $y = Parse $b; Hex @(0..2 | ForEach-Object { [math]::Round($x[$_] * $f + $y[$_] * (1 - $f)) }) }
function Lum([string]$h) { $c = Parse $h | ForEach-Object { $v = $_ / 255.0; if ($v -le 0.03928) { $v / 12.92 } else { [math]::Pow(($v + 0.055) / 1.055, 2.4) } }; 0.2126 * $c[0] + 0.7152 * $c[1] + 0.0722 * $c[2] }
function Ratio([string]$a, [string]$b) { $l1 = Lum $a; $l2 = Lum $b; ([math]::Max($l1, $l2) + 0.05) / ([math]::Min($l1, $l2) + 0.05) }

function Palette([string]$mode, [string]$bg, [string]$accent, [string]$text) {
    $dark = $mode -eq 'dark'
    $base = Mix $text 0.035 $bg; $raised = Mix $text 0.075 $bg; $overlay = Mix $text 0.11 $bg
    $onAccent = if ((Ratio '#FFFFFF' $accent) -ge (Ratio '#101114' $accent)) { '#FFFFFF' } else { '#101114' }
    $sem = if ($dark) { @('#7FC79A', '#E2B868', '#EC8C8C', '#8DBEE6') } else { @('#2E7D4F', '#8A5D00', '#B23A3A', '#2F6E96') }
    $hover = if ($dark) { Mix '#FFFFFF' 0.14 $accent } else { Mix '#000000' 0.12 $accent }
    $pressed = if ($dark) { Mix "#000000" 0.06 $accent } else { Mix '#000000' 0.22 $accent }
    [ordered]@{
        background = $bg; surface_base = $base; surface_raised = $raised; surface_overlay = $overlay
        border_subtle = $(if ($dark) { '#FFFFFF14' } else { '#00000014' }); border_control = $(if ($dark) { '#FFFFFF66' } else { '#00000075' })
        text_primary = $text; text_secondary = (Mix $text 0.80 $overlay); text_muted = (Mix $text 0.73 $overlay); text_disabled = (Mix $text 0.38 $bg)
        text_on_accent = $onAccent; accent = $accent; accent_hover = $hover; accent_pressed = $pressed
        selection = (Mix $accent 0.26 $bg); focus_ring = $accent
        success = $sem[0]; warning = $sem[1]; danger = $sem[2]; info = $sem[3]
        shadow = $(if ($dark) { '#00000040' } else { '#00000018' })
    }
}

$themes = @(
    @{ id = 'midnight'; name = 'windowsRICE Midnight'; desc = 'Deep navy surfaces with a cool violet accent.'; tags = 'dark'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 34; radius = 6
       dark = @('#0B1020', '#8C7CF0', '#E6E8F5') },
    @{ id = 'soft-gray'; name = 'windowsRICE Soft Gray'; desc = 'Muted gray tones with gentle contrast for long sessions.'; tags = 'dark", "light'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 34; radius = 8
       dark = @('#2A2C30', '#9FB0C4', '#E4E6EA'); light = @('#E3E5E8', '#4D5D73', '#202226') },
    @{ id = 'blue-accent'; name = 'windowsRICE Blue Accent'; desc = 'Neutral surfaces with a vivid blue accent.'; tags = 'dark", "light'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 34; radius = 6
       dark = @('#16181D', '#3D8BFF', '#ECEEF2'); light = @('#F3F5F9', '#0A5BD3', '#14171C') },
    @{ id = 'monochrome'; name = 'windowsRICE Monochrome'; desc = 'Black, white and grays only.'; tags = 'dark", "light'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 32; radius = 2
       dark = @('#111111', '#D0D0D0', '#F2F2F2'); light = @('#F4F4F4', '#2B2B2B', '#111111') },
    @{ id = 'ocean'; name = 'windowsRICE Ocean'; desc = 'Deep sea blue with an aqua accent, slightly see-through.'; tags = 'dark", "translucent'; default = 'dark'; backdrop = 'acrylic'; opacity = 0.86; height = 36; radius = 8
       dark = @('#0A2233', '#36D1C4', '#E3F4F8') },
    @{ id = 'neon'; name = 'windowsRICE Neon'; desc = 'Near-black surfaces with an electric magenta accent.'; tags = 'dark'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 34; radius = 4
       dark = @('#0A0A12', '#FF4FD8', '#F2F0FF') },
    @{ id = 'pastel'; name = 'windowsRICE Pastel'; desc = 'Soft lilac paper tones with a calm purple accent.'; tags = 'light'; default = 'light'; backdrop = 'none'; opacity = 1.0; height = 36; radius = 12
       light = @('#F4EEF8', '#7A4FB3', '#2A2233') },
    @{ id = 'warm'; name = 'windowsRICE Warm'; desc = 'Espresso and cream with an amber accent.'; tags = 'dark", "light'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 34; radius = 8
       dark = @('#1E1712', '#E8A04C', '#F3E8DC'); light = @('#F6EEE4', '#9A5A12', '#2B2119') },
    @{ id = 'high-contrast'; name = 'windowsRICE High Contrast'; desc = 'Maximum contrast: pure black or white with a strong yellow or blue accent.'; tags = 'dark", "light", "accessible'; default = 'dark'; backdrop = 'none'; opacity = 1.0; height = 36; radius = 2
       dark = @('#000000', '#FFE500', '#FFFFFF'); light = @('#FFFFFF', '#0037A8', '#000000') }
)

foreach ($t in $themes) {
    $sb = New-Object System.Text.StringBuilder
    [void]$sb.AppendLine("# $($t.name) — generated by scripts/gen-themes.ps1 from seed colours.")
    [void]$sb.AppendLine()
    [void]$sb.AppendLine('[meta]')
    [void]$sb.AppendLine('schema_version = 1')
    [void]$sb.AppendLine("id = `"windowsrice-$($t.id)`"")
    [void]$sb.AppendLine("name = `"$($t.name)`"")
    [void]$sb.AppendLine("description = `"$($t.desc)`"")
    [void]$sb.AppendLine('author = "windowsRICE"')
    [void]$sb.AppendLine('version = "1.0.0"')
    [void]$sb.AppendLine("default_mode = `"$($t.default)`"")
    [void]$sb.AppendLine('accent_source = "theme"')
    [void]$sb.AppendLine("tags = [`"$($t.tags)`"]")
    foreach ($mode in 'dark', 'light') {
        if (-not $t.ContainsKey($mode)) { continue }
        $s = $t[$mode]; $p = Palette $mode $s[0] $s[1] $s[2]
        [void]$sb.AppendLine(); [void]$sb.AppendLine("[palette.$mode]")
        foreach ($k in $p.Keys) { [void]$sb.AppendLine("$k = `"$($p[$k])`"") }
    }
    $mono = if ($t.id -eq 'monochrome') { 'Cascadia Code' } else { 'Cascadia Code' }
    [void]$sb.AppendLine(@"

[typography]
family = "Segoe UI Variable"
fallback = ["Malgun Gothic"]
mono_family = "$mono"
mono_fallback = ["Consolas", "Malgun Gothic"]
weight_regular = 400
weight_emphasis = 500
weight_strong = 500
line_height = 1.4

[typography.sizes]
caption = 11
body = 13
body_large = 14
subtitle = 15
title = 18
display = 24

[spacing]
unit = 4
xs = 4
sm = 8
md = 12
lg = 16
xl = 24
xxl = 32

[radius]
control = $($t.radius)
menu = $([math]::Max(6, $t.radius))
card = $([math]::Max(8, $t.radius))
panel = 0
dock = 12
window = $([math]::Max(4, $t.radius))

[shadow]
offset_y = 2
blur = 8

[materials]
backdrop = "$($t.backdrop)"
surface_opacity = $($t.opacity.ToString([Globalization.CultureInfo]::InvariantCulture))

[motion]
level = "full"
duration_fast_ms = 100
duration_normal_ms = 160
duration_slow_ms = 240
easing = [0.2, 0.0, 0.0, 1.0]

[components.bar]
position = "top"
height = $($t.height)
floating = false
margin = 0
padding_x = 12

[components.dock]
position = "bottom"
icon_size = 40
spacing = 4
padding = 6
auto_hide = "always"
magnification = false
magnification_scale = 1.0

[components.launcher]
width = 600
max_results = 7
top_offset_percent = 30

[components.widget]
padding = 12
gap = 8

[components.window]
border_width = 1
corner = "round_small"
accent_border_on_focus = false
"@)
    $path = Join-Path $Dir "windowsrice-$($t.id).toml"
    [IO.File]::WriteAllText($path, $sb.ToString().Replace("`r`n", "`n"), [Text.UTF8Encoding]::new($false))
    "wrote $path"
}
