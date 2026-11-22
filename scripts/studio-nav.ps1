# Design Studio navigation for the verification scripts (Phase 12): the
# sidebar shows six groups ("group:<key>"); the open group lists its pages
# ("nav:<page>") when it has more than one. Dot-source this file, then call
# Open-StudioPage with a function that finds an element by automation id.
$StudioGroupOf = @{
    styles = 'templates'; themes = 'templates'
    modules = 'bar-layout'; layout = 'bar-layout'
    colors = 'appearance'; background = 'appearance'; typography = 'appearance'; icons = 'appearance'
    effects = 'motion'; motion = 'motion'; media = 'motion'
    presets = 'my-designs'; import = 'my-designs'
    advanced = 'advanced'
}
$StudioPages = @('styles', 'themes', 'modules', 'layout', 'colors', 'background', 'typography', 'icons', 'effects', 'motion', 'media', 'presets', 'import', 'advanced')

function Select-StudioElement($e) {
    $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 450
}

# Opens a page by key; $find is a scriptblock taking an automation id.
# Returns the element that shows the page as selected (the page item, or
# the group item for a group with one page).
function Open-StudioPage([string]$page, [scriptblock]$find) {
    $g = $StudioGroupOf[$page]
    if (-not $g) { throw "unknown Studio page $page" }
    $ge = & $find "group:$g"
    if (-not $ge) { throw "no element group:$g" }
    Select-StudioElement $ge
    $pe = & $find "nav:$page"
    if ($pe) { Select-StudioElement $pe; return (& $find "nav:$page") }
    return (& $find "group:$g")
}

function Test-StudioPageSelected([string]$page, [scriptblock]$find) {
    $e = & $find "nav:$page"
    if (-not $e) { $e = & $find "group:$($StudioGroupOf[$page])" }
    $e -and $e.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Current.IsSelected
}
