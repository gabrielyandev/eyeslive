# EyesLive — Remote Desktop System

[![Download Windows Executable](https://img.shields.io/badge/Download%20.exe-Windows%20(64--bit)-brightgreen?style=for-the-badge&logo=windows)](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe)
[![GitHub Releases](https://img.shields.io/badge/GitHub-Releases-blue?style=for-the-badge&logo=github)](https://github.com/gabrielyandev/eyeslive/releases)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org)

> **Execucao Direta Sem Terminal + Modo Segundo Plano**  
> Baixe diretamente o executavel **[`eyeslive.exe`](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe)** pronto para uso.  
> Execute diretamente em interface grafica nativa (`windows_subsystem = "windows"`) ou instale no sistema em 1 clique para iniciar automaticamente e permanecer ativo em segundo plano.

---

## Download do Executavel (.exe)

| Plataforma | Arquivo Executavel | Instrucoes |
| :--- | :--- | :--- |
| **Windows 10 / 11 (64-bit)** | [**`eyeslive.exe` (Download Direto)**](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe) | Baixe e execute diretamente em janela grafica nativa sem console, ou clique em `INSTALAR EYESLIVE E ATIVAR SEGUNDO PLANO` no painel. |
| **Historico de Releases** | [**Pagina de Releases do Repositorio**](https://github.com/gabrielyandev/eyeslive/releases) | Todas as versoes e notas de lancamento. |

---

## Tela de Autorizacao de Acesso Remoto (Consent Modal)

Quando outro dispositivo solicita conexao a esta maquina (Host), o **EyesLive** exibe automaticamente uma **Janela Modal de Autorizacao de Seguranca** (restaurando a janela caso esteja minimizada em segundo plano) com:
- **Identificacao do Solicitante** e **Endereco IP de Origem**.
- **Contagem Regressiva de Seguranca (60 segundos)**: se nenhuma acao for tomada, a conexao e recusada automaticamente.
- **Tres Niveis de Decisao Imediata**:
  1. `PERMITIR ACESSO COMPLETO (TELA + MOUSE E TECLADO)`: libera transmissao de video e injecao de entrada.
  2. `SOMENTE VISUALIZACAO`: permite que o cliente veja a tela, mas bloqueia no Host qualquer tentativa de controlar mouse ou teclado.
  3. `RECUSAR CONEXAO`: rejeita o acesso imediatamente.
- Opcao configuravel no Painel Principal para ativar ou desativar a solicitacao na tela (para casos de acesso nao supervisionado protegido por PIN).

---

## Instalacao no Sistema e Execucao em Segundo Plano

O **EyesLive** possui instalador nativo integrado (`src/installer.rs`):
1. **Instalacao em 1 Clique**: O botao `INSTALAR EYESLIVE E ATIVAR SEGUNDO PLANO` copia o binario para `%LOCALAPPDATA%\EyesLive\eyeslive.exe` e cria atalhos na **Area de Trabalho** e no **Menu Iniciar** sem exigir privilegios de administrador.
2. **Inicializacao com o Windows (`--background`)**: Registra automaticamente o aplicativo em `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` para iniciar minimizado em segundo plano junto com o sistema.
3. **Permanencia em Segundo Plano ao Fechar (`X`)**: Ao clicar no botao `X` da janela (ou em `MINIMIZAR PARA SEGUNDO PLANO AGORA`), o aplicativo nao encerra o servidor Host — ele permanece em segundo plano aguardando conexoes e salta automaticamente para a frente quando alguem solicita acesso. Para fechar completamente o processo, basta clicar em `ENCERRAR PROCESSO` na barra inferior.

---

## Notificacao e Atualizacao Automatica Integrada

1. **Verificacao Automatica ao Iniciar**: Consulta o repositorio GitHub silenciosamente em segundo plano.
2. **Aviso de Nova Versao**: Exibe uma barra destacada no topo da interface informando a versao disponivel.
3. **Atualizacao em 1 Clique**: Clique em `BAIXAR E ATUALIZAR AUTOMATICAMENTE (.EXE)` para baixar a nova versao e reiniciar atualizado.
4. **Verificacao Manual**: Botao `VERIFICAR ATUALIZACOES NO REPOSITORIO` disponivel no Painel de Controle e na barra inferior de status.

---

## Caracteristicas Principais

- **Operacao Sem Autenticacao Centralizada**: Conexao direta via endereco IP e porta ou por meio de identificadores unicos (Desk ID), sem requisitos de cadastro ou login em nuvem.
- **Sessoes Continuas e Ilimitadas**: Ausencia de desconexoes programadas ou cronometros de sessao.
- **Interface Multi-Abas Concorrente**: Conecte e controle multiplas estacoes remotas simultaneamente em abas independentes na mesma janela.
- **Interface Dark Moderna Sem Emojis**: Tema escuro customizado em tons Slate/Zinc com indicadores geometricos de status, tipografia tecnica e atalhos diretos para o GitHub do autor.
- **Transmissao de Video de Baixa Latencia (Zero Bufferbloat)**: Pipeline em tempo real com descarte automatico de quadros obsoletos, envio exclusivo do quadro mais recente, deteccao de tela estatica (dirty hash check) e downscaling dinamico (50%, 75%, 100%).
- **TCP_NODELAY no Host e Relay**: Desativacao do algoritmo de Nagle em toda a cadeia de conexao para eliminar delays em movimentacao de cursor e digitacao.
- **Perfis de Desempenho e Medicao de Ping**: Indicador visual de latencia RTT (ms) na barra de controle e alternancia rapida em 1 clique (`MODO RAPIDO`, `EQUILIBRADO`, `ALTA RESOLUCAO`).
- **Sincronizacao de Area de Transferencia e Transferencia de Arquivos**: Troca bidirecional de texto e envio de arquivos fracionados em blocos com confirmacao de recebimento (ACK).

---

## Arquitetura do Codigo

| Arquivo | Funcao Principal |
| :--- | :--- |
| `Cargo.toml` | Manifesto do projeto com dependencias nativas (`eframe`, `tokio`, `screenshots`, `enigo`, `arboard`, `bincode`, `ureq`). |
| `src/main.rs` | Ponto de entrada da aplicacao, interface grafica moderna em `egui`/`eframe`, modal de autorizacao de acesso e gerenciamento de segundo plano. |
| `src/host.rs` | Motor de captura de tela, fila de autorizacao de acesso (`PendingConsent`), codificacao JPEG e injecao de eventos locais. |
| `src/installer.rs` | Instalador nativo em `%LOCALAPPDATA%\EyesLive`, criacao de atalhos no Windows e registro de inicializacao automatica em segundo plano. |
| `src/viewer.rs` | Cliente de visualizacao assincrono multi-sessao, decodificacao de quadros de imagem e envio de acoes do usuario. |
| `src/protocol.rs` | Definicao das estruturas de pacotes e do protocolo binario serializado via `bincode` sobre transporte TCP assincrono. |
| `src/relay.rs` | Servidor de retransmissao independente para conectar clientes e hosts utilizando Desk IDs. |
| `src/updater.rs` | Verificador e instalador automatico de novas releases diretamente do GitHub. |

---

## Instrucoes de Compilacao e Execucao

### 1. Compilar e Iniciar em Modo de Producao

```powershell
# Compilacao com otimizacoes completas
cargo build --release

# Executar a aplicacao completa (Interface Grafica + Host + Viewer)
cargo run --release
```

### 2. Instalar e Executar em Segundo Plano via Linha de Comando

```powershell
# Instalar no sistema e iniciar em segundo plano:
cargo run --release -- --install --background
```

### 3. Executar como Servidor Relay Dedicado

```powershell
cargo run --release -- --relay 0.0.0.0:9091
```

---

## Creditos e Autoria

- **Autor e Desenvolvedor**: Gabriel Yan ([@gabrielyandev](https://github.com/gabrielyandev))
- **Repositorio Oficial**: [https://github.com/gabrielyandev/eyeslive](https://github.com/gabrielyandev/eyeslive)
- **Perfil do Autor**: [https://github.com/gabrielyandev](https://github.com/gabrielyandev)
