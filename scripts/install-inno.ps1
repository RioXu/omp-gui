param([string]$Destination = (Join-Path ([IO.Path]::GetTempPath()) 'omp-gui-build-tools/inno-6.5.4'))
$ErrorActionPreference = 'Stop'
$compiler = Join-Path $Destination 'ISCC.exe'
if (-not (Test-Path -LiteralPath $compiler)) {
    $setup = Join-Path ([IO.Path]::GetTempPath()) "omp-inno-$([guid]::NewGuid().ToString('N')).exe"
    Invoke-WebRequest -Uri 'https://github.com/jrsoftware/issrc/releases/download/is-6_5_4/innosetup-6.5.4.exe' -OutFile $setup
    $expected = 'FA73BF47A4DA250D185D07561C2BFDA387E5E20DB77E4570004CF6A133CC10B1'
    if ((Get-FileHash -LiteralPath $setup -Algorithm SHA256).Hash -ne $expected) { throw 'Inno Setup checksum mismatch' }
    if ((Get-AuthenticodeSignature -LiteralPath $setup).Status -ne 'Valid') { throw 'Invalid Inno Setup signature' }
    $process = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/CURRENTUSER', '/PORTABLE=1', "/DIR=`"$Destination`"") -WindowStyle Hidden -Wait -PassThru
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $compiler)) { throw "Inno Setup installation failed: $($process.ExitCode)" }
}
Write-Output $compiler
