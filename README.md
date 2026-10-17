# 👁 EyesLive — Remote Desktop

> **Criado e Assinado por [Gabriel Yan (`@gabrielyandev`)](https://github.com/gabrielyandev)**  
> 🌐 **GitHub do Autor:** [https://github.com/gabrielyandev](https://github.com/gabrielyandev)  
> 📦 **Repositório Oficial:** [https://github.com/gabrielyandev/eyeslive](https://github.com/gabrielyandev/eyeslive)

Um sistema próprio e independente de acesso remoto e compartilhamento de tela em tempo real em **Rust**, desenvolvido do zero por **Gabriel Yan (`@gabrielyandev`)**.

---

## ✨ Recursos Principais
- **🔓 Sem Login Obrigatório:** Conecte-se diretamente via `IP:Porta` (LAN/VPN/Tailscale) ou via **EyesLive ID (`XXX-XXX-XXX`)** sem necessidade de criar conta.
- **♾ Sem Limite de Tempo:** Sessões contínuas sem desconexões por tempo ou bloqueios comerciais.
- **🖥 Múltiplas Sessões Simultâneas (Multi-Abas):** Acesse e controle várias máquinas ao mesmo tempo em abas independentes dentro da mesma janela.
- **🎮 Controle Completo + Transferência de Arquivos + Clipboard:** Captura de tela em tempo real, injeção de mouse/teclado, sincronização de área de transferência e envio de arquivos.
- **🔗 Créditos & Links Diretos para o GitHub:** Botões clicáveis integrados na interface nativa que levam diretamente ao perfil [@gabrielyandev](https://github.com/gabrielyandev) e ao repositório do projeto.

---

## 📂 Estrutura do Projeto

- [`Cargo.toml`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/Cargo.toml) — Configuração do crate `eyeslive` e dependências nativas.
- [`src/main.rs`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/src/main.rs) — Interface gráfica nativa multi-abas (`eframe`/`egui`) com créditos e links clicáveis para o GitHub de **Gabriel Yan (`@gabrielyandev`)**.
- [`src/protocol.rs`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/src/protocol.rs) — Protocolo binário TCP assíncrono (`bincode` + `tokio`) para vídeo, mouse/teclado, clipboard e arquivos.
- [`src/host.rs`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/src/host.rs) — Motor Host que aceita múltiplos clientes simultâneos na porta `9090` (com fallback automático), captura a tela em JPEG e injeta eventos.
- [`src/viewer.rs`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/src/viewer.rs) — Cliente multi-sessão concorrente.
- [`src/relay.rs`](file:///c:/Users/gabrielyandev/Documents/projects/anydesk/src/relay.rs) — Servidor Relay próprio opcional para roteamento por ID (`--relay`).

---

## 🚀 Como Compilar e Executar

```powershell
# Compilar e iniciar o EyesLive (Host + Viewer Multi-Abas):
cargo run --release

# (Opcional) Iniciar um Servidor Relay próprio por ID:
cargo run --release -- --relay 0.0.0.0:9091
```

---

## 👨‍💻 Autor & Créditos

- **Autor:** Gabriel Yan ([@gabrielyandev](https://github.com/gabrielyandev))
- **GitHub:** [https://github.com/gabrielyandev](https://github.com/gabrielyandev)
