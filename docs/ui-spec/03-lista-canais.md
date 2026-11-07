# 03 - Painel de canais (lista de canais, cabeçalho do servidor, voz e cartão do usuário)

Legenda: **[F]** confirmado no bundle do cliente; **[I]** inferência (conhecimento do produto, maquete Grafite ou decisão minha); **[?]** não consegui confirmar.

Fontes usadas (somente leitura): `scratchpad/discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior, 12,9 MB) com conferência em `web.3f3154a164805d56.js`; strings pt-BR em `12064a7d28fc7eb1.js` (chunk de locale, chaves hash -> texto); `docs/discord/10-ux-interface.md`, `docs/ui-preview/grafite.html`, `app/src/main.rs` (`channel_panel`, linhas 578-680), `app/src/theme.rs`, `proto/src/lib.rs`, `server/migrations/0001_init.sql`.

## 0. Limite da evidência (leia antes de implementar)

O bundle tem **só tokens de cor** nos 4 `.css` (28 regras, todas `:root`/`.theme-*`); o CSS dos componentes (CSS-modules) **não está nele**, e o chunk lazy que renderiza a barra lateral (linha de canal, cabeçalho, painel de voz, cartão do usuário) **não faz parte dos 93 arquivos** (as chaves de i18n desses menus, ex. "Ocultar canais silenciados", não são referenciadas em nenhum arquivo de código). O que o bundle tem de sólido:

- o **modelo da lista de canais** (`ChannelListStore`, classes de categoria/canal com `renderLevel`) em `web.0bcf...js` por volta do offset 5 889 516: é daqui que saem as regras de recolher, esconder silenciados, ordenação e o que aparece quando;
- stores de colapso, silêncio, leitura/menções, estados de voz, atalhos de teclado;
- tabelas de tamanho de ícone e de avatar/presença;
- textos pt-BR.

Portanto: **regras e comportamento = [F]; medidas em px de linha/padding/altura = [I]** (valores típicos do cliente desktop, ajustados para casar com a maquete Grafite e com o `list_row` atual de 34 px). Onde houver número [F] eu cito o de onde veio.

---

## 1. Resumo e fora de escopo

O painel é a coluna esquerda do servidor (hoje 240 px, `Panel::left("channels")`). De cima para baixo: **cabeçalho do servidor** (nome + menu suspenso), **árvore rolável** (canais sem categoria, depois categorias recolhíveis; dentro de cada categoria os canais de texto/anúncio/fórum por posição e, abaixo deles, os de voz/palco), e uma **doca inferior** com o painel "Voz conectada" (só quando conectado) e o **cartão do usuário** (avatar com presença, nome, status, microfone, fones, configurações).

Regras centrais [F]: a visibilidade de cada canal é calculada por um "nível de renderização" (sem acesso / escondido por silenciado / escondido por categoria recolhida / visível); canal selecionado, com menção ou com o usuário conectado nunca some; canais de voz vêm sempre depois dos de texto na mesma categoria; recolher e "ocultar silenciados" são preferências **da conta**, sincronizadas com o servidor.

**Fora de escopo** (outros agentes): lista de DMs/Home (o `match &s.view` do `channel_panel` para `Home`/`Dm`), barra de servidores, lista de membros, área de chat, o motor de voz/vídeo e o pop-up de perfil (04/05), modal de configurações do usuário e de servidor (só aponto o ponto de entrada), menu de contexto de usuário.

---

## 2. Árvore de componentes (de fora para dentro)

Convenção Telacast: camadas L0-L4, `LINE` como filete, `TEXT/TEXT2/MUTED`, `AMBER` único acento, `OK` só para presença/conexão, `RED` só erro/destrutivo (ver pergunta 3). Raio de linhas 8 (igual a `list_row`), cartões 10-12. Texto da lista em `FontId::proportional(14.5)` (= `TextStyle::Body`).

```
ChannelPanel                      largura 240 (min 200, max 360 se redimensionável), fill L1, filete direito 1px LINE
├─ ServerHeader                   altura 52, padding 0 16, filete inferior 1px LINE
│   ├─ título                     Newsreader (serif) 18px/24, TEXT, 1 linha, reticências
│   ├─ chevron                    12px (xxs [F]), MUTED -> TEXT no hover; vira "X" (2 traços) com o menu aberto [I]
│   └─ (opcional P3) banner       imagem 240x135 sob/atrás do título; título sobre faixa L0 a 60% [I]
├─ ChannelTree (ScrollArea)       padding-top 8, padding-bottom 8 [F: lista de DMs usa paddingBottom 8], margens laterais 8
│   ├─ [canais sem categoria]     primeiro, sem cabeçalho [F: ordem das seções]
│   ├─ CategoryBlock (n)          ordenados por `position`
│   │   ├─ CategoryHeader         altura 24 + 8 de respiro acima (primeira categoria: 4) [I; 24 = sectionHeight da lista de DMs, F]
│   │   │   ├─ setinha            12px, desenhada; baixo = aberta, direita = recolhida
│   │   │   ├─ nome               11.5px, MAIÚSCULAS (`caps()`), MUTED -> TEXT2 no hover; peso 600 (precisa fonte semibold)
│   │   │   └─ "+" (16px)         só no hover da categoria e com permissão; tooltip "Criar canal"
│   │   └─ ChannelRow (n)         ver abaixo
│   │        └─ VoiceUserRow (n)  só em canal de voz/palco com participantes
│   └─ placeholder "vazio"        categoria vazia p/ quem pode gerenciar: linha de 24px, texto MUTED "Nenhum canal" [I]
├─ Dock                           margem 8, Frame fill L2, raio 10, sem sombra
│   ├─ VoicePanel                 (só conectado) altura ~92: ver 2.4
│   ├─ filete 1px LINE entre as partes
│   └─ UserCard                   altura 52: ver 2.5
```

### 2.1 ChannelRow (linha de canal)

| Item | Medida | Fonte |
|---|---|---|
| Altura | **34** (igual ao `list_row` atual e ao `.ch` da maquete); sem gap vertical entre linhas | [I] |
| Padding horizontal | 8 esquerda, 8 direita (dentro da margem lateral de 8 do painel) | [I] |
| Raio | 8 | existente |
| Slot do ícone | 20x20 (glifo de 16 = `xs` [F] centrado; `sm`=18 para canal de voz) | tabela de ícones [F] |
| Gap ícone-nome | 8 | [I] |
| Nome | 14.5px, 1 linha, reticências; sem negrito = peso normal, negrito = semibold (ver 6.1) | [I] |
| Selo de menção | altura 16, largura mín. 16, raio 8, texto 11px bold `AMBER_INK` sobre `AMBER`, padding 0 5; mais de 99 vira "99+" | altura/raio [I]; "99+" [F] só no cabeçalho do chat, na barra lateral [?] |
| Ícones de ação | 16px (`xs` [F]), alvo de clique 20x20, gap 4; à esquerda do selo | [I] |
| Cadeado | glifo de 8px no canto inferior direito do slot do ícone | [I] |
| Barra de não lido | retângulo 3x8, raio 2, `TEXT`, colado na borda esquerda do painel (x = 0, centro vertical da linha) | [I] |

**Ícone por tipo** [F: o cliente tem um glifo por tipo, e uma variante com cadeado para cada um: `AnnouncementsLockIcon`, `ForumLockIcon`, `StageLockIcon`, `VoiceLockIcon`; variantes de aviso/spoiler (NSFW) também existem, fora de escopo]:

| Tipo (coluna `kind`) | Glifo | Observação |
|---|---|---|
| 0 texto | `#` (duas verticais inclinadas + duas horizontais) | |
| 1 voz | alto-falante com 1-2 ondas (`sm` 18) | "VoiceNormal"/"VoiceLow" [F]: onda a menos quando o canal tem limite de usuários cheio [?] |
| 2 anúncio | megafone | |
| 3 palco | círculo com ondas / "palco" | |
| 4 categoria | sem ícone, só a setinha | |
| 5 fórum (novo, ver seção 5) | dois balões sobrepostos | |

### 2.2 VoiceUserRow (participante dentro do canal de voz)

- Altura 32, recuo esquerdo 28 a partir da borda da linha do canal (o avatar fica logo à esquerda do início do nome do canal) [I].
- Avatar **24** (`SIZE_24` [F]); sem bolinha de presença (no cliente a presença só aparece a partir de 32 em contextos de lista de membros) [I].
- Gap 8; nome 13.5px `TEXT2` (negrito/`TEXT` quando fala), 1 linha com reticências; usa apelido do servidor, depois nome de exibição [F: o aria-label usa `nick ?? nome`].
- Ícones à direita, 16px, gap 4, ordem da esquerda para a direita [I]: **AO VIVO** (pílula `AMBER`, 10px bold, texto "AO VIVO" [F: string "Ao Vivo"]) -> câmera -> microfone cortado -> fones cortados. "Cortado por você" (auto) = `MUTED`; "cortado pelo servidor" = variante "deny" [F: existem `MicrophoneDenyIcon`/`HeadphonesDenyIcon` além de `...SlashIcon`] em `RED`.
- Falando: anel de 2px `OK` ao redor do avatar [I]; o avatar do cliente recebe `voiceStatus` ("speaking" | "soundboard") e `voiceDb` [F] e há duas intensidades `HIGH=0.7` e `LOW=0.45` [F: constantes; uso como opacidade do anel é [I]].
- Hover: fundo L2; clique no participante abre o perfil (área 05); botão direito: volume/silenciar para mim (área 04).
- Quando o canal tem `user_limit`, o texto de acessibilidade é "N de M usuários" [F: `6qgTOF`]; mostrar "N/M" em `MUTED` no lado direito da linha do **canal** (não do participante) [I].

### 2.3 CategoryHeader e ícones de ação

- Setinha rotaciona: aberta = aponta para baixo, recolhida = aponta para a direita; 0.15 s `ease_out` [I]; o glifo vem do mesmo desenho de traços (ver código 6.3).
- Ações no hover do **canal** (somente com permissão; ver seção 5): **convidar** (pessoa+, tooltip "Criar convite" [F: `zJrgTG`]) e **configurar** (engrenagem, tooltip "Editar canal" [F: `3gUsJb`] / "Configurações do canal" [F: `XPDhcc`]). Canal de voz ganha também "abrir chat" (balão) [I; descartar enquanto não houver texto-na-voz]. Linha selecionada mostra as ações o tempo todo.
- Ações no hover da **categoria**: "+" com tooltip "Criar canal" [F: `fUYU+j`]; ao clicar em uma categoria o canal novo já nasce nela (o texto do menu é "Criar canal em {categoria}" [F: `VHLvn/`]).
- Botão "Criar canal" global (pedido): não existe como botão fixo no Discord, só "+" nas categorias e item "Criar canal" no menu do cabeçalho [F: itens existem]. Para o Telacast: manter o "+" em cada cabeçalho **e**, no fim da lista, quando não há categoria nenhuma e o usuário é dono, uma linha-botão "+ Criar canal" (estado vazio, seção 3).

### 2.4 VoicePanel ("Voz conectada")

Visível só enquanto há conexão de voz; fica **acima** do cartão do usuário, dentro da doca.

```
┌────────────────────────────────────┐ padding 8 10
│ [sinal 16] Voz conectada     [ ☎ ] │  linha 1 (28): sinal colorido pela latência + título 13px semibold OK; botão desconectar 28x28 à direita
│            Canal / Servidor        │  linha 2 (16): 12px MUTED, 1 linha
│ [ câmera ] [ transmitir ] [ ... ]  │  linha 3 (32): botões 1/3 da largura cada, altura 32, raio 8, L3, ícone 20
└────────────────────────────────────┘
```

- Título por estado de conexão [F: tabela em `web.0bcf...js` ~10 267 000, `getStatus`]: Conectando -> "Conectando"; Autenticando -> "Autenticando"; esperando endpoint -> "Aguardando o servidor de voz"; conectado -> **"Voz conectada"** (com vídeo: "Vídeo conectado"); WebRTC -> "Conectando CTR"; ICE -> "Verificando Rota"; DTLS -> "Conectando DTLS"; sem rota -> "Sem rota"; "CTR desconectado"; "Desconectado". Cor: conectado `OK`; conectando `AMBER` (pulso lento de opacidade, o mesmo de `dot()`); erro/sem rota/desconectado `RED`. Para o Telacast, exibir só os 4 grupos: Conectando..., Voz conectada, Reconectando..., Sem conexão.
- Latência: tooltip "Latência: N ms" (clique abre detalhes: [?]). Cor do ícone: <=80 ms `OK`, <=200 `AMBER`, acima `RED` [I].
- Botão desconectar: tooltip "Desconectar-se da voz" [F: `/lEZpt`]. Botões da linha 3: câmera ("Ativar câmera" [F]), **Transmitir tela** (hoje `Act::Publish`/`GoLive`, que é o recurso central do Telacast), e um terceiro reservado (atividades/soundboard: não implementar; ficam 2 botões de 50%).
- Clicar no nome do canal (linha 2) navega para o chat daquele canal [I].

### 2.5 UserCard (cartão do usuário)

- Altura 52, padding 0 8, gap 8. Frame próprio (sem fundo; o fundo é o da doca, L2).
- Avatar **32** [F: SIZE_32] com bolinha de presença: **diâmetro 10, contorno 3** [F: tabela `SIZE_32:{size:32,status:10,stroke:3}`], posição canto inferior direito, o "contorno" é a cor do fundo da doca (L2). Tabela completa [F], útil para os outros avatares: 16->6/2, 20->6/2, 24->8/3, 32->10/3, 40->12/4, 44->12/4, 48->12/4, 56->14/4, 72->16/6, 80->16/6, 96->20/8, 120->24/8.
- Bloco de texto: nome de exibição 14px semibold `TEXT` (linha 1), status 12px `MUTED` (linha 2: "Online", "Ausente", "Não Perturbe", "Invisível" [F: strings]; ou o status personalizado). Largura: restante menos 3 botões; reticências.
- Hover sobre o bloco (avatar+texto): fundo L4, raio 8, alterna o texto de linha 2 entre o status e o @usuário [I]. Clique abre o menu de presença (6.4): Online / Ausente / Não perturbe / Invisível, "Definir status personalizado", "Trocar de conta", "Copiar ID" (modo dev) e **"Sair"** (hoje o único botão do cartão; vai para dentro do menu, depois de um separador, em `RED`).
- Botões: **microfone, fones, configurações**, cada um 32x32, raio 8, ícone 20 (`refresh_sm`=20 [F]), gap 0 [I]. Estados: normal `MUTED`; hover fundo L4 + `TEXT`; ativo-cortado = ícone com traço diagonal em `RED` (e fones cortados implicam microfone cortado; Discord: ensurdecer também muta o microfone [I]); desativado (sem dispositivo) 40% opacidade. Tooltips: "Silenciar"/"Dessilenciar" [F], "Desativar áudio" [F: `wjcRFX`]/"Ativar áudio", "Configurações do usuário" [I]. Atalhos [F]: **Ctrl+Shift+M** muta, **Ctrl+Shift+D** ensurdece.
- Rolar texto longo: sem marquee; reticências e tooltip com nome completo.

---

## 3. Estados e interações

Temporização: hover 0.12 s (o `anim()` de `theme.rs`), seleção instantânea (o `list_row` atual já é assim), setinha 0.15 s `ease_out`, ações do hover 0.10 s (opacidade). Nada de gradiente, brilho ou fundo animado. "Reduzir movimento" (futuro) zera os tempos. Recolher/expandir categoria **sem animar a altura** [I: evita saltos no modo imediato e é o que o cliente faz visualmente].

| Estado da ChannelRow | Fundo | Ícone | Nome | Extras |
|---|---|---|---|---|
| Padrão | transparente | `MUTED` | `TEXT2` normal | - |
| Hover | L2 | `TEXT2` | `TEXT` | ações (convidar/configurar) aparecem |
| Selecionado | L3 | `TEXT` | `TEXT` | ações sempre visíveis; ganha prioridade sobre hover |
| Não lido | como padrão | `TEXT2` | `TEXT` **semibold** | barra 3x8 na borda esquerda |
| Menção(ões) | idem não lido | idem | idem | selo `AMBER` com a contagem à direita; canal fica visível mesmo silenciado/recolhido [F] |
| Silenciado | como padrão | `MUTED` a 50% | `MUTED` a 55%, nunca negrito | sem barra; só o selo de menção se houver [F: menção mantém visível]; hover e seleção voltam a 100% |
| Privado | qualquer | glifo com cadeado | idem | tooltip "Canal privado" [F: `4qvAtn` é "Canal Privado (bloqueado)"] |
| AO VIVO (Telacast hoje) | idem | idem | idem | texto "AO VIVO" `AMBER` 10px à direita (já existe); em canal de voz vira pílula no participante |
| Voz com usuários | idem | idem | idem | participantes aninhados abaixo |
| Voz cheia (`user_limit`) | idem | idem | `MUTED` | clique bloqueado; tooltip "Canal cheio"; mostrar "N/M" |
| Foco por teclado | contorno 1.5px `AMBER` por dentro | | | setas/Tab (P2) |
| Arrastando (P3) | fantasma L4 a 80% | | | linha-guia de 2px `AMBER` na posição de soltura |
| Carregando | 5 barras estáticas L2 de 34px com raio 8 e larguras 60-90% (sem shimmer) | | | enquanto o `Snapshot` não chegou |
| Vazio | texto 13px `MUTED` "Nenhum canal ainda." + (dono) linha-botão "+ Criar canal" | | | |
| Erro/offline | lista permanece; cliques que dependem do servidor viram toast "Sem conexão" | | | o banner de reconexão é de outra área |

Estados do cabeçalho: padrão (fundo transparente), hover (L2), menu aberto (L3 + chevron como "X"). Estados da categoria: aberta / recolhida (setinha) / com algo não lido enquanto recolhida (o nome vira `TEXT` e as linhas não lidas continuam aparecendo, ver 4.2) / silenciada (nome a 55%).

---

## 4. Regras de comportamento

### 4.1 Ordem

1. Canais **sem categoria** primeiro, sem cabeçalho [F: `noParentCategory` vem antes das categorias nomeadas na lista de linhas].
2. Categorias ordenadas por `position` [F: `sortBy(categoria.position)`].
3. Dentro de cada categoria, ordem por `position`, **mas canais de voz/palco recebem +10 000**, ou seja, todos os de texto/anúncio/fórum primeiro e todos os de voz depois [F: `isGuildVocal() ? position + 1e4 : position`]. Empate: `id` crescente [I].
4. Participantes de voz: ordem [?]. Proposta: `connected_at` crescente, empate por `id` (o estado de voz carrega `connectedAt` [F]).
5. Seções que o Discord tem acima das categorias (notícias do canal, ações do servidor, favoritos, "recentes"/"canais para você") **não entram** (favoritos e recentes dependem de experimento; ver P3).

### 4.2 Nível de visibilidade de cada canal (porte direto de `computeState`) [F]

Avaliação **nesta ordem**, para canais normais (texto/anúncio/fórum/voz/palco):

1. Sem permissão de ver: **escondido** (nível 1). Exceção: é o canal de voz em que o usuário está conectado -> visível.
2. **Visível sempre** (nível 4) se: é o canal selecionado, **ou** é o canal de voz conectado, **ou** é o pai de uma thread selecionada, **ou** tem threads ativas relevantes, **ou** `mention_count > 0`.
3. Se "ocultar silenciados" está ligado no servidor **e** o próprio canal está silenciado: **escondido** (nível 2). (Só o silêncio do próprio canal conta nesta regra; silenciar a categoria não esconde os filhos por aqui.)
4. Se a categoria está **recolhida** e o canal é: silenciado (ele ou a categoria), **ou** de voz/palco, **ou** de texto/anúncio/fórum **sem não lido** -> **escondido pelo recolhimento** (nível 3).
5. Caso contrário: visível (nível 4).

Consequências visíveis: categoria recolhida ainda mostra os canais de texto **com não lido** que não estão silenciados, o selecionado e os com menção; **voz nunca aparece em categoria recolhida a menos que você esteja conectado nele** (é o que o código diz; o cliente real pode destacar voz com gente, mas não achei evidência -> [?], mantenha a regra do código).

Cabeçalho da categoria: aparece se tem canal visível, **ou** tem canais escondidos por recolhimento (nível >= 3), **ou** o usuário tem gerenciar-canais e a categoria está vazia [F: `shouldShowEmptyCategory`]. Categoria cujos filhos estão todos escondidos por "silenciados" ou por falta de permissão **some** [F, por consequência].

### 4.3 Recolher

- Clicar no cabeçalho (qualquer ponto da linha, não só na setinha) alterna. Estado **por categoria e por usuário**, persistido na conta (no cliente é `channel_overrides[].collapsed`, store `CategoryCollapseStore`, chave `collapsedCategories`) [F]. Sincroniza entre dispositivos.
- Menu da categoria: "Recolher categoria" / "Expandir" [F: `SvVRsj`, `rTQZY1`]; no servidor: "Recolher todas as categorias" [F: `9dqzUr`].
- **Ctrl+Shift+A** [F: `mod+shift+a`]: se **alguma** categoria está aberta, recolhe todas; senão expande todas. (Nota: o texto da seção 2.1 de `10-ux-interface.md` atribui Ctrl+Shift+A a "marcar servidor como lido"; no bundle esse atalho é de recolher/expandir categorias.) Categoria sintética "sem categoria" (`id = "null"`) nunca recolhe [F].
- Excluir uma categoria remove a entrada de colapso [F: `CHANNEL_DELETE`].

### 4.4 Silenciar e "ocultar silenciados"

- Silêncio = `muted` + `mute_config.end_time` opcional; `end_time = null` significa "até eu reativar"; com data, o cliente liga um temporizador e desfaz sozinho ao expirar [F: `544743`]. Opções de duração [F: strings]: "Por 15 minutos", "Por 1 hora", "Por 3 horas", "Por 8 horas", "Por 24 horas", "Até eu reativar".
- Silenciar categoria silencia todos os filhos para fins de notificação e de recolhimento [F: `parent_id` verificado em `isCategoryMuted`/regra 4].
- "Ocultar canais silenciados" é **por servidor** (`hide_muted_channels`) [F], item do menu do cabeçalho e do menu de contexto do servidor. Aviso na UI: "Você não receberá notificações de canais silenciados e eles aparecerão em cinza na sua lista de canais." [F: `6yI+JS`].
- Um canal silenciado **com menção** continua visível e com selo [F regra 2].

### 4.5 Não lidos e menções

- Texto/anúncio/fórum: não lido quando `last_message_id > last_read_id`. **Voz e palco nunca contam como não lido** para o servidor/lista, só menção [F: `getGuildHasUnreadIgnoreMuted` ignora canais vocais, salvo `mentionCount != 0`].
- Contagem de menção = alta + baixa importância somadas [F]; para o Telacast, uma contagem só (`mention_count` já existe em `read_state`).
- Abrir o canal marca como lido (ack da última mensagem vista, com o chat rolado até o fim; detalhe de tempo é da área do chat). **Esc** marca o canal como lido, depois de fechar popups/modais [F: `["esc","shift+pagedown"]`]; "Marcar canal como lido" [F: `5X9vFj`], "Marcar servidor como lido" [F: `UaXAPx`] (atalho padrão [?]), categoria "Marcar como lida" [F: `e6RscS`].
- Navegação [F, atalhos padrão do bundle; `mod` = Ctrl no Linux]: **Alt+Cima/Baixo** canal anterior/próximo; **Alt+Shift+Cima/Baixo** próximo canal **não lido**; **Ctrl+Shift+Alt+Cima/Baixo** próximo canal com **menção**; Ctrl+Alt+Direita/Esquerda troca de servidor.
  - Predicado do "próximo não lido": texto/anúncio/fórum -> `(não silenciado OU menções > 0) E não lido`; voz/palco -> só `menções > 0` OU é o canal conectado [F: função `y`]. Pula servidores silenciados.
- Servidor/barra de servidores: o selo vermelho de menção (área 02/barra) soma `mention_count` dos canais visíveis.

### 4.6 Seleção e rolagem

- Clique em canal de texto/anúncio/fórum: abre. Clique em canal de voz: **entra** na chamada (e abre o chat do canal se existir texto-na-voz [I]); clicar no que já está conectado só navega para ele. Duplo clique não é necessário.
- Ao trocar de servidor, a rolagem da lista é **lembrada por servidor** [F: `updateChannelListScroll(guildId, scrollTop)` e `channelListScrollTo`]. No egui basta `ScrollArea::id_salt(("chan_scroll", server_id))`. Ao abrir canal selecionado que está fora da área visível (atalho, convite, link), rolar até ele centralizado.
- Barra de rolagem flutuante (só aparece com o mouse sobre a lista, 6px).

### 4.7 Menus

- **Cabeçalho do servidor** (clique no nome; dono vê todos): "Convidar pessoas" (destaque `AMBER`, abre o modal de convite existente) [F: `MJQOuJ`], "Configurações do servidor" [F: `MeYuqs`], "Criar canal" [F], "Criar categoria" [F: `ISN+NM`], "Silenciar servidor" [F: `vRzp7P`], "Configurações de notificação" [F: `lprV7V`], "Editar perfil do servidor" (apelido) [I], "Ocultar canais silenciados" (marcador) [F], "Marcar como lido" [F: `UaXAPx`], separador, "Sair do servidor" `RED` [F: `cU96ip`]. Convite para quem não é dono: o item existe para todos (hoje o botão "Convite" já é de todos). A ordem exata do menu no cliente é [?].
- **Canal** (botão direito): "Marcar canal como lido", "Silenciar canal" (submenu com as 6 durações), "Configurações de notificação", separador, "Editar canal", "Duplicar canal" [F: `dEaPc4`], "Copiar link do canal" [F: `A+Fv0R`], "Excluir canal" `RED`. Itens de gestão só com permissão.
- **Categoria**: "Marcar como lida", "Recolher categoria", "Silenciar categoria", "Configurações de notificação", "Editar categoria" [F: `zdPFs9`], "Criar canal", "Excluir categoria" [F: `ifbXnL`] `RED`.
- Menus em `egui` com `Response::context_menu`; itens perigosos `RichText::color(RED)`; setinha de submenu com `menu_button`.

### 4.8 Arrastar e redimensionar (P3)

- Reordenar canais e categorias por arrastar, com permissão; não pode soltar canal de voz acima de texto (a ordenação +10 000 faz isso valer sozinha) [F por consequência].
- Largura do painel: o cliente tem uma barra lateral **redimensionável, mín. 264, máx. 432, padrão 375, e 76 quando colapsada** [F: `P_`, `Pu(375)`, `minDimension:264, maxDimension:432`, `--custom-guild-sidebar-width`], com setas para 10 px por passo, Home/End = mín./máx. A relação com a barra de servidores (se esses 375 incluem os 72-76 da coluna de servidores) é [?]: 375-76 = 299 de canais no padrão. O Telacast usa rail de 68 + painel de 240. Proposta: arrastar a borda direita, 200 a 360, padrão 240, guardar em `eframe::Storage`.

---

## 5. Dados necessários (o que falta hoje)

Estado atual: `proto::Channel{id,name}`, `ServerInfo{id,name,owner,invite,channels,members}`, `Live{channel_id,user_id}`, `User{id,name,online}`; unread só em memória (`Session.unread: HashSet<i64>`); DB já tem `channels(kind, parent_id, position)` e `read_state(last_read_id, mention_count)`, **sem uso no protocolo**; sem status de presença além de `online`; sem voz.

### P0 - estrutura da árvore e leitura

1. `proto::Channel` ganha `kind: u8` (0 texto, 1 voz, 2 anúncio, 3 palco, **4 categoria**, **5 fórum** (novo; atualizar o comentário da migração), `parent_id: Option<i64>`, `position: i32`, `private: bool`, `user_limit: Option<u16>` (voz). Categorias vão na mesma lista `channels` com `kind = 4`.
2. `ServerInfo` ganha `perms` do usuário (bitmask simples: `MANAGE_CHANNELS`, `CREATE_INVITE`, `MANAGE_SERVER`), hoje deduzido de `owner == me.id`.
3. Estado de leitura: `Snapshot.read: Vec<ReadState{channel_id, last_read_id, last_message_id, mention_count}>` e `S2C::ReadState{channel_id,...}` atualizando quando chega mensagem; `C2S::Ack{channel_id, message_id}`. Consulta de não lidos do `Snapshot` (usa o índice `messages_channel_id (channel_id, id DESC)`):
   ```sql
   SELECT c.id, COALESCE(rs.last_read_id,0) AS last_read,
          (SELECT max(m.id) FROM messages m WHERE m.channel_id = c.id) AS last_msg,
          COALESCE(rs.mention_count,0) AS mentions
   FROM channels c JOIN members mb ON mb.server_id = c.server_id AND mb.user_id = $1
   LEFT JOIN read_state rs ON rs.user_id = $1 AND rs.channel_id = c.id
   WHERE c.server_id = ANY($2) AND c.kind IN (0,2,5);
   ```
4. Preferências da conta, nova migração `0002`: `channel_overrides(user_id, channel_id, collapsed bool, muted_until timestamptz NULL, muted bool, PRIMARY KEY(user_id, channel_id))` (categoria e canal usam a mesma tabela, como no cliente [F: `channel_overrides`]), `guild_settings(user_id, server_id, hide_muted bool, muted bool, muted_until)`. `S2C::Settings{...}` no `Snapshot` e em atualizações; `C2S::SetCollapsed{channel_id,bool}`, `C2S::SetMute{target: Channel|Server, until: Option<i64>|Forever}`, `C2S::SetHideMuted{server_id,bool}`, `C2S::CollapseAll{server_id,bool}`.
5. Eventos de canal: `S2C::ChannelCreate/Update/Delete`, `C2S::CreateChannel{server_id,name,kind,parent_id}` (hoje só nome), `C2S::CreateCategory`, `C2S::UpdateChannel`, `C2S::DeleteChannel`.

### P1 - presença, cartão do usuário e voz

6. `User.status: Status{Online,Idle,Dnd,Invisible}` + `custom_status: Option<String>`; `C2S::SetStatus`; `S2C::Presence{user_id,status,custom}`. Colunas `users.status`, `users.custom_status` (ou tabela `presence` em memória no servidor; invisível aparece como offline para os outros).
7. Voz (área 04 define o transporte; esta área precisa só do estado de exibição): `VoiceState{user_id, channel_id, self_mute, self_deaf, mute, deaf, self_video, self_stream, connected_at}` [F: campos do cliente `selfMute, selfDeaf, mute, deaf, selfVideo, selfStream, suppress, connectedAt`]; `Snapshot.voice: Vec<VoiceState>`; `S2C::VoiceState` por mudança; `C2S::VoiceJoin{channel_id}`, `VoiceLeave`, `VoiceSet{self_mute,self_deaf}`. Latência e `status` da conexão ficam só no cliente.
8. `Live` ganha relação com canal de voz (hoje `channel_id` serve a qualquer canal): ver pergunta 1.

### P2/P3

9. `server_invites` já existe; `C2S::CreateInvite{server_id, channel_id?, max_uses, expires}` para a ação de convidar por canal.
10. Banner/ícone do servidor (`servers.banner`, upload), P3. Favoritos e "canais recentes" (P3, dependem de experimento no cliente [F: `enable_recently_active`]).
11. Permissão por canal (`channel_overrides` de cargo/usuário) é da área de cargos; até lá `private` vem do servidor já filtrado e a lista só contém o que o usuário pode ver.

Índices novos: `channel_overrides (user_id)` (carga do snapshot), nada em `channels` (já tem `channels_server`).

---

## 6. Plano de implementação em egui 0.37

### 6.1 Armadilhas do egui que mudam o plano

- **Sem negrito real**: `Inter.ttf` do projeto (876 KB) é fonte variável e o egui não escolhe peso. `RichText::strong()` só troca a cor para `strong_text_color`, não engrossa (confirme em `main.rs:613`: hoje o "negrito" de não lido é só cor). Para "não lido em negrito", categorias e nome do usuário: adicionar um **arquivo estático** `Inter-SemiBold.ttf` e registrar `FontFamily::Name("semibold")` em `apply_theme` (igual ao que `serif_family()` já faz). Helper `pub fn semibold(text, size) -> RichText`.
- **Sem letter-spacing**: o cabeçalho de categoria em caixa alta fica um pouco apertado; compensar com 0.5px a mais de gap entre letras só se der (desenhar letra a letra é caro), senão aceitar.
- **Ícones**: duas rotas. **A** (painter): desenhar com traços e polígonos convexos (hash, setinha, cadeado, alto-falante) - sem dependência, mas 10+ glifos dão trabalho e ficam feios em 12-16px. **B (recomendada)**: embutir um **arquivo de fonte de ícones de licença livre** (ex.: Phosphor, MIT) como `FontFamily::Name("icons")` e usar codepoints num `pub const`; cor/tamanho/animação ficam triviais (`RichText::new('\u{e...}').font(FontId::new(16.0, icons()))`). Os codepoints saem do arquivo de metadados do próprio pacote. Não copiar os SVGs do Discord. Verificar se o crate `egui-phosphor` já suporta 0.37 [?]; se não, embutir o `.ttf` direto.
- **Hover sobre filhos**: ao pôr botões (convidar/engrenagem) em cima da linha, `resp.hovered()` da linha vira falso. Usar `ui.rect_contains_pointer(rect)` para o estado de hover da linha e `ui.interact(...)` para os botões **depois** de alocar a linha (o último registrado fica por cima e recebe o clique).
- **Texto longo**: use `LayoutJob` com `wrap.max_rows = 1` e `overflow_character = Some('…')`; `painter.text` não trunca.
- **Sem emoji colorido**: nomes de canal com emoji aparecem sem cor ou quadrados; filtrar/sinalizar (área 03-mensagens).
- **Modo imediato**: reconstruir `build_rows` por frame é barato (dezenas de canais), mas cacheie por `(server_id, version)` com versão incrementada ao receber `ChannelUpdate/ReadState/Settings`. Lista longa (>200 linhas): `ScrollArea::show_rows` não serve (alturas diferentes entre cabeçalho 32 e linha 34/32), então virtualize manualmente com `ui.clip_rect()` e `allocate_space` para as linhas fora da janela.

### 6.2 Onde entra cada coisa

- `theme.rs`: constantes `CH_ROW_H=34.0`, `CAT_H=24.0`, `VUSER_H=32.0`, `DOCK_USER_H=52.0`; fontes `semibold`/`icons` em `apply_theme`; widgets `chevron(ui, rect, open_t, color)`, `badge(ui, count)`, `icon_button(ui, glyph, size, active_color) -> Response`, `status_dot(painter, center, avatar_size, status, surface)` com a tabela [F] (substitui o cálculo `size*0.11` de `avatar()`), `semibold()`.
- Novo `app/src/channels.rs` (módulo): `Kind`, `ChanView`, `Level`, `Row`, `build_rows(...)` (puro, sem egui, testável), `channel_tree(ui, ...)`, `server_header(ui, ...)`, `dock(ui, ...)`.
- `main.rs`: `channel_panel` só chama as três funções; `Session` ganha `collapsed: HashSet<i64>`, `muted`, `hide_muted`, `read: HashMap<i64, ReadState>`, `voice: Vec<VoiceState>`, `status`. `Act` ganha `ToggleCollapse(i64)`, `Mute{..}`, `Ack(i64)`, `CreateChannel{kind,parent}`, `SetStatus(..)`.
- `Modal::NewChannel` ganha seletor de tipo (Texto, Voz, Anúncio, Palco, Fórum; as strings "Criar canal de texto"/"Criar canal de voz"/"Criar canal palco" existem [F]) e categoria de destino.

### 6.3 Trechos difíceis (esboços, não compilados)

**(1) Porte de `renderLevel` e construção das linhas** (puro; é a parte de maior valor):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind { Text, Voice, Announce, Stage, Category, Forum }
impl Kind {
    pub fn from_db(k: i16) -> Self { match k { 1 => Self::Voice, 2 => Self::Announce, 3 => Self::Stage, 4 => Self::Category, 5 => Self::Forum, _ => Self::Text } }
    pub fn is_voice(self) -> bool { matches!(self, Self::Voice | Self::Stage) }
    pub fn is_text_like(self) -> bool { matches!(self, Self::Text | Self::Announce | Self::Forum) }
}

pub struct ChanView { pub id: i64, pub name: String, pub kind: Kind, pub parent: Option<i64>, pub position: i32, pub private: bool }

pub struct Cx<'a> {
    pub selected: Option<i64>,          // canal aberto
    pub voice_channel: Option<i64>,     // canal de voz conectado
    pub hide_muted: bool,               // preferência do servidor
    pub collapsed: &'a HashSet<i64>,    // categorias recolhidas
    pub muted: &'a HashSet<i64>,        // canais e categorias silenciados (já sem os expirados)
    pub unread: &'a HashSet<i64>,
    pub mentions: &'a HashMap<i64, u32>,
    pub can_manage: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level { NoAccess = 1, Hidden = 2, HiddenByCollapse = 3, Shown = 4 }

pub fn level(c: &ChanView, cx: &Cx) -> Level {
    let mentions = cx.mentions.get(&c.id).copied().unwrap_or(0);
    if cx.selected == Some(c.id) || cx.voice_channel == Some(c.id) || mentions > 0 {
        return Level::Shown;
    }
    let own_muted = cx.muted.contains(&c.id);
    let muted = own_muted || c.parent.is_some_and(|p| cx.muted.contains(&p));
    if cx.hide_muted && own_muted { return Level::Hidden; }
    let collapsed = c.parent.is_some_and(|p| cx.collapsed.contains(&p));
    if collapsed && (muted || c.kind.is_voice() || (c.kind.is_text_like() && !cx.unread.contains(&c.id))) {
        return Level::HiddenByCollapse;
    }
    Level::Shown
}

pub enum Row { Header(i64), Chan(i64), VoiceUser { chan: i64, idx: usize }, EmptyHint(i64) }

pub fn build_rows(chans: &[ChanView], cx: &Cx, voice_users: &HashMap<i64, Vec<VoiceState>>) -> Vec<Row> {
    let key = |c: &&ChanView| (c.kind.is_voice() as i32 * 10_000 + c.position, c.id); // [F: +1e4 para voz]
    let mut rows = Vec::new();
    let mut push_chan = |rows: &mut Vec<Row>, c: &ChanView| {
        rows.push(Row::Chan(c.id));
        if c.kind.is_voice() {
            for i in 0..voice_users.get(&c.id).map_or(0, |v| v.len()) { rows.push(Row::VoiceUser { chan: c.id, idx: i }); }
        }
    };
    let mut loose: Vec<&ChanView> = chans.iter().filter(|c| c.parent.is_none() && c.kind != Kind::Category).collect();
    loose.sort_by_key(key);
    for c in loose.into_iter().filter(|c| level(c, cx) == Level::Shown) { push_chan(&mut rows, c); }

    let mut cats: Vec<&ChanView> = chans.iter().filter(|c| c.kind == Kind::Category).collect();
    cats.sort_by_key(|c| (c.position, c.id));
    for cat in cats {
        let mut kids: Vec<&ChanView> = chans.iter().filter(|c| c.parent == Some(cat.id)).collect();
        kids.sort_by_key(key);
        let lv: Vec<Level> = kids.iter().map(|c| level(c, cx)).collect();
        let any_shown = lv.contains(&Level::Shown);
        let any_collapsed_hidden = lv.contains(&Level::HiddenByCollapse);
        if any_shown || any_collapsed_hidden || (cx.can_manage && kids.is_empty()) {   // [F: shouldShowEmptyCategory]
            rows.push(Row::Header(cat.id));
            for (c, l) in kids.iter().zip(&lv) { if *l == Level::Shown { push_chan(&mut rows, c); } }
            if kids.is_empty() { rows.push(Row::EmptyHint(cat.id)); }
        }
    }
    rows
}
```

Testes de unidade (rodar quando liberarem o `cargo`): categoria recolhida só mostra não lido de texto; voz some salvo conectado; `hide_muted` esconde silenciado mas não com menção; categoria sem filhos visíveis some; ordem texto antes de voz.

**(2) Linha de canal com ações no hover** (hover correto, ações por cima, truncamento):

```rust
pub struct RowOut { pub open: bool, pub invite: bool, pub gear: bool, pub menu_at: Option<Pos2> }

pub fn channel_row(ui: &mut Ui, c: &ChanView, st: &ChanState, selected: bool, perms: Perms) -> RowOut {
    let mut out = RowOut::default();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), CH_ROW_H), Sense::click());
    let over = ui.rect_contains_pointer(rect);                       // continua true sobre os ícones filhos
    let hot = ui.ctx().animate_bool_with_time(resp.id.with("h"), over && !selected, 0.12);
    let p = ui.painter();
    let r = egui::CornerRadius::same(8);
    if selected { p.rect_filled(rect, r, L3); } else if hot > 0.0 { p.rect_filled(rect, r, lerp(L1, L2, hot)); }

    let dim = if st.muted && !selected && !over { 0.55 } else { 1.0 };
    let name_col = if selected || over || st.unread || st.mentions > 0 { TEXT } else { TEXT2 };
    let ico_col = if selected || over { TEXT } else if st.unread { TEXT2 } else { MUTED };

    // barra de não lido (fora da linha, na borda do painel)
    if st.unread && !st.muted && !selected {
        let bar = Rect::from_min_size(pos2(rect.left() - 8.0, rect.center().y - 4.0), vec2(3.0, 8.0));
        p.rect_filled(bar, 2.0, TEXT);
    }
    // ícone
    let ico = Rect::from_center_size(pos2(rect.left() + 8.0 + 10.0, rect.center().y), vec2(20.0, 20.0));
    paint_kind_icon(p, ico, c.kind, c.private, ico_col.gamma_multiply(dim));

    // lado direito, da direita para a esquerda: selo, depois ações
    let mut right = rect.right() - 8.0;
    if st.mentions > 0 { right -= badge(ui, pos2(right, rect.center().y), st.mentions).width() + 4.0; }
    if (over || selected) && !c.kind.is_category() {
        if perms.manage {
            let g = Rect::from_center_size(pos2(right - 10.0, rect.center().y), vec2(20.0, 20.0));
            let rg = ui.interact(g, resp.id.with("gear"), Sense::click());      // registrado depois => fica por cima
            paint_glyph(ui.painter(), g, Glyph::Gear, if rg.hovered() { TEXT } else { MUTED });
            rg.clone().on_hover_text("Editar canal");
            out.gear = rg.clicked(); right -= 24.0;
        }
        if perms.invite {
            let i = Rect::from_center_size(pos2(right - 10.0, rect.center().y), vec2(20.0, 20.0));
            let ri = ui.interact(i, resp.id.with("inv"), Sense::click());
            paint_glyph(ui.painter(), i, Glyph::UserPlus, if ri.hovered() { TEXT } else { MUTED });
            ri.clone().on_hover_text("Criar convite");
            out.invite = ri.clicked(); right -= 24.0;
        }
    }
    // nome truncado até `right`
    let text_rect = Rect::from_min_max(pos2(ico.right() + 8.0, rect.top()), pos2(right - 4.0, rect.bottom()));
    let font = if st.unread && !st.muted { semibold_font(14.5) } else { FontId::proportional(14.5) };
    let mut job = egui::text::LayoutJob::simple_singleline(c.name.clone(), font, name_col.gamma_multiply(dim));
    job.wrap.max_width = text_rect.width(); job.wrap.max_rows = 1; job.wrap.overflow_character = Some('…');
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(text_rect.left(), rect.center().y - g.size().y / 2.0), g, name_col);

    out.open = resp.clicked() && !(out.gear || out.invite);
    if resp.secondary_clicked() { out.menu_at = resp.interact_pointer_pos(); }
    if over { ui.ctx().set_cursor_icon(CursorIcon::PointingHand); }
    out
}
```

**(3) Setinha animada e glifos simples sem fonte de ícones** (rota A):

```rust
/// t = 0 recolhida (aponta para a direita), 1 aberta (aponta para baixo).
pub fn chevron(p: &Painter, c: Pos2, size: f32, t: f32, col: Color32) {
    let a = std::f32::consts::FRAC_PI_2 * t;                  // 0..90 graus
    let rot = |v: Vec2| vec2(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos());
    let s = size * 0.28;
    let pts = [c + rot(vec2(-s * 0.6, -s)), c + rot(vec2(s * 0.6, 0.0)), c + rot(vec2(-s * 0.6, s))];
    p.add(Shape::line(pts.to_vec(), Stroke::new(1.6, col)));
}

pub fn hash(p: &Painter, c: Pos2, s: f32, col: Color32) {      // "#"
    let st = Stroke::new(1.5, col); let k = s * 0.5; let sk = s * 0.12;
    for dx in [-s * 0.18, s * 0.18] { p.line_segment([c + vec2(dx + sk, -k), c + vec2(dx - sk, k)], st); }
    for dy in [-s * 0.16, s * 0.16] { p.line_segment([c + vec2(-k, dy), c + vec2(k, dy)], st); }
}

pub fn speaker(p: &Painter, c: Pos2, s: f32, col: Color32) {   // dois polígonos convexos (o contorno inteiro é côncavo)
    let u = s / 16.0;
    p.add(Shape::convex_polygon(vec![c + vec2(-6.0*u,-2.5*u), c + vec2(-3.0*u,-2.5*u), c + vec2(-3.0*u,2.5*u), c + vec2(-6.0*u,2.5*u)], col, Stroke::NONE));
    p.add(Shape::convex_polygon(vec![c + vec2(-3.0*u,-2.5*u), c + vec2(1.0*u,-6.0*u), c + vec2(1.0*u,6.0*u), c + vec2(-3.0*u,2.5*u)], col, Stroke::NONE));
    let arc: Vec<Pos2> = (0..=8).map(|i| { let a = -0.9 + 1.8 * i as f32 / 8.0; c + vec2(2.0*u + 4.0*u*a.cos(), 4.0*u*a.sin()) }).collect();
    p.add(Shape::line(arc, Stroke::new(1.4, col)));
}
```

**(4) Doca: cartão do usuário e menu de presença** (esqueleto; ajustar a assinatura de `Popup` à 0.37):

```rust
fn dock(&mut self, ui: &mut Ui, s: &Session, acts: &mut Vec<Act>) {
    egui::Frame::new().fill(L2).corner_radius(10).inner_margin(egui::Margin::same(0)).show(ui, |ui| {
        if let Some(v) = &s.voice_conn { voice_panel(ui, v, acts); rule_bleed(ui, 0.0); }
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), DOCK_USER_H), Sense::hover());
        let mut u = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(8.0, 0.0))).layout(Layout::left_to_right(Align::Center)));
        // bloco clicável avatar + textos
        let w = u.available_width() - 3.0 * 32.0 - 8.0;
        let (blk, br) = u.allocate_exact_size(vec2(w, 40.0), Sense::click());
        let hot = u.ctx().animate_bool_with_time(br.id, br.hovered(), 0.12);
        if hot > 0.0 { u.painter().rect_filled(blk, 8.0, alpha(L4, hot)); }
        let av = Rect::from_min_size(blk.min + vec2(4.0, 4.0), vec2(32.0, 32.0));
        paint_avatar(u.painter(), av, &s.me, L2);
        status_dot(u.painter(), av.center(), 32.0, s.status, hot_surface(L2, L4, hot));  // bolinha 10px, contorno 3px [F]
        // nome (semibold 14) e status (12 MUTED), truncados em blk.right()
        // ...
        Popup::menu(&br).show(|ui| {                         // Online / Ausente / Não perturbe / Invisível / status personalizado / Sair
            for (st, label) in [(Status::Online,"Online"),(Status::Idle,"Ausente"),(Status::Dnd,"Não Perturbe"),(Status::Invisible,"Invisível")] {
                if ui.selectable_label(s.status == st, label).clicked() { acts.push(Act::SetStatus(st)); ui.close(); }
            }
            ui.separator();
            if ui.button(RichText::new("Sair").color(RED)).clicked() { acts.push(Act::Logout); ui.close(); }
        });
        // três botões 32x32: mic, fones, engrenagem (icon_button); mute = traço diagonal RED
    });
}
```

---

## 7. Ordem de entrega (passos pequenos e verificáveis)

1. **Visual da linha e do cabeçalho com os dados atuais** (sem protocolo novo): fonte semibold, glifo `#`, altura 34, hover/selecionado, não lido em negrito de verdade + barra 3x8, "AO VIVO" mantido; cabeçalho 52 px com chevron e menu suspenso contendo "Convidar pessoas", "Criar canal" (dono), "Sair do servidor" (placeholder desabilitado). *Usuário vê:* lista mais próxima do Discord e o menu abrindo ao clicar no nome.
2. **Cartão do usuário e doca**: avatar 32 com bolinha da tabela, nome/status, botões microfone/fones/engrenagem (por enquanto só alternam estado local e o ícone muda), "Sair" movido para o menu do avatar. *Vê:* cartão novo; mutar/ensurdecer troca o ícone e responde a Ctrl+Shift+M/D.
3. **Protocolo de árvore** (`kind/parent/position`, categorias no snapshot, `CreateChannel` com tipo e pai): categorias aparecem, ordem texto-antes-de-voz, ícones por tipo, "+" por categoria. Recolher **local** (guardado em `eframe::Storage`). *Vê:* categorias recolhíveis que lembram o estado.
4. **Leitura real**: `Snapshot.read`, `S2C::ReadState`, `C2S::Ack`; selo de menção, não lido por canal, Esc marca como lido, Alt+Shift+Cima/Baixo e Ctrl+Shift+Alt+Cima/Baixo. *Vê:* o não lido sobrevive a reiniciar o app.
5. **Silenciar e ocultar silenciados**: tabela `channel_overrides`/`guild_settings`, menus de contexto de canal/categoria/servidor com as durações, regra de visibilidade completa (`build_rows` + testes), recolher passa a ser da conta e Ctrl+Shift+A. *Vê:* canais em cinza, somem com "Ocultar silenciados", categoria recolhida mostra só não lidos.
6. **Presença**: `Status` + menu do avatar, bolinha na cor certa (OK/AMBER/RED?/MUTED) no cartão e na lista de membros.
7. **Painel "Voz conectada" e participantes aninhados** (depende da área 04): estados de conexão, latência, desconectar, câmera, transmitir; lista de participantes com avatar 24, anel ao falar, ícones de mudo/surdo/AO VIVO.
8. **Acabamento (P2/P3)**: ações de hover reais (convidar por canal, editar), arrastar para reordenar, largura redimensionável, banner do servidor, teclado (setas na lista), barra "não lidos abaixo".

---

## 8. Perguntas em aberto (só as que mudam o que será construído)

1. **Transmissão ao vivo**: hoje qualquer canal de texto pode ter "AO VIVO" (`Live.channel_id`). Mover a transmissão para dentro de **canais de voz** (modelo do Discord: participantes com selo AO VIVO, "Transmitir tela" no painel de voz) ou manter por canal de texto? Muda o modelo de dados e onde o botão mora.
2. **Largura do painel**: fixa em 240 ou redimensionável (200-360) como no cliente atual?
3. **Cor do mudo**: o Grafite reserva `RED` para erro/destrutivo. Microfone/fones cortados em `RED` (como no Discord) ou usar `AMBER`/`TEXT` com traço?
4. **Fonte**: posso adicionar `Inter-SemiBold.ttf` estático (~300 KB) e uma fonte de ícones (Phosphor, ~150 KB) ao `assets/fonts`? Sem isso, "negrito" e ícones decentes não saem.
5. **Tipos de canal**: renderizar Palco e Fórum já (só ícone e criação, sem função) ou esconder do seletor "Criar canal" até existirem?
6. **Convidar**: continua um botão por servidor para todos os membros, ou passa a ser permissão (dono/cargo) e ação por canal?
