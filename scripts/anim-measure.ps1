param([string]$Root)
Set-Location $Root
$out = Join-Path $Root 'target\phase14\anim'
$plain = Join-Path $out 'plain.toml'; $anim = Join-Path $out 'anim.toml'
"schema_version = 3`n" | Set-Content $plain -Encoding utf8
@'
schema_version = 3

[bar.style]

[[bar.style.animations]]
target = "text_gradient"
duration_ms = 8000
interpolation = "linear"
keyframes = [
  { at = 0.0, color = "#FF5A5A" },
  { at = 0.33, color = "#69DB7C" },
  { at = 0.66, color = "#4DABF7" },
]
'@ | Set-Content $anim -Encoding utf8
foreach ($c in @(@{ n = 'without animation'; f = $plain }, @{ n = 'rainbow text'; f = $anim })) {
  $stats = Join-Path $out ("stats-" + [IO.Path]::GetFileNameWithoutExtension($c.f) + ".json")
  $p = Start-Process 'target\release\windowsrice-panel.exe' -ArgumentList @('--config', $c.f, '--stats', $stats, '--no-hotkey', '--exit-after', '40', '--title', '"Anim test"', '--force-motion') -PassThru
  Start-Sleep 10; $p.Refresh(); $c0 = $p.TotalProcessorTime.TotalMilliseconds; $t0 = Get-Date
  Start-Sleep 20; $p.Refresh(); $c1 = $p.TotalProcessorTime.TotalMilliseconds; $el = ((Get-Date) - $t0).TotalMilliseconds
  $mem = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
  $null = $p.WaitForExit(30000)
  $j = Get-Content $stats -Raw | ConvertFrom-Json
  $draw = @($j.frame_draw_ms | Sort-Object); $med = if ($draw.Count) { $draw[[int]($draw.Count/2)] } else { 0 }
  '{0,-20} CPU {1:N2}% of one core, frames {2}, draw median {3:N2} ms, private {4} MiB, log: {5}' -f $c.n, (($c1 - $c0) / $el * 100), $j.frames.Count, $med, $mem, (($j.popup_log | Where-Object { $_ -match 'animation' }) -join '; ')
}
