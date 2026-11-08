# Notificações do sistema (app desktop)

Feito em 2026-11-07. Código: `app/src/notif.rs` (decisão, pura e testada), `app/src/notif_os.rs` (fala com o sistema),
ligações em `app/src/main.rs` (`Session::notify_message`, `notify_new_friend_requests`, `notif_tick`, `os_title`).
Não depende do servidor: o app usa só o que já recebe (`S2C::Message`, `Snapshot`).

## Quando notifica

Só com a janela **fora de foco** ou minimizada. Nunca: com as notificações desligadas, em "Não perturbe", mensagem do próprio
usuário, de bloqueado, de sistema (alguém entrou / fixou), do histórico (`S2C::History` nem passa por aqui; só `S2C::Message` ao vivo,
e um id já visto no canal não repete), nem mensagem vazia.

| Evento | Notifica (padrão "só DMs, menções e amizade") | Com o filtro desligado |
|---|---|---|
| DM | sim | sim |
| `<@você>`, `@everyone`, `@here`, resposta a uma mensagem sua | sim | sim |
| Mensagem comum de canal | não | sim |
| Pedido de amizade recebido (novo no `Snapshot`; o primeiro snapshot só marca o que já existia) | sim | sim |

Anti-spam (`notif::Limiter`): 1 por canal a cada 3 s (as intermediárias ficam retidas e a **última** sai quando o intervalo
acaba; o app pede o redesenho no momento certo) e 10 por minuto no total (a 11ª cai). O mesmo canal **substitui** a notificação anterior
(`replaces_id` no D-Bus; etiqueta + grupo no toast).

## Texto

- Título: DM = autor; canal = `autor em #canal · servidor` (corta em 90 caracteres). Amizade: `Pedido de amizade` / `<nome> quer ser seu amigo`.
- Corpo: Markdown tirado com o parser do `telacast-md`; spoiler vira `spoiler oculto`; `<@id>` vira `@nome`; `<#id>` vira `#canal`;
  anexo vira `📎 nome`; quebras de linha viram espaço; no máximo 180 **caracteres** (não bytes), com `…` no fim se cortou.
- No Linux o corpo vai com `& < >` escapados quando o servidor de notificação declara `body-markup` (senão uma mensagem com `<b>` ou
  `<a href>` viraria formatação/link na notificação).
- Ícone: `app/assets/icon.png`, gravado em `~/.cache/telacast/icon.png` (Windows: `%LOCALAPPDATA%\telacast\`) na primeira notificação.

## Clique

Traz a janela para frente (`Minimized(false)` + `Focus`) e abre, pela navegação por histórico que já existe (`Session::open`, então as
setas ← → funcionam): a DM, o canal ou Início → aba **Pendente**. Se o canal sumiu do snapshot, só traz a janela.

## Contador no título

Fora de foco: `(N) Telacast` com N = menções não lidas + DMs não lidas sem menção (`99+` acima de 99); `(•) Telacast` quando só há
mensagens comuns de servidor não lidas; `Telacast` ao focar. A barra de título própria mostra o mesmo texto (no lugar do nome do
canal) enquanto o contador existe. O título do sistema (`ViewportCommand::Title`) só é reenviado quando muda.

Consequência no não lido: com a janela fora de foco, uma mensagem no **canal aberto** não é mais "lida" na hora (antes o app dava `Ack`
sempre que o canal estava aberto). Ela conta como não lida/menção e o `Ack` sai quando a janela volta ao foco.

## Configuração

Menu do usuário → **Minha conta** → NOTIFICAÇÕES (a janela agora tem rolagem, porque a lista de sessões a deixava maior que a tela):
"Notificações do sistema" (`notif_on`), "Só mensagens diretas, menções e pedidos de amizade" (`notif_only`) e o botão
"Enviar notificação de teste" (ignora foco e anti-spam; serve para conferir o sistema do usuário). Guardado no `storage` do eframe como
`voice_ns` e as outras.

## Por que `zbus` direto (e `windows` direto no Windows)

| Opção | Custo | Veredito |
|---|---|---|
| `notify-rust` (Linux: zbus; Windows: tauri-winrt-notification) | crate novo + dependências próprias; no Windows traz outra versão do `windows` | mais código sem ganho: usa o mesmo zbus |
| `notify-send` (processo) | zero dependência | sem clique, sem substituir, um processo por aviso, depende de `libnotify-bin` |
| **`zbus` direto** | **já está no binário** (`accesskit_unix` do eframe); só `futures-lite` e `async-channel`, que também já estavam no `Cargo.lock` | escolhido: ~0,9 MB de código, nenhuma biblioteca nova no disco |
| **`windows` direto (toast WinRT)** | o crate já é compilado (wgpu, accesskit); só liga `UI_Notifications`, `Data_Xml_Dom`, `Foundation`, `Win32_System_Registry` | escolhido |

Medido (release, 2026-11-07, mesma máquina e árvore, antes = binário de 21:25 sem a funcionalidade):

| | antes | depois | delta |
|---|---|---|---|
| binário Linux | 43 079 584 B | 44 118 648 B | +1,04 MB (+2,4%): ~0,11 MB de lógica/UI, ~0,93 MB do backend D-Bus |
| `Telacast.exe` (zigbuild) | 35 182 080 B | 35 734 528 B | +0,55 MB (+1,6%) |
| PSS, 75 s depois de abrir, Xvfb/lavapipe, mesma conta | 80,1 MB | 80,4 MB | +0,3 MB |
| PSS depois da 1ª notificação (thread `notif` + executor do zbus) | 80,5 MB | 80,8 MB | +0,4 MB, +2 threads |

A thread só nasce na primeira notificação; quem desliga as notificações não paga nada. Os valores absolutos de PSS são do lavapipe
(Xvfb) e **não** representam o app na tela real (89 MB, ver `docs/MEMORIA.md`): vale só o delta.

## Linux: detalhes e limites

- Fala `org.freedesktop.Notifications` (GNOME, KDE Plasma, dunst, mako, xfce4-notifyd...). Ação `default` = clique no corpo; o
  `Notify` leva `["default", "Abrir"]`. Servidores que não suportam ações ignoram, e o clique não faz nada (as notificações seguem
  funcionando).
- Uma thread só (`notif`) é dona da conexão: manda `Notify`, escuta `ActionInvoked` e `NotificationClosed`. Sem D-Bus de sessão ou sem
  servidor de notificação: uma linha em stderr e nova tentativa só depois de 1 minuto; o app nunca cai.
- GNOME agrupa por `.desktop` (hint `desktop-entry`); o app ainda não instala um `.desktop`, então lá o nome/ícone vêm de `app_name` e
  `app_icon`. Conferir no PC do usuário.
- Sem som (o roadmap ainda pede "notificação + som": fica para depois; o servidor de notificação do usuário pode tocar o som dele).

## Windows: o que foi feito e o que não foi testado

Toast WinRT (`ToastNotificationManager::CreateToastNotifierWithId("Telacast")`). Como o app não é empacotado, o app grava
`HKCU\Software\Classes\AppUserModelId\Telacast` (`DisplayName`, `IconUri`) na primeira notificação, sem administrador. O clique chega
no evento `Activated`, que o Windows dispara numa thread dele; ela só manda o alvo por um canal e acorda a interface.
**Compila** com `cargo zigbuild --target x86_64-pc-windows-gnu --release -p telacast`, mas **nunca rodou num Windows de verdade**
(sem Windows para testar; Wine não implementa `Windows.UI.Notifications`). Falha de toast: log em stderr e para de tentar naquela sessão.
Pontos a conferir num Windows: o toast aparece com nome/ícone "Telacast" (se aparecer como "Windows PowerShell" ou não aparecer, o
registro do AUMID não pegou e o caminho é um atalho do menu Iniciar com o AUMID, no `.wxs`), o clique traz a janela, "Focus assist"
respeitado.

## Como foi verificado

Testes de `notif.rs` (23): foco, minimizada, DND, desligado, bloqueado, autor próprio, histórico, mensagem de sistema, menção
(`<@id>`, `<@!id>`, `@everyone`, `@here`, resposta), DM, amizade, Markdown, spoiler, menções para nomes, anexos, truncamento UTF-8,
escape de marcação, anti-spam (3 s por canal com "a última vence", 10 por minuto), dedupe de id, título com contador.
Ponta a ponta no Linux: Xvfb `:99` isolado + barramento D-Bus **privado** (`dbus-daemon` do snap `core26`, sem `dbus-run-session` na
máquina) + servidor de notificação de teste em Python (GIO) que registra cada `Notify` e emite `ActionInvoked` a pedido; contas
`teste16`/`beto16` no servidor de teste local.
