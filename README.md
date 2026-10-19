# EyesLive Remote Desktop

[![Download Windows (.exe)](https://img.shields.io/badge/Download-Windows%2064--bit%20(.exe)-2563eb?style=for-the-badge&logo=windows)](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe)
[![Releases](https://img.shields.io/badge/GitHub-Releases-0f172a?style=for-the-badge&logo=github)](https://github.com/gabrielyandev/eyeslive/releases)
[![Licença](https://img.shields.io/badge/Plataforma-Windows%2010%20%2F%2011-10b981?style=for-the-badge)](https://github.com/gabrielyandev/eyeslive)

**EyesLive** é um software nativo de acesso remoto de alta performance para Windows. Permite conectar e controlar computadores remotamente por **EyesLive ID** (através da internet) ou **endereço IP direto** (rede local / VPN), sem necessidade de cadastro, sem login obrigatório e sem limite de tempo de sessão.

---

## Download e Instalação

| Versão | Download | Descrição |
| :--- | :--- | :--- |
| **Windows 10 / 11 (x64)** | [**Baixar `eyeslive.exe`**](https://github.com/gabrielyandev/eyeslive/releases/latest/download/eyeslive.exe) | Executável único: pode ser usado em **Modo Portátil** (sem instalação) ou instalado no sistema em 1 clique. |
| **Todas as Versões** | [**Página de Releases**](https://github.com/gabrielyandev/eyeslive/releases) | Histórico de versões, notas de atualização e binários oficiais. |

---

## Principais Recursos

- **Conexão Simples por ID ou IP**: Conecte-se informando apenas o **EyesLive ID** de 9 dígitos (ex: `462-152-868`) para acesso externo ou o endereço **IP:Porta** para conexões diretas em rede local/VPN.
- **Zero Login e Tempo Ilimitado**: Sem criação de contas, sem assinaturas e sem interrupções ou limites de duração nas sessões.
- **Múltiplas Sessões Simultâneas**: Controle vários computadores ao mesmo tempo através de abas independentes dentro da mesma janela.
- **Autorização na Tela e PIN de Segurança**:
  - Ao receber uma tentativa de conexão, o computador exibe um alerta permitindo **Acesso Completo (Tela + Controle)**, **Somente Visualização** ou **Recusar Conexão**.
  - Suporte a **PIN de Segurança** configurável para acesso não supervisionado.
- **Bandeja do Sistema (System Tray) e Segundo Plano**:
  - Opção de instalação em 1 clique com criação automática de atalhos na Área de Trabalho e no Menu Iniciar.
  - Permanece ativo discretamente ao lado do relógio do Windows e pode iniciar automaticamente junto com o sistema.
- **Transmissão Fluida e Transferência de Arquivos**:
  - Perfis rápidos de qualidade (**Modo Rápido**, **Equilibrado** e **Alta Resolução**) com ajuste em tempo real de FPS, compressão e escala.
  - Sincronização de área de transferência (Clipboard) e envio direto de arquivos entre máquinas.
- **Temas Escuro Suave e Claro**: Interface de alto contraste com alternância instantânea entre **Modo Escuro Suave** e **Modo Claro**.
- **Atualizador Automático Integrado**: Verifica, baixa e aplica novas versões diretamente pela interface do programa.

---

## Como Usar

### 1. Receber Acesso neste Computador (Host)
1. Abra o **EyesLive**.
2. Compartilhe seu **EyesLive ID** (ou seu endereço IP local) exibido no painel **Este Dispositivo**.
3. Quando o parceiro solicitar conexão, escolha na janela de confirmação se deseja liberar o controle completo ou apenas a visualização da tela.
4. *(Opcional)* Defina um **PIN de Segurança** caso queira acessar seu próprio computador remotamente sem precisar confirmar na tela.

### 2. Acessar Outro Computador (Cliente)
1. No painel **Nova Conexão**, digite o **EyesLive ID** ou o **IP:Porta** do computador que deseja acessar.
2. Informe o PIN caso o computador remoto tenha configurado um.
3. Clique em **Conectar e Iniciar Sessão Remota**.

---

## Parâmetros de Linha de Comando (Opcional)

O executável suporta parâmetros adicionais para automação:

```powershell
# Instalar silenciosamente no perfil do usuário e iniciar na Bandeja do Sistema
eyeslive.exe --install --background

# Executar um servidor Relay privado próprio (para redes corporativas isoladas)
eyeslive.exe --relay 0.0.0.0:9091
```

---

## Compilando a partir do Código-Fonte

Caso deseje compilar o projeto manualmente em seu ambiente:

```powershell
git clone https://github.com/gabrielyandev/eyeslive.git
cd eyeslive
cargo build --release
```

O executável otimizado será gerado em `target\release\eyeslive.exe`.

---

## Autor

Desenvolvido por **Gabriel Yan** ([@gabrielyandev](https://github.com/gabrielyandev))  
Repositório oficial: [github.com/gabrielyandev/eyeslive](https://github.com/gabrielyandev/eyeslive)
