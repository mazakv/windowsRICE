# A controlled backdrop for screenshots of the windowsRICE bar.
#
# The bar can be translucent, so whatever is behind it shows through. Every
# script that captures the bar first opens this window across the top of
# the screen, waits until it exists (title "windowsRICE test backdrop"),
# and only then captures, so no private desktop content is recorded.
# Kind: black, white, colour or checker. Closes itself after -Seconds.
param([string]$Kind = 'colour', [int]$Seconds = 20, [int]$Height = 220)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -Namespace WBW -Name Dpi -MemberDefinition '[DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);'
[void][WBW.Dpi]::SetProcessDpiAwarenessContext([IntPtr](-4))
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$f = New-Object System.Windows.Forms.Form
$f.Text = 'windowsRICE test backdrop'
$f.FormBorderStyle = 'None'; $f.StartPosition = 'Manual'; $f.ShowInTaskbar = $false; $f.TopMost = $true
$f.Bounds = New-Object System.Drawing.Rectangle($screen.X, $screen.Y, $screen.Width, $Height)
$f.Add_Paint({ param($s, $e)
    $g = $e.Graphics; $r = $f.ClientRectangle
    switch ($Kind) {
        'black' { $g.Clear([System.Drawing.Color]::Black) }
        'white' { $g.Clear([System.Drawing.Color]::White) }
        'checker' {
            $g.Clear([System.Drawing.Color]::White)
            for ($x = 0; $x -lt $r.Width; $x += 40) { for ($y = 0; $y -lt $r.Height; $y += 40) { if ((($x + $y) / 40) % 2 -eq 0) { $g.FillRectangle([System.Drawing.Brushes]::Black, $x, $y, 40, 40) } } }
        }
        default {
            $b = New-Object System.Drawing.Drawing2D.LinearGradientBrush($r, [System.Drawing.Color]::FromArgb(255, 0, 170), [System.Drawing.Color]::FromArgb(0, 200, 255), 0.0)
            $blend = New-Object System.Drawing.Drawing2D.ColorBlend(4)
            $blend.Colors = @([System.Drawing.Color]::FromArgb(255, 0, 170), [System.Drawing.Color]::FromArgb(255, 220, 0), [System.Drawing.Color]::FromArgb(0, 200, 120), [System.Drawing.Color]::FromArgb(0, 120, 255))
            $blend.Positions = @(0.0, 0.33, 0.66, 1.0); $b.InterpolationColors = $blend
            $g.FillRectangle($b, $r)
        }
    }
})
# The shell moves windows out of the strip the bar reserves; keep this one
# at the top edge (moving it does not change its stacking order).
$pin = New-Object System.Windows.Forms.Timer; $pin.Interval = 100; $pin.Add_Tick({ if ($f.Top -ne $screen.Y) { $f.Top = $screen.Y } }); $pin.Start()
$t = New-Object System.Windows.Forms.Timer; $t.Interval = $Seconds * 1000; $t.Add_Tick({ $f.Close() }); $t.Start()
[System.Windows.Forms.Application]::Run($f)
