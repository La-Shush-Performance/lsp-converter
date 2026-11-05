# Fits the game-scale PI (car/physics/ai.rs PI_A, PI_B) to the stock
# ratings in pifit_stock.tsv: ln(evo) = PI_A + PI_B ln(ours), least squares.
# Usage: powershell -File tools/pifit.ps1
$rows = Get-Content "$PSScriptRoot\pifit_stock.tsv" | ? { $_ -and $_ -notmatch '^(#|id\t)' } | % {
  $f = $_ -split "`t"; [pscustomobject]@{ id = $f[0]; ours = [double]$f[1]; evo = [double]$f[2] }
}
$n = $rows.Count
$x = $rows | % { [math]::Log($_.ours) }; $y = $rows | % { [math]::Log($_.evo) }
$mx = ($x | Measure-Object -Average).Average; $my = ($y | Measure-Object -Average).Average
$sxy = 0; $sxx = 0
for ($i = 0; $i -lt $n; $i++) { $sxy += ($x[$i] - $mx) * ($y[$i] - $my); $sxx += ($x[$i] - $mx) * ($x[$i] - $mx) }
$b = $sxy / $sxx; $a = $my - $b * $mx
$se = 0
foreach ($r in $rows) {
  $q = [math]::Exp($a + $b * [math]::Log($r.ours)); $se += ($q - $r.evo) * ($q - $r.evo)
  "{0,-32} ours {1,5:N1}  evo {2,5:N1}  fit {3,5:N1}  off {4,5:N1}" -f $r.id, $r.ours, $r.evo, $q, ($q - $r.evo)
}
"cars $n  PI_A $a  PI_B $b  rms {0:N2}" -f [math]::Sqrt($se / $n)

