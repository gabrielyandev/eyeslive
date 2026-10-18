# EyesLive — Remote Desktop System

[![Download Windows Executable](https://img.shields.io/badge/Download%20.exe-Windows%20(64--bit)-brightgreen?style=for-the-badge&logo=windows)](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe)
[![GitHub Releases](https://img.shields.io/badge/GitHub-Releases-blue?style=for-the-badge&logo=github)](https://github.com/gabrielyandev/eyeslive/releases)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org)

> **🚀 Não precisa instalar compilador nem abrir terminal!**  
> Baixe diretamente o executável portátil **[`eyeslive.exe`](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe)** pronto para uso.  
> Dê dois cliques para abrir diretamente a interface gráfica — **nenhuma janela preta de terminal é aberta por trás** (`windows_subsystem = "windows"` nativo).

---

## ⬇️ Download do Executável (.exe)

| Plataforma | Arquivo Executável | Instruções |
| :--- | :--- | :--- |
| **Windows 10 / 11 (64-bit)** | [**`eyeslive.exe` (Download Direto)**](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe) | Baixe e dê 2 cliques. Execução pura em janela gráfica sem console. |
| **Histórico de Releases** | [**Página de Releases do Repositório**](https://github.com/gabrielyandev/eyeslive/releases) | Todas as versões e notas de lançamento. |

---

## 🔄 Notificação e Atualização Automática Integrada

O **EyesLive** conta com verificador e instalador de atualizações integrado:
1. **Verificação Automática ao Iniciar**: O aplicativo consulta o repositório GitHub silenciosamente em segundo plano.
2. **Aviso de Nova Versão**: Se houver qualquer nova versão no repositório, surge uma barra destacada no topo da interface informando a versão disponível.
3. **Atualização em 1 Clique**: Clique em `⚡ Baixar e Atualizar Automaticamente (.exe)` para que o EyesLive faça o download da nova versão e reinicie já atualizado.
4. **Verificação Manual**: Botão `🔍 Checar Atualizações no Repositório` disponível no Painel Principal e na barra inferior de status.

---

## Visao Geral

O software opera de maneira hibrida, funcionando como Host (servidor de captura e transmissao) e Viewer (cliente com suporte a multiplas sessoes em abas concorrentes) dentro do mesmo binario. A comunicacao pode ser estabelecida diretamente via conexao ponto a ponto (LAN, VPN, Tailscale) ou por meio de um servidor Relay proprio baseado em identificadores numericos de nove digitos (Desk ID).

---

## Caracteristicas Principais

- **Operacao Sem Autenticacao Centralizada**: Conexao direta via endereco IP e porta ou por meio de identificadores unicos (Desk ID), sem requisitos de cadastro, login em nuvem ou licenciamento comercial restritivo.
- **Sessoes Continuas e Ilimitadas**: Ausencia de desconexoes programadas, cronometros de sessao ou intervencoes automaticas por tempo de inatividade.
- **Interface Multi-Abas Concorrente**: Capacidade de conectar e controlar multiplas estacoes remotas simultaneamente a partir de abas independentes na mesma janela de aplicacao.
- **Transmissao de Video de Baixa Latencia (Zero Bufferbloat)**: Pipeline em tempo real com descarte automatico de quadros obsoletos, envio exclusivo do quadro mais recente, deteccao de tela estatica (dirty hash check) e downscaling dinamico (50%, 75%, 100%).
- **TCP_NODELAY no Host e Relay**: Desativacao do algoritmo de Nagle em toda a cadeia de conexao para eliminar delays de ate 200ms em movimentacao de cursor e digitacao.
- **Perfis de Desempenho e Medicao de Ping**: Indicador visual de latencia RTT (ms) na barra de controle e alternancia rapida em 1 clique (⚡ Modo Rapido, ⚖ Equilibrado, 🎨 Alta Resolucao).
- **Interacao Completa de Entrada**: Injecao de eventos de teclado e mouse a 60 Hz com resolucao de coordenadas normalizadas e suporte ao modo Somente Leitura (View-Only).
- **Sincronizacao de Area de Transferencia**: Troca bidirecional de dados de texto entre a maquina local e o host remoto.
- **Transferencia de Arquivos em Blocos**: Envio de arquivos fracionados em pacotes gerenciados com confirmacao de recebimento (ACK).
- **Fallback Automatico de Portas**: O motor Host aloca dinamicamente a proxima porta disponivel caso a porta padrao (9090) esteja em uso.

---

## Arquitetura do Codigo

A organizacao modular do codigo-fonte esta distribuida da seguinte forma:

| Arquivo | Funcao Principal |
| :--- | :--- |
| `Cargo.toml` | Manifesto do projeto com dependencias nativas (`eframe`, `tokio`, `screenshots`, `enigo`, `arboard`, `bincode`). |
| `src/main.rs` | Ponto de entrada da aplicacao e interface grafica baseada em `egui`/`eframe`, responsavel pelo gerenciamento de abas e painel de controle. |
| `src/host.rs` | Motor de captura de tela, codificacao JPEG, processamento de pacotes recebidos e injecao de eventos locais no sistema hospedeiro. |
| `src/viewer.rs` | Cliente de visualizacao assincrono, decodificacao de quadros de imagem, controle de estado de sessao e envio de acoes do usuario. |
| `src/protocol.rs` | Definicao das estruturas de pacotes e do protocolo binario serializado via `bincode` sobre transporte TCP assincrono. |
| `src/relay.rs` | Servidor de retransmissao independente e leve para conectar clientes e hosts em redes distintas utilizando Desk IDs. |

---

## Protocolo de Comunicacao

O EyesLive utiliza um protocolo binario sob medida serializado com `bincode` sobre streams assincronas gerenciadas pelo `tokio`:

1. **Handshake e Autenticacao Local**: No inicio da conexao, o cliente envia um pacote `Hello`. Caso um PIN local tenha sido configurado no Host, a validacao e efetuada neste estagio.
2. **Negociacao de Sessao**: O Host responde com `Welcome`, informando as dimensoes do monitor remoto e o estado da permissao de acesso.
3. **Fluxo de Video**: O Host transmite quadros compactados (`VideoFrame`) contendo largura, altura, carimbo de data/hora e bytes JPEG compactados.
4. **Comandos de Controle**: Eventos de movimentacao de cursor (`MouseMove`), cliques (`MouseClick`), rolagem (`MouseScroll`) e digitacao (`KeyInput`) sao enviados pelo Viewer com coordenadas relativas normalizadas (0.0 a 1.0).
5. **Transferencia de Dados**: Transmissao de dados de clipboard (`ClipboardSync`) e fluxo fracionado de arquivos (`FileStart`, `FileChunk`, `FileEnd`, `FileAck`).

---

## Requisitos de Sistema

- **Compilador Rust**: Versao 1.80 ou superior (Edicao 2021).
- **Ambiente de Build**: Cargo e gerenciador de pacotes padrao do Rust.
- **Sistemas Suportados**:
  - Windows 10 / 11 (x86_64)
  - Linux (X11 / Wayland via dependencias nativas do `enigo` e `arboard`)
  - macOS (suporte a compilacao nativa)

---

## Instrucoes de Compilacao e Execucao

### 1. Compilar e Iniciar em Modo de Producao

```powershell
# Compilacao com otimizacoes completas
cargo build --release

# Executar a aplicacao completa (Interface Grafica + Host + Viewer)
cargo run --release
```

### 2. Executar como Servidor Relay Dedicado

Caso deseje disponibilizar um no de retransmissao proprio para conectar estacoes atraves de IDs numericos:

```powershell
cargo run --release -- --relay 0.0.0.0:9091
```

### 3. Execucao em Modo de Desenvolvimento

```powershell
cargo run
```

---

## Portas de Rede Padrao

- **Host TCP**: `9090` (com selecao automatica entre `9090` e `9110` caso a porta principal esteja ocupada).
- **Relay TCP**: `9091` (configuravel via parametro `--relay <HOST:PORTA>`).

---

## Creditos e Autoria

- **Autor e Desenvolvedor**: Gabriel Yan ([@gabrielyandev](https://github.com/gabrielyandev))
- **Repositorio Oficial**: [https://github.com/gabrielyandev/eyeslive](https://github.com/gabrielyandev/eyeslive)
- **Perfil do Autor**: [https://github.com/gabrielyandev](https://github.com/gabrielyandev)
