Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  EyesLive - Compilando Executavel Windows " -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan

cargo build --release

if ($LASTEXITCODE -eq 0) {
    $exePath = "target\release\eyeslive.exe"
    if (Test-Path $exePath) {
        $fileObj = Get-Item $exePath
        $sizeMb = [math]::Round($fileObj.Length / 1048576, 2)
        Write-Host ""
        Write-Host "[SUCESSO] Executavel compilado com sucesso!" -ForegroundColor Green
        Write-Host "Arquivo: $exePath - Tamanho: $sizeMb MB" -ForegroundColor Yellow
        Write-Host "Janela de Terminal Desativada: Sim (subsystem: windows)" -ForegroundColor Green
        Write-Host "Voce pode distribuir este arquivo .exe diretamente ou publica-lo no GitHub Releases." -ForegroundColor Cyan
    }
} else {
    Write-Host ""
    Write-Host "[ERRO] Falha durante a compilacao." -ForegroundColor Red
}
