param([string]$Log, [int]$Seconds)
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -Namespace WRU -Name Win -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindowW(string cls, string title);'
$end = (Get-Date).AddSeconds($Seconds)
while ((Get-Date) -lt $end) {
    $h = [WRU.Win]::FindWindowW('windowsRICE.Panel', [NullString]::Value)
    if ($h -ne [IntPtr]::Zero) {
        $sw = [Diagnostics.Stopwatch]::StartNew()
        $r = [System.Windows.Automation.AutomationElement]::FromHandle($h)
        $all = $r.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        $names = foreach ($e in $all) { $e.Current.Name; $e.Current.BoundingRectangle }
        $sw.Stop(); Add-Content $Log ("{0} {1}" -f $all.Count, $sw.Elapsed.TotalMilliseconds)
    }
    Start-Sleep -Seconds 5
}
