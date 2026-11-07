# 10 - Tokens de design, movimento, markdown, atalhos e acessibilidade

Área: escalas reais do cliente (espaço, raio, sombra, tipografia, ícones, avatares), movimento (durações, easings, molas, reduzir movimento), markdown/texto rico do chat, atalhos globais e acessibilidade. Tradução para o Grafite e plano em egui 0.37.

Legenda: **[F]** confirmado no código do cliente, **[I]** inferência, **[?]** não confirmado.

Fontes lidas (somente leitura):
- JS: `discord-bundle/web.0bcf9a9e83b2c3c7.js` (o maior; conferido contra `web.3f3154a1...`). Módulos citados pelo offset do arquivo (`@N`).
- CSS: o `discord-bundle/*.css` só traz variáveis de cor. O CSS dos componentes (CSS-modules) está em `scratchpad/barra/css/*.css` (1813 arquivos, extraídos por outro agente). Cito pelo prefixo do arquivo e pela classe (ex.: `738015.* .cozy .timestamp`). As classes têm hash; ignoro o hash.
- Strings pt-BR: `scratchpad/tools/pt.json`.
- Código do egui 0.37 em `~/.cargo/registry/src/*/{egui,epaint,emath}-0.37.0` (só leitura; nada foi compilado).

---

## 1. Resumo

1. O Discord tem **dois sistemas de token**: um módulo JS (`Spacing`, `Space` por densidade, `radii`, `Modules`, `Shadows`) [F @12256000-12285600] e o CSS-modules por componente. Os números abaixo vêm dos dois e estão marcados.
2. Escala base: espaço em múltiplos de 4 (4/8/12/16/20/24/32), raios 4/8/12/16/24/32, fonte-base **16 px** (`FONT_SIZE_DEFAULT`) com linha 22 px no chat (`1.375rem`), avatar de chat 40 px, ícones 12/16/18/24/32.
3. Movimento: UI "de cor" (hover) em **50-150 ms**, entrada e saída de camadas com **molas** do react-spring (tooltip 2400/52, modal 1000/48, popout 800/32). Tudo que move geometria fica sob a classe `.full-motion`; "reduzir movimento" remove a classe e as molas passam a só fazer fade.
4. Markdown = **simple-markdown** com regras próprias (ordem, regexes e limites abaixo). Armazenamento é o texto cru do Discord (`<@id>`, `<:nome:id>`, `<t:unix:R>`), a renderização é do cliente.
5. **Correção importante das premissas do `00-instrucoes.md`**: o egui 0.37 já tem (a) **fonte variável**: `TextFormat.coords: VariationCoords` (eixo `wght`) e o `Inter.ttf` do projeto é variável (wght 100-900, opsz 14-32); (b) **letter-spacing**: `TextFormat.extra_letter_spacing` e `RichText::extra_letter_spacing`; (c) **altura de linha explícita**: `TextFormat.line_height`; (d) **emoji colorido**: `epaint` tem a feature `color_fonts` (COLR/sbix/CBDT) que fica desligada por padrão; o `NotoColorEmoji.ttf` do sistema é CBDT (11 MB). Ver 6.1. Itálico continua sintético (inclinação), sem eixo `ital`.
6. Atalhos: cerca de 50 ações com binds extraídas do cliente; `mod` = Ctrl no Linux. Acessibilidade: o `eframe` já liga `accesskit` por padrão; falta rotular o que é pintado à mão.

Fora do escopo: cores finais de cargo/tema do usuário, anexos/galeria, embeds ricos (só o essencial do link), voz, as medidas das colunas (outras áreas), push-to-talk global (fim de 6.5).

---

## 2. Árvore de componentes (tokens e blocos de texto rico)

### 2.1 Escala de espaço [F]

`Layout.Space` (px): 0, 4, 6, 8, 10, 12, 16, 20, 24, 26, 30, 32, 40, 48, 64, 80, 96, 128, 160, 192. `Spacing` (outra escala, mobile/mana): 4, 8, 12, 16, 24, 32, 40, 48, 56, 64, 72, 80, 96.

Tokens semânticos dependem da densidade (`density-compact|default|cozy`; `default` é o normal):

| token | compact | **default** | cozy |
|---|---|---|---|
| space-xxs | 4 | **4** | 4 |
| space-xs | 6 | **8** | 10 |
| space-sm | 10 | **12** | 16 |
| space-md | 12 | **16** | 20 |
| space-lg | 16 | **20** | 24 |
| space-xl | 20 | **24** | 30 |
| space-xxl | 26 | **32** | 40 |

Espaço entre grupos de mensagens (configuração "Espaço entre grupos de mensagens"): opções `[0, 4, 8, 16, 24]`, padrão 16; a margem real do início de grupo é o valor **+1 px** (`group-spacing-16 → 1.0625rem`) [F 381941 e CSS 524434]. Breakpoints: 480, 640, 768, 1024, 1280, 1536, 1800, 2500; mobile = 768 [F].

Chat (CSS 738015.*): `padding-top/bottom` de cada mensagem 2 px (0.125rem), `padding-right` 16 (space-md), conteúdo alinhado em `margin-left = avatar 40 + 16 + 16 = 72 px` (4.5rem), avatar em `left: 16`, `margin-top: 4px - 2px`. Altura mínima de uma mensagem com cabeçalho: 2.75rem (44 px); mensagem de sistema: 1.375rem (22 px) [F].

### 2.2 Raios [F]

`radii`: none 0, xs 4, sm 8, md 12, lg 16, xl 24, xxl 32, round 2147483647. Usos confirmados no CSS:

| elemento | raio |
|---|---|
| menu de contexto (container) / item focado | 8 / 4 (item base 2) |
| tooltip | 8 (+ seta de 5 px) |
| modal (mana) | 12 (md) |
| menção, carimbo `<t:>` | 3 |
| código em linha, bloco de código, embed, mídia de embed | 4 |
| reação (pílula) | 8 |
| avatar, ícone de resposta | 50% |
| barra de rolagem (thumb) | 4 (fina) / 7 (14 px) |
| botão (mana, desktop) | pílula (`BORDER_RADIUS = 0x7fffffff`); 8 sob o experimento mobile |
| spoiler inline | 4 (`radius-xs`) |

### 2.3 Sombras e elevação [F @12204000]

Tema escuro (os valores de `ASH`/`DARK`/`ONYX` são iguais; o claro usa 0.08):

| token | sombra |
|---|---|
| BORDER | `0 0 0 1px` branco 8% |
| LOW | `0 1px 4px` preto 14% |
| LOW_HOVER | `0 4px 10px` preto 14% |
| MEDIUM | `0 4px 8px` preto 16% |
| HIGH | `0 12px 24px` preto 24% |
| TOP_HIGH | `0 -12px 32px` preto 24% |
| LEDGE | `0 2px/1.5px/1px` preto 5/5/16% (separador sob o cabeçalho) |

Quem usa: menu e tooltip e modal = `HIGH` + borda `1px` `border-subtle`; avatar do chat em hover = `drop-shadow(elevation-medium)` só em modo mouse; barra de botões flutuante de hover = `LOW`. A borda de 1 px (`--border-subtle`, branco 12%) faz mais trabalho que a sombra.

### 2.4 Tipografia [F: CSS `121046.*` e `230322.*`]

Fontes: `--font-primary` "gg sans" (fallback Noto Sans), `--font-code` "gg mono", `--font-nitro`/`--font-headline` (display, só marketing/Nitro). Pesos: `normal 400, medium 500, semibold 600, bold 700, extrabold 800` (`light 300`, `black 900` existem).

Escala de texto (tamanho/linha em px; a linha é o `line-height` unitless × tamanho):

| variante | tamanho / linha | uso |
|---|---|---|
| text-xxs / footnote | 10 / 12 | "(editado)", rodapé |
| text-xs / body-xs | 12 / 16 | carimbo cozy, rótulos pequenos |
| text-sm / body-sm | 14 / 18 | menu, tooltip, contagem de reação, legenda |
| text-md / body-md | 16 / 20 | corpo de UI (o chat usa linha 22, ver abaixo) |
| text-lg / body-lg | 20 / 24 | destaque |
| heading-xs | 14 / 18 | w500-600 |
| heading-sm | 14 / 18 (ou 16 / 20) | |
| heading-md | 16 / 20 (ou 18 / 22) | |
| heading-lg | 20 / 24 | título de modal |
| heading-xl | 24 / 30-32 | |
| heading-xxl | 32 / 40 | |
| eyebrow | 12 / 16, w700, `letter-spacing .02em`, MAIÚSCULAS | rótulo de seção |
| code / mono-md | 12 / 16 (`--font-code`) | |
| display-xs..lg | 16 / 24 / 32 / 44, w700, MAIÚSCULAS | marketing |

Tamanhos viram `rem` quando a classe `fontScaling` está ativa, e o `rem` raiz = `fontSize` da acessibilidade: **12, 14, 15, 16 (padrão), 18, 20, 24** [F `FONT_SIZES`]. Zoom de interface (separado): `[50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200]` %.

**Papéis no chat (CSS 738015.* e 44141.*)**:

| papel | tamanho / linha | peso | cor (token Discord) |
|---|---|---|---|
| corpo (`.markup`) | 1rem / **1.375rem (22)**, `white-space: break-spaces` | 400 | text-default |
| nome do autor | 1rem / 1.375rem | **500** (tema claro 600) | text-strong (ou cor do cargo) |
| carimbo ao lado do nome | 0.75rem / 1.375rem, `margin-left 8` | 500 | chat-text-muted |
| carimbo na margem (hover, mensagens seguintes) | 0.6875rem (11) / 1.375rem, largura 56, alinhado à direita | 500 | text-muted |
| "(editado)" | 0.625rem (10) / 1 | 400 | text-muted |
| código em linha | 85% do pai, linha 1.125rem, `padding 0 .2em`, `margin -.2em 0` | 400 | |
| bloco de código `pre > code` | 0.875rem (14) / 1.125rem (18); `pre` em 0.75rem/1rem | 400 | text-code |
| `# h1` / `## h2` / `### h3` | 1.5rem / 1.25rem / 1rem; linha 1.375em; margem `16 0 8` (primeiro filho: 8 em h1-h2, 4 em h3) | 700 | text-strong |
| `-# subtexto` | 0.875rem / 1.203rem | 400 | text-subtle |
| contagem de reação | 14 | 600 | text-subtle |
| resposta (prévia) | 0.875rem / 1.125rem | 400 | text-default |
| tooltip | 14 / 16 | 500 | text-default |
| item de menu (label) | 14 / 20 | 500 | text-strong |
| rótulo de grupo no menu | 14 / 20 | 500 | text-muted |

Emoji no texto: `--custom-emoji-size-emoji: 1.375em` (22 px a 16 px), `vertical-align: bottom`; jumbo `3rem` (48 px) quando a mensagem só tem emoji (ver 4.1); em resposta/prévia `1.25em`.

### 2.5 Ícones [F]

`icon`: XXS 12, XS 16, SM 18, MD 24, LG 32. Chat: ícone da caixa de texto 20 (18 em densidade compacta), botão de mensagem 20, ícone de cargo `calc(1rem + 4px)` = 20, avatar de resposta 16, seta de submenu 20, container de ícone de menu 20 (grande 32), selo de bot 18 de altura, ícone de "sistema" 12.

### 2.6 Avatares [F @ Avatar sizes]

| tamanho | status (diâmetro) | contorno (stroke) | offset | uso |
|---|---|---|---|---|
| 16 | 6 | 2 | 0 | resposta, lista densa |
| 20 | 6 | 2 | 0 | |
| 24 | 8 | 3 | 0 | pilhas de avatares |
| 32 | 10 | 3 | 0 | DM/lista de membros/amigos |
| **40** | 12 | 4 | 0 | **chat**, barra de servidores |
| 44 / 48 | 12 | 4 | 0 | barra de servidores (44 com experimento), pasta 48/52 |
| 56 | 14 | 4 | 2 | |
| 72 / 80 | 16 | 6 | 4 | cartão de perfil |
| 96 | 20 | 8 | 6 | |
| 120 | 24 | 8 | 8 | perfil |
| 152 | 30 | 10 | 10 | perfil grande |
| (legado) 30 / 60 / 100 | 0 | 0 | 0 | |

O status é desenhado com contorno na cor da superfície (igual ao `avatar()` de `theme.rs`, que usa `size*0.17` externo e `size*0.11` interno; pelos números acima o diâmetro correto é ~`size*0.30` com contorno `size*0.10`).

### 2.7 Medidas transversais [F]

- Alturas de controle: input/item `MD` **40**, `SM` **32**; campo de formulário 44; opção de select 40 (largura máx. 248).
- Modal (mana, CSS 555628.*): largura **400 (sm) / 480 (md) / 680 (lg)**, raio 12, borda 1 px, sombra HIGH, padding horizontal 24 (lg: 32). O módulo JS antigo ainda traz 442/602/800. Fundo (scrim): opacidade animada 0→1 (mola, ver 3.1); o `backdrop` de navegação usa `0→0.9`.
- Menu de contexto (CSS 230322.*): `fixed` 220 de largura, `flexible` mín 188 / máx 320; padding do scroller 8 (vertical = `SCROLLER_BLOCK_PADDING` 8); item: mín 32 de altura, padding 8; ícone 20 + 8; separador 1 px com margem 8; submenu máx 320; borda 1 px; raio 8.
- Tooltip: padding `8 12`, máx **190** (alguns 244), seta 5 px, `z-index 1002`.
- Toast: máx 280 de largura, duração mínima configurável **3-20 s (padrão 3 s)**, animação 300 ms, atraso de entrada na fila 400 ms [F].
- Rolagem: barra "auto" 16 px; "thin" 8 px (thumb raio 4, borda transparente 2); barra de 14 px com margem 3 (padrão da lista de mensagens).
- Reação: pílula `padding 2 6`, borda 1 px transparente, raio 8, margem `0 4 4 0`, emoji + contagem com `margin-left 6`.
- Embed de link: borda esquerda 4 px (`border-normal` ou cor do embed), borda 1 px `border-subtle`, fundo `surface-high`, raio 4, grade com padding `8 16 16 12`, largura máx ~520 (516 no container), mídia raio 4 e `margin-top 16`, título 16 px semibold com cor de link.
- Barra de resposta (espinha): 2 px de borda, raio 6 no canto, cor `spine-default`; faixa de mensagem destacada/mencionada: **2 px** na esquerda (4 px em alto contraste).

### 2.8 Tradução para o Grafite

Princípio: **estrutura do Discord, valores do Grafite** (`theme.rs` + maquete `grafite.html`). A maquete já usa 14.5/1.55 no chat, avatar 32/26/38, easing `cubic-bezier(.2,.7,.2,1)`, foco `outline 2px amber offset 2`.

Escala adotada (constantes novas em `theme.rs`/`tokens.rs`):

```rust
pub mod sp { pub const XXS: f32 = 4.0; pub const XS: f32 = 8.0; pub const SM: f32 = 12.0;
             pub const MD: f32 = 16.0; pub const LG: f32 = 20.0; pub const XL: f32 = 24.0; pub const XXL: f32 = 32.0; }
pub mod rad { pub const XS: u8 = 4; pub const SM: u8 = 8; pub const MD: u8 = 12; pub const LG: u8 = 16; pub const MENTION: u8 = 3; }
pub mod icon { pub const XXS: f32 = 12.0; pub const XS: f32 = 16.0; pub const SM: f32 = 18.0; pub const MD: f32 = 24.0; pub const LG: f32 = 32.0; }
pub mod av { pub const XS: f32 = 16.0; pub const S: f32 = 24.0; pub const M: f32 = 32.0; pub const CHAT: f32 = 40.0;
             pub const L: f32 = 56.0; pub const XL: f32 = 80.0; pub const PROFILE: f32 = 120.0; }
```

Papéis tipográficos (tamanho-base do chat `B = 14.5`, configurável 12-24 em passos; tudo em razão de `B`, como o `rem` do Discord):

| papel | Discord | **Grafite** |
|---|---|---|
| corpo do chat | 1rem/1.375rem, w400 | `B` (14.5) / **22** (`line_height`), w400, `TEXT2` |
| nome do autor | 1rem, w500 | `B` w**600** (o Inter variável aguenta; maquete usa 600), `TEXT` (ou cor do cargo) |
| carimbo | .75rem, w500 | `B*0.80` (11.6) tabular, `MUTED` |
| carimbo de margem | .6875rem | `B*0.74` (10.7) `MUTED`, alinhado à direita em 56 |
| "(editado)" | .625rem | `B*0.69` (10) `MUTED` |
| h1 / h2 / h3 | 1.5 / 1.25 / 1 rem, w700 | `B*1.5` / `*1.25` / `*1.0`, **serifa Newsreader w500-600** nos dois primeiros, `TEXT` |
| subtexto `-#` | .875rem | `B*0.875`, `MUTED` |
| código em linha | 85% | `B*0.85` JetBrains Mono, fundo `L3` + borda `LINE`, raio 4 |
| bloco de código | .875rem/1.125rem | `B*0.9` / 18, JetBrains Mono, fundo `L1`, borda `LINE`, raio 6 |
| rótulo de seção | eyebrow 12 w700 +.02em MAIÚSC. | `caps()` já existente: 11, `extra_letter_spacing 1.0` (≈.09em da maquete) |
| título de página/modal | heading-xl/lg | serifa 24 / 20, `TEXT` |
| tooltip | 14/16 w500 | 12.5 w500 |
| menu | 14/20 w500 | 13.5 (`TextStyle::Button` já é 13.5) |

Estados de superfície (Discord usa sobreposição translúcida 8/12/16/20/24/28%; o Grafite usa passos de camada):

| estado | Grafite |
|---|---|
| repouso | transparente sobre `L1`/`L2` |
| hover | `L4` (#252528) |
| selecionado | `L4` + texto `TEXT` + marca âmbar (barra/pílula) |
| pressionado | `lerp(L4, LINE2, 0.5)` |
| desativado | opacidade 0.5 (Discord: `.disabled{opacity:.5}`) |
| mensagem em hover (modo mouse) | fundo `alpha(WHITE, 0.025)` sobre `L2` (equivale ao `message-background-hover`) |
| mensagem mencionada | fundo `alpha(AMBER, 0.07)` + barra 2 px `AMBER` (Discord: fundo amarelo + barra 2 px; hover sobe o alfa) |
| mensagem respondida/destacada (pulo) | fundo `alpha(AMBER, 0.10)` + barra 2 px `AMBER_HOT`; some em 600 ms |

Sombras: **só** em janelas flutuantes (menu, tooltip, modal, popout): `Shadow { offset:[0,12], blur:24, spread:0, color: black α 90 }` + borda 1 px `LINE2` (já é o `window_shadow` atual com `[0,8]`; subir para 12 como o `HIGH`). Cartões e barras: sem sombra, só filete `LINE`.

Menção / carimbo `<t:>` / spoiler (Grafite, sem azul):

| elemento | repouso | hover |
|---|---|---|
| menção `@user`/`@everyone`/`#canal` | fundo `alpha(AMBER, 0.16)`, texto `AMBER_HOT`, w500 | fundo `AMBER`, texto `AMBER_INK` |
| menção de cargo | fundo `alpha(cor_cargo, 0.12)`, texto `cor_cargo` (se cor 0 → como a de usuário) | fundo `alpha(cor_cargo, 0.25)` |
| carimbo `<t:...>` | fundo `L4`, texto `TEXT2`, raio 3, padding `0 2` | fundo `LINE2` |
| spoiler oculto | fundo `LINE2`, texto = fundo | fundo `lerp(LINE2, L4, .4)` |
| spoiler revelado | fundo `alpha(WHITE, 0.06)`, texto normal | — |
| link | `AMBER` sem sublinhado (sublinha no hover; se "mostrar sublinhado em links" ligado, sempre) | sublinhado |

(Link = âmbar porque `v.hyperlink_color` já é `AMBER`. Menção e link terem o mesmo âmbar é aceitável porque a menção tem fundo; se confundir, trocar o link para `TEXT` sublinhado, ver Perguntas.)

---

## 3. Estados e interações

### 3.1 Movimento: tudo que o cliente anima [F]

**Regra geral de reduzir movimento [F]**: a raiz recebe `full-motion` ou `reduce-motion` (nunca as duas). `reduce-motion` vale se a preferência é `reduce`, ou `auto` com `prefers-reduced-motion: reduce` do sistema; opções `auto | reduce | no-preference` [F AccessibilityStore @2651300]. Todo CSS de **geometria** (`transform`, `height`, `width`, `border-radius`, `drop-shadow`) está escrito como `.full-motion .alvo { transition... }`, logo sem a classe a mudança é instantânea. Transições **só de cor/opacidade** continuam. As molas (react-spring) consultam `reducedMotion.enabled`: escala fica em 1, translação some, resta o fade [F `280336`, `150591`, `273875`].

Hover/foco só aparecem em `mouse-mode`; em `keyboard-mode` (ligado ao usar Tab/setas, desligado ao mexer o mouse) valem só os estilos de foco: `.mouse-mode .message:hover`, `.mouse-mode .message:hover .buttons`, `.mouse-mode .timestamp {pointer-events:auto}` [F].

**Padrão assimétrico de hover (importante)** [F, em `toggleButton`, `pillButton`, `guildTagPill`, `selectButtonPill`]: o elemento base declara `transition: background 50ms ease-in` e o estado hover declara `transition: background .15s ease-out`. Como vale a transição do estado de destino, **entrar em hover leva 150 ms (ease-out) e sair leva 50 ms (ease-in)**.

Tabela de CSS (durações/easings mais frequentes no corpus; contagem de regras únicas):

| duração / easing | onde | n |
|---|---|---|
| `opacity .15s ease-out` | aparecer botões de hover, overlays de edição | 26 |
| `opacity .2s ease-in-out`, `.2s ease` | fades gerais | 43 |
| `background-color .2s ease` | linhas clicáveis, `spoilerMarkdownContent` (revelar), botão de conta | 20 |
| `opacity 50ms ease-in`, `background 50ms ease-in` | saída de hover | 28 |
| `background-color .15s ease-out` | entrada de hover (pílulas, itens) | 13 |
| `background-color .1s ease(-in-out)` | item de lista (`.full-motion .item`), ícone de servidor (`.1s ease-out`), reação (`.1s`) | 20 |
| `transform .2s ease-out/ease` | pequenos deslocamentos | 27 |
| `height/opacity/transform .2s ease-out` | linhas que entram/saem de uma lista (ex.: amigos) | 2 |
| `height .2s ease-in-out` / `.1s` | painéis (voz) `.full-motion .panel` | 3 |
| `transform .4s cubic-bezier(.16,1,.3,1)` | picker de emoji/figurinha abrindo (expo-out) | 4 |
| `transform/opacity .25s cubic-bezier(.22,1,.36,1)` (fecha em .15s) | cartões de onboarding/clarificação, escala .96→1 | 5 |
| `backdrop-fade-in .3s cubic-bezier(.2,0,0,1)`, saída `.29-.38s cubic-bezier(.3,0,0,1)` | scrim de navegação | 2 |
| `width .22s ease` | popout | 1 |
| `transform .26s cubic-bezier(.2,0,0,1)` | barra de ações de resposta | 2 |
| `border-radius .3s ease-out` | perfil animando | 2 |

Casos específicos:

| evento | valor | fonte |
|---|---|---|
| menção interativa (hover) | `background-color 50ms ease-out, color 50ms ease-out`; hover = marca sólida + texto branco | CSS 971156.* |
| espinha de resposta clicável | `border 50ms`, hover = cor de texto interativo | 738015.* |
| avatar do chat | hover: sombra; `:active` `translateY(1px)` (também na decoração) | 738015.* |
| mensagem destacada ao pular (flash) | entra `.2s`, sai `.6s`; no flash também `flashPulse .6s ease-in-out` (opacidade 1→0→1→0→1 em 0/20/40/60/80%) | 446860.* |
| mensagem: fundo no hover | sem transição declarada (instantâneo) [I: não achei `transition` na regra] | 524434.* |
| caixa de texto | `border-color .2s ease`, `box-shadow .2s ease` | |
| "digitando…" | keyframes `typing-dot-pulse`: 0/50/100% opacidade .32 escala .8; 25% opacidade 1 escala 1 (3 pontos defasados); duração [?] | |
| esqueleto | `placeholderPulse 3s ease-in-out infinite` (fundo `base-lower`↔`base-low`) | |
| spinner | `pulsingEllipsis 1.4s`; **variante de baixo movimento** `spinner-low-motion 1.4s` (só opacidade 1↔0.6) | |
| toast | 300 ms (`ANIMATION_DURATION_MS`), `toast-slide-in/out` + fade, espera de 400 ms na fila | `Modules.toast` |
| upload: "shake" de erro | `.82s cubic-bezier(.36,.07,.19,.97)` (só `.full-motion`) | |
| botão de ícone no hover (voz) | animações Lottie de 70 quadros (mute/deafen) [irrelevante p/ egui] | |
| mensagem nova chegando | **não achei keyframe nem transição** de entrada [I: não anima] | |

**Molas (react-spring `{mass 1, tension, friction}`)** [F]. Conversão: ω0 = √tension, ζ = friction / (2·ω0); tempo de assentamento (2%) ≈ 4 / (ζ·ω0).

| uso | tension / friction | ζ | assenta em | efeito | fonte |
|---|---|---|---|---|---|
| **tooltip** (entrar e sair) | 2400 / 52 | 0.53 | ~150 ms | `scale .95→1`, `opacity 0→1`; reduzido: só opacidade | `280336`, `900002` |
| **modal** (entrar) | 1000 / 48, com `delay 64 ms` | 0.76 | ~170 ms | `scale .9→1`, `opacity 0→1` | `150591 kS` |
| **modal** (sair) | 1200 / 80 | 1.15 (sobreamortecida) | ~200 ms | volta a `scale .9`, `opacity 0` | idem |
| **scrim** do modal | 1000 / 48 | 0.76 | ~170 ms | `opacity 0→1` (`animate-never` se `animationVariant=instant`) | `462824` |
| **popout / popover** (entrar) | 800 / 32 | 0.57 | ~250 ms (overshoot ~11%) | `opacity` + `translate3d(±10px→0)` do lado do alvo | `555115`, `273875` |
| popout (sair) | 1200 / 48 | 0.69 | ~170 ms | inverso | idem |
| indicador de status do avatar | 450 / 20 (`clamp`) e 1200 / 70 | | | troca de forma do status | `97808` |
| deslizar de carrossel | 300 / 28 (`clamp`) | 0.81 | ~280 ms | | `430690` |
| lista de canais (entrada) | 500 / 18 (`clamp`) | 0.40 | ~360 ms | | |
| recolher cabeçalho/aviso | 120 / 12 e 120 / 14 | ~0.55 | ~600 ms | `translateY(-100%)`/`120%` | |
| outras ~25 molas | de 20/10 a 2400/60 | | | contexto não identificado | |

Obs.: nas quatro linhas de indicador de status, carrossel, lista de canais e recolher cabeçalho, o **uso** é [I] (o valor da mola é [F]); modal, scrim, tooltip e popout são [F] de ponta a ponta.

Atrasos de tooltip [F]: o componente padrão usa `delay = r ?? 0` (**0 ms**; quem chama passa o atraso). Valores vistos: rich tooltip padrão **300 ms**, tooltip de som/soundboard **500 ms**, tooltip de emoji **150 ms**, barra de controle de vídeo **1500 ms**. Navegação por teclado abre o tooltip sem esperar.

### 3.2 Foco, seleção e outros estados

- **Anel de foco** [F]: só aparece em `keyboard-mode`. Dois mecanismos: (a) `FocusRing` (div `focus-rings-ring`, `box-shadow 0 0 0 4px var(--focus-primary,#00b0f4)`, raio 4 ou o do alvo, deslocável por `offset {top,bottom,left,right}`; ex. itens de lista de membros usam `offset 4`, botões laterais `-2/6`); (b) o desenho novo `outline: 2px solid var(--border-focus)` com `outline-offset: -2px` (itens de menu, listbox) ou `2px` (inputs avulsos). Em modo de cores forçadas o anel vira `CanvasText` com borda `Canvas`.
- **Item de menu focado**: fundo `interactive-background-hover`, raio 4, texto `text-strong`, ícone/caret branco; perigoso: fundo `feedback-mod-critical` + texto vermelho; `:active` escurece.
- **Desativado**: `opacity .5` e `pointer-events:none` (menu, botão). **Carregando**: botão com spinner/`loading`; modal de confirmação desativa o botão e mostra spinner até o `await` terminar [F `732159`].
- **Link**: sem sublinhado até o hover, exceto com `decorate-links` (configuração "mostrar sublinhado em links"; liga sozinha quando saturação ≤ 0.4) [F].
- **Seleção de texto num spoiler oculto**: a seleção fica transparente (não revela nem vaza o texto) [F `::selection{color:transparent}`].
- **Estados de mensagem**: `mentioned` (barra 2 px aviso), `replying`/`highlighted` (barra 2 px marca), `ephemeral` (barra marca), `isSending` (`opacity .5`), `isUnsupported` (muted) [F].

---

## 4. Regras de comportamento

### 4.1 Markdown: gramática, ordem e limites [F @6276800-6300500, 3 regexes de 6305000]

**Armazenamento**: o servidor guarda o texto cru. Tokens estruturais no texto: `<@id>` (também `<@!id>`), `<@&id>` cargo, `<#id>` canal, `<:nome:id>` e `<a:nome:id>` emoji do servidor, `<t:unix[:estilo]>`, `</comando:id>`, `<id:guide|browse|customize|home|linked-roles[:id]>`. `@everyone`, `@here` e `@silent` (só no início da mensagem) são texto puro com regex. O composer troca "@nome" por `<@id>` ao enviar.

**Motor**: simple-markdown com regras do Discord. As regras de texto (menções, spoiler, emoji...) usam a ordem da regra `text`; a regra `text` é "anyScope" e **para antes de qualquer caractere não alfanumérico** (`^[\s\S]+?(?=[^0-9A-Za-z\sÀ-￿]|\n\n| {2,}\n|\w+:\S|[0-9]+\.|$)`), por isso os especiais têm sua chance de casar.

Ordem de tentativa (menor primeiro; os números absolutos do simple-markdown são [I], as relações abaixo são [F] pelos `order:` do código):

1. `heading`, `subtext`, `list` (mesma ordem do heading; só em início de linha) 
2. `codeBlock` e `customEmoji` (mesma ordem)
3. `blockQuote`, `newline`, `paragraph`, `escape`
4. `channelOrMessageUrl` e `mediaPostLink` (ordem do `url` − 0.5, antes de `url`), `autolink`, `mailto`, `tel`, `url`, `link` (mascarado)
5. `em`, `strong`, `u` (`__`), `s` (`~~`, ordem do `u`)
6. `inlineCode`, `br`
7. `timestamp` (ordem do `text` − 1, ganha da `text`)
8. mesma ordem de `text`: `mention`, `roleMention`, `channelMention`, `commandMention`, `spoiler`, `emoji` (`:nome:`), `soundboard`, `staticRouteLink`, `silentPrefix`, `emoticon` (`¯\_(ツ)_/¯` literal, para o `_` não virar itálico), `text`.

Regexes (reescritas por mim; origem no cliente):

| elemento | sintaxe | regex / regra | observações |
|---|---|---|---|
| negrito | `**x**` | `^\*\*(...)\*\*(?!\*)` | peso 700 |
| itálico | `*x*` ou `_x_` | `*` com `(?=\S)`; `_` com `\b` nas pontas | inclinação sintética |
| sublinhado | `__x__` | `^__(...)__(?!_)` | **não é negrito** |
| tachado | `~~x~~` | `^~~([\s\S]+?)~~(?!_)` | |
| spoiler | `\|\|x\|\|` | `^\|\|([\s\S]+?)\|\|` | aceita inline e dentro de lista/título/citação |
| código em linha | `` `x` `` / ``` ``x`` ``` | crase(s) simétricas, conteúdo literal | fundo `background-code`, borda 1 px |
| bloco de código | ```` ```lang\ncódigo``` ```` | ```^```(?:([a-z0-9_+\-.#]+?)\n)?\n*([^\n][^]*?)\n*```` ```` (i) | a linguagem só vale se vier seguida de `\n`; quebras de linha nas pontas são cortadas; conteúdo não é interpretado |
| citação | `> x` | `^( *>>> +([\s\S]*))|^( *>(?!>>) +[^\n]*(\n *>(?!>>) +[^\n]*)*\n?)` | **exige espaço depois de `>`**; `>>> ` consome o resto da mensagem; sem aninhamento (`inQuote`/`nested` → não casa); só em início de linha |
| título | `# `, `## `, `### ` | `^ *(#{1,3})(?:\s+)((?!\s*#{1,3}\s)[^\n]+?)#*\s*(?:\n\|$)` | **só níveis 1-3**; só início de linha; não vale dentro de lista |
| subtexto | `-# x` | `^ *-# +((?!-#)[^\n]+)(?:\n\|$)` | só início de linha; não recursivo (`allowSubtext:false` por dentro) |
| lista | `- x`, `* x`, `1. x` | marcador `[*-]` ou `\d+\.` + espaço; indentação por recuo | profundidade máx. **10** (`_listLevel >= 11` → não casa); início numerado limitado a `[1, 1e9]`; título não vale dentro de item; cada item vira `li` com vírgula invisível para leitor de tela |
| link mascarado | `[texto](url)` | exige `](`; recusa mais de 10 `[` aninhados, mais de 200 parênteses abertos, mais de 100 caracteres seguidos dentro de 2 níveis | protocolos permitidos: `http, https, discord, tel, sms, mailto`; recusa URL com caracteres de controle/invisíveis; o texto do link só pode ter nós `text/u/strong/em/s/inlineCode/spoiler/br/timestamp(+emoji)`; link mascarado aninhado vira texto cru |
| URL solta | `https://...` (também `steam://`) | `^((?:https?\|steam):\/\/[^\s<]+[^<.,:;"'\]\s])` | só em contexto inline; tira `)` finais sem par; `<url>` entre ângulos vira autolink sem prévia [I] |
| escape | `\*` | `\` + qualquer não alfanumérico | desligável por contexto |
| usuário | `<@id>` | `^<@!?(\d+)>` ou `^(@(?:everyone\|here))` | texto exibido = apelido no servidor, senão nome; desconhecido → `@usuário-desconhecido` |
| cargo | `<@&id>` | `^<@&(\d+)>` | desconhecido → `@cargo desconhecido`; cor do cargo (respeita "estilo do cargo" e saturação) |
| canal | `<#id>` | `^<#(\d+)>` | ícone do tipo + nome; outro servidor mostra `Servidor › #canal`; sem permissão → "Sem acesso" com cadeado; inexistente → `#canal desconhecido`; link de mensagem vira "canal › mensagem" |
| comando | `</nome sub:id>` | nome 1-32 em até 3 partes | cor de link, w500 |
| emoji do servidor | `<:nome:id>` / `<a:nome:id>` | `^<(a)?:(\w+):(\d+)>` | tooltip `:nome:`; animado só com autoplay |
| emoji padrão | `:nome:` ou caractere Unicode | reconhece shortcodes e substitui; caracteres Unicode viram imagem | no egui: texto (ver 6.3) |
| timestamp | `<t:1700000000:R>` | `^<t:(-?\d{1,17})(?::(t\|T\|d\|D\|f\|F\|s\|S\|R))?>` | estilo padrão **f** |

Estilos de `<t:>` (moment, pt-BR) [F]: `t` hora curta ("14:03"), `T` com segundos, `d` data curta ("07/11/2026"), `D` data longa ("7 de novembro de 2026"), `f` data longa + hora (padrão), `F` com dia da semana ("sábado, 7 de novembro de 2026 14:03"), `s` "07/11/2026 14:03", `S` com segundos, `R` relativo ("há 3 minutos", "em 2 horas"). O tooltip do carimbo mostra sempre o `F`. O `R` se atualiza por *timer*: |diferença| < 2 min → a cada **1 s**; < 5 min → 60 s; < 45 min → 120 s; < 21 h → 300 s; senão 2 h. Carimbo inválido vira texto cru. Visual: fundo `background-mod-normal`, raio 3, `padding 0 2`.

Limites e regras de contexto:
- Mensagem: **2000** caracteres (Nitro **4000**, `premiumMaxMessageLength`) [F]. O Telacast já impõe 1-4000 no servidor (`CHECK` + `main.rs:611`): manter 4000 para todos.
- **Emoji jumbo**: se a mensagem só contém emoji/figurinha de som e espaços e o total é **≤ 30** (`MAX_EMOJI_TO_BE_JUMBO`), cada um fica 3rem (48 px); no modo compacto fica 1.375em [F].
- Nome de emoji ≤ 32 caracteres, 50 emojis por servidor base, +200 de slots extras (`EMOJI_MAX_LENGTH 32`; `50`; `200`), arquivo ≤ 256 KB (`l=256`), URL base de 48 px [F].
- **Conjuntos de regras por contexto** [F `RULES`, `CHANNEL_TOPIC_RULES`...]: tópico de canal sem `codeBlock/blockQuote/subtext/br`; título de embed sem blocos; resposta (prévia de 1 linha) sem `blockQuote/codeBlock/br`, mantém `inlineCode`; status de canal de voz sem `url/blockQuote/mention`; bio de perfil sem menções e sem parágrafo; evento sem código; AutoMod com destaque de palavra. Cada rótulo de UI deve escolher seu conjunto (`RuleSet::Message | Topic | ReplyPreview | Embed | Bio`).
- Espaço em branco: `white-space: break-spaces` (preserva espaços e quebras), `word-wrap: break-word`. `.messageContent:empty` some.
- Mensagem que só contém `||...||` e anexo: o anexo também pode ser spoiler pelo prefixo `SPOILER_` (`MO`) [F].
- Realce de sintaxe: classes `hljs-*` (highlight.js): `keyword`, `type/built_in`, `title/function/class`, `variable/literal/params`, `attr/attribute`, `meta` (decorador), `property`, `number`, `operator`, `string`, `regexp`, `symbol`, `comment`. Sem linguagem informada, **[?]** (não vi detecção automática). Também existe `ansi` (sequências `ESC[...m`, 16 cores `--ansi-*`).

Interações de mídia/links (já implementadas pelo cliente):
- **Spoiler** (CSS 880150.*): oculto = conteúdo `opacity 0`, cursor pointer, clique revela (tooltip/aria "Clique para revelar o spoiler"), fundo `.2s ease`; hover escurece/clareia o fundo; blocos (citação, código, lista, título) pintam o retângulo todo; texto selecionado não vaza.
- **Link mascarado**: clicar abre o URL; se o domínio do texto e do destino diferirem há aviso de saída [I]. Clique em canal/menção/comando navega (mantém a janela); botão direito abre o menu da entidade.
- **Prévia de link** (embed): imagem/vídeo inline até 520 px, ver 2.7; criada pelo servidor (*unfurl*), não pelo cliente [F pelas rotas e pelo tipo `embed`].

### 4.2 Atalhos de teclado [F @2327000-2362500]

`mod` = Ctrl (Windows/Linux), Cmd (macOS). A lista abaixo é a tabela real de binds + o modal "Atalhos" (Ctrl+/).

| grupo | ação | bind | pt-BR do cliente |
|---|---|---|---|
| Navegação | servidor anterior/próximo | `Ctrl+Alt+↑/↓`, `Ctrl+Shift+[` / `]`, `Ctrl+Shift+Tab` / `Ctrl+Tab` | Navegar entre servidores |
| | canal anterior/próximo | `Alt+↑` / `Alt+↓` | Navegar entre canais |
| | voltar/avançar no histórico | `Alt+←` / `Alt+→` (macOS `Cmd+[` / `]`) | Voltar ou avançar no histórico de páginas |
| | canal não lido ant./próx. | `Alt+Shift+↑` / `↓` | Navegar entre canais não lidos |
| | canal com menção ant./próx. | `Ctrl+Shift+Alt+↑` / `↓` | Navegar entre canais com menções não lidas |
| | alternar último servidor/DMs | `Ctrl+Alt+→` | Alterne entre o último servidor e mensagens diretas |
| | ir para a chamada atual | `Ctrl+Shift+Alt+V` | Navegar para ligação atual |
| | voltar ao canal de áudio | `Ctrl+Alt+←` | [?] sem rótulo na lista |
| | servidor 1-9 | `Ctrl+1` ... `Ctrl+9` (1 = DMs/Home) | [?] fora da lista de ajuda |
| | QuickSwitcher | `Ctrl+K` (e `Ctrl+T` quando abas desligadas) | Ativar/desativar QuickSwitcher |
| | criar ou entrar em servidor | `Ctrl+Shift+N` | Criar ou entrar em um servidor |
| | abas de canal (recurso opcional) | abrir `Ctrl+T`, fechar `Ctrl+W`, fixar `Ctrl+Shift+P` | |
| Chat | marcar servidor como lido | `Shift+Esc` | Marcar servidor como lido |
| | marcar canal como lido | `Esc` (depois de fechar camadas, edição, resposta) | Marcar canal como lido |
| | grupo privado | `Ctrl+Shift+T` | Criar um grupo privado |
| | criar tópico | `Ctrl+Shift+Alt+N` | Criar um tópico |
| | fixações / caixa de entrada | `Ctrl+P` / `Ctrl+I` | Abrir/fechar mural de fixações / caixa de entrada |
| | marcar caixa de entrada lida | `Ctrl+Shift+E` | |
| | lista de membros | `Ctrl+U` | Ativar/Desativar lista de membros |
| | emoji / GIF / figurinha | `Ctrl+E` / `Ctrl+G` / `Ctrl+S` | seletores |
| | rolar | `PageUp` / `PageDown` | Rolar a conversa |
| | ir à mais antiga não lida | `Shift+PageUp` | Avançar para a mensagem não lida mais antiga |
| | focar o texto | qualquer caractere imprimível (fora de campo) ou `Tab`; `Esc` em mensagem focada | Focalizar área de texto |
| | enviar arquivo | `Ctrl+Shift+U` | Enviar um arquivo |
| | copiar link do canal | `Ctrl+Shift+L` | Copiar link do canal |
| | gravar mensagem de voz | [?] bind não localizado | Gravar mensagem de voz |
| | buscar | `Ctrl+F` / `Ctrl+Shift+F` | Buscar |
| | recolher/expandir categorias | `Ctrl+Shift+A` | |
| | editar a última mensagem | `↑` com campo vazio [I; a ação `EDIT_LAST_MESSAGE` existe] | |
| Voz | mutar / ensurdecer | `Ctrl+Shift+M` / `Ctrl+Shift+D` | Ativar/desativar microfone / áudio |
| | atender / recusar chamada | `Ctrl+Enter` / `Esc` ou `Shift+PageDown` | Atender / Recusar chamada |
| | iniciar chamada | `Ctrl+'` e `Ctrl+Shift+'` (a diferença voz/vídeo é [I]) | Iniciar chamada |
| | painel de som | `Ctrl+Shift+B` | Alternar painel de som |
| Diversos | lista de atalhos | `Ctrl+/` ou `Ctrl+Shift+/` | |
| | ajuda | `Ctrl+Shift+H` ou `F1` | Obter ajuda |
| | menu de contexto pelo teclado | `Shift+F10` | Abrir menu contextual |
| | zoom | `Ctrl+=` ou `Ctrl++` / `Ctrl+-` / `Ctrl+0` | |
| Mover (DnD) | iniciar / mover / soltar / cancelar | `Ctrl+D` / `↑ ↓` / `Espaço ou Enter` / `Esc` | Começar Arraste e Solte... |
| Mensagem focada | editar / apagar / fixar / reagir / responder / encaminhar / falar / copiar / marcar não lida | `E` / `Backspace` / `P` / `+` / `R` / `F` / `S` / `Ctrl+C` / `Alt+Enter` | |
| Modais | confirmar | `Enter` (só se não estiver em modo teclado) | |

Regras: um bind "global" do Discord é `comboKeysBindGlobal`; é ignorado se há camada/modal aberto para os que abrem seletores (`getLayers().length > 0`). Teclas simples só valem com o foco fora de campos de texto. `Esc` é uma cascata (ver 6.5).

### 4.3 Acessibilidade (o que o cliente faz) [F]

Configurações (Acessibilidade): tamanho da fonte do chat (7 valores), zoom 50-200 %, espaço entre grupos, **movimento reduzido** (`Auto | Ligado | Desligado`; "Reduza a quantidade e a intensidade de animações, efeitos de cursor e outros efeitos de movimento"), saturação de cores 0-100 % (passo 5), contraste, **alto contraste** (sincroniza com o do sistema), modo para daltônicos, "aplicar saturação às cores personalizadas", "mostrar sublinhado em links", estilo da cor de cargo (`username` etc.), duração mínima dos avisos (3-20 s), autoplay de GIF e de emoji animado, figurinhas animadas (sobrescritas por movimento reduzido), balão de leitura TTS.

Semântica: listas com `role="list"` e navegação por setas (*roving tabindex*, `aria-orientation`); canais como `treeitem` com `aria-level`; modais `role="dialog"`/`alertdialog` com `aria-labelledby` do título e **retorno de foco** ao elemento de origem (`returnRef`) e prisão de foco; avisos `role="status"`/`role="alert"`; `aria-live="polite"` para texto dinâmico; `aria-posinset/setsize` em itens de lista; títulos e itens de lista emitem uma "vírgula" invisível (*screen-reader-pause*) para o leitor fazer pausa; rótulo da lista de mensagens "Mensagens em {canal}"; modais com "Fechar janela pop-up".
Cores forçadas (`enable-forced-colors`): menu/botões/spoiler usam cores do sistema (`ButtonFace`, `CanvasText`, `Highlight`), menções e links ganham sublinhado e bordas.

---

## 5. Dados necessários (servidor e protocolo)

Prioridade para a UI de texto rico (nada disso é obrigatório para o MVP do markdown, que só usa `content`):

1. **P0 (cliente apenas)**: nenhuma mudança de servidor; o markdown e os `<@id>` são resolvidos no cliente com o que já chega (membros, canais).
2. **P0**: resolver `<@id>` precisa de **mapa id→(nome, apelido, cor do cargo)** de todos os autores e mencionados. Hoje `S2C` entrega membros do servidor aberto; para DMs/usuários fora da lista, enviar `users: Vec<UserRef>` junto de `History`/`Message`, ou criar `C2S::ResolveUsers { ids }` -> `S2C::Users`.
3. **P1**: tabela `message_mentions(message_id, user_id)` + coluna `mention_everyone bool` preenchidas no `Send` (parse no servidor) para contador de menções e destaque da linha (outra área usa). Evita reparsear no cliente para cada mensagem não vista.
4. **P1**: cargos (`roles`: id, nome, cor, posição) e `member_roles` para `<@&id>` e cor do nome (área "servidores e cargos").
5. **P2**: emoji do servidor: `emojis(id, guild_id, name, animated, mime, hash)` + `GET /emojis/{id}.{png|gif}?size=48|96`; `C2S::UploadEmoji`; `S2C::EmojiList { server }`.
6. **P2**: prévia de link: `message_embeds(message_id, url, title, description, image_url, site, color)` + *unfurl* no servidor (buscar `og:*`, limite de 1 MB e 3 s, bloquear IP privada para evitar SSRF); evento `S2C::MessageEmbeds`.
7. **P2**: preferências do usuário (`user_settings`: `chat_font_px`, `zoom`, `group_spacing`, `compact`, `reduce_motion`, `link_underline`, `saturation`, `high_contrast`, `keybinds` JSON). Primeiro local (`eframe::Storage`, a feature `persistence` já está ligada); depois sincronizado.
8. **P3**: editado: `edited_at` já existe; falta mostrar "(editado)" e tooltip com data.

---

## 6. Plano de implementação em egui (0.37)

### 6.1 Armadilhas e capacidades reais (verificadas no código do crate)

| tema | situação em 0.37 |
|---|---|
| Negrito/semi-negrito | **Existe**: `TextFormat.coords = VariationCoords::new([(b"wght", 700.0)])` (`egui::epaint::text::VariationCoords`; **não** reexportado em `egui::text`). `Inter.ttf`, `JetBrainsMono.ttf`, `Newsreader.ttf` são variáveis (eixo `wght`). Alternativa: `FontData::tweak(FontTweak { coords, .. })` e uma `FontFamily::Name("inter-600")` por peso. `RichText::strong()` **não** usa o eixo (só muda cor) |
| Itálico | `TextFormat.italics` = inclinação sintética (text_layout.rs:1304); aceitável |
| Letter-spacing | `TextFormat.extra_letter_spacing`, `RichText::extra_letter_spacing` |
| Altura de linha | `TextFormat.line_height: Option<f32>`; centraliza o texto na linha |
| Fundo do trecho | `TextFormat.background` é retângulo reto por trecho; **sem raio**. Para pílulas (menção, spoiler, carimbo) pintar os retângulos você mesmo (6.3) |
| Imagem no meio do texto | **não há**. Reservar espaço com caractere-sentinela e pintar a textura por cima (6.3) |
| Emoji colorido | `epaint` precisa da feature `color_fonts` (desligada); adicionar `epaint = { version = "0.37", features = ["color_fonts"] }` ao `Cargo.toml` (unificação de features liga no `egui`) e registrar `NotoColorEmoji.ttf` como fonte de fallback. O do sistema é **CBDT (bitmap, 11 MB)**; empacotar um COLRv1 (≈1-2 MB) é melhor. Atenção: `egui_system_font_provider` pula fontes de emoji. **[?] não testado aqui** (regra: sem build) |
| Quebra de palavra longa | usar `wrap.break_anywhere = false`; palavras maiores que a largura são quebradas pelo layout [I] |
| Tabular nums | sem `font-variant-numeric`; Inter variável tem `tnum` mas o egui não expõe *features* OpenType. Para carimbos use JetBrainsMono ou largura fixa manual |
| Hover/animação | `animate_bool_with_time_and_easing` aceita `fn(f32)->f32` **sem captura** e usa a **mesma** duração nos dois sentidos |
| `Style::animation_time` | 0.2 s padrão; `0.0` desliga `animate_bool*` (útil para reduzir movimento) |
| Tooltip | `Style.interaction.tooltip_delay = 0.5`, `tooltip_grace_time = 0.2`, `show_tooltips_only_when_still = true` |
| Zoom | `Options.zoom_with_keyboard = true` já trata `Ctrl +/-/0` em passos de 0.1 |
| Seleção de texto | `Label` selecionável não atravessa mensagens; copiar por mensagem (menu) |
| AccessKit | `eframe` tem `accesskit` nas features padrão e o app usa as padrão; Linux via `accesskit_unix` (AT-SPI) |

### 6.2 Módulos novos

```
app/src/
  tokens.rs     // sp/rad/icon/av/ type scale (2.8) + Density + ChatPrefs (font px, group spacing, compact)
  motion.rs     // spring(), hover_t(), easings, Motion { reduce } (6.4)
  md/mod.rs     // Ast, parse(), RuleSet
  md/parse.rs   // 6.3
  md/render.rs  // Ast -> Vec<Block>, LayoutJob, hit-test
  md/time.rs    // <t:...> em pt-BR, sem chrono-locales
  shortcuts.rs  // Action, tabela de binds, dispatcher (6.5)
  a11y.rs       // helpers de rótulo / região viva / foco (6.6)
```
`theme.rs` ganha só as constantes de cor novas (`MENTION_BG`, `SPOILER_BG`) e os `FontFamily` de peso, se a opção por família for escolhida.

### 6.3 Markdown no egui: parser + LayoutJob

**Parser**: escrever à mão (sem regex pesada nem crates novos), em duas passagens.
1. Blocos (início de linha): `heading`, `subtext`, `list`, `blockQuote`, `codeBlock` (procurar ```` ``` ```` com a regex acima feita manualmente), o resto é `paragraph`.
2. Inline (recursivo com `depth` ≤ 10): varrer o texto; em cada posição testar, **nesta ordem**: escape `\`, `<` (customEmoji, mention, roleMention, channelMention, timestamp, commandMention), `||`, `` ` ``, `**`, `__`, `*`/`_`, `~~`, `[`...`](`, `http(s)://`, `@everyone|@here`, `:nome:`, texto. Cada regra devolve `(nó, bytes_consumidos)`; se não casar, o caractere vira texto.

```rust
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>), Em(Vec<Inline>), Under(Vec<Inline>), Strike(Vec<Inline>),
    Code(String),
    Spoiler { id: u32, kids: Vec<Inline> },          // id = índice dentro da mensagem
    Link { kids: Vec<Inline>, url: String },          // url cru já validado (esquemas da lista)
    User(i64), Role(i64), Channel(i64), Everyone, Here, Command(String),
    Time { unix: i64, style: char },
    Emoji(String),                                    // caractere Unicode ou :shortcode:
    Custom { id: i64, name: String, animated: bool },
}
pub enum Block { Paragraph(Vec<Inline>), Heading(u8, Vec<Inline>), Subtext(Vec<Inline>),
                 Quote(Vec<Block>), List { start: Option<u32>, items: Vec<Vec<Inline>> },
                 Code { lang: String, text: String } }
```

**Render**: cada `Block` vira um widget. `Paragraph/Heading/Subtext/List item` = um `LayoutJob` (um galley) por bloco; `Quote` = barra 4 px (`LINE2`, raio 2) + conteúdo recuado 12 (largura máx. 90% como no Discord); `Code` = `Frame` próprio (6.2) com `ScrollArea::horizontal`.

```rust
use egui::{epaint::text::VariationCoords, text::{LayoutJob, TextFormat}, FontFamily, FontId, Color32, Stroke, Align};

struct Ctx<'a> { b: f32, r: &'a Resolver, revealed: &'a HashSet<(i64,u32)>, msg: i64, now: i64 }
#[derive(Clone)]
struct Style { w: f32, ital: bool, under: bool, strike: bool, fam: FontFamily, scale: f32, color: Color32 }

fn fmt(c: &Ctx, s: &Style, bg: Color32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(c.b * s.scale, s.fam.clone()),
        color: s.color,
        background: bg,
        coords: VariationCoords::new([(b"wght", s.w)]),
        italics: s.ital,
        underline: if s.under { Stroke::new(1.0, s.color) } else { Stroke::NONE },
        strikethrough: if s.strike { Stroke::new(1.0, s.color) } else { Stroke::NONE },
        line_height: Some((c.b * 22.0 / 14.5 * s.scale).round()),
        valign: Align::BOTTOM,
        ..Default::default()
    }
}

/// Anexa os inlines ao job e registra os intervalos clicáveis/pintáveis (em *bytes* do job).
fn push(job: &mut LayoutJob, spans: &mut Vec<Span>, c: &Ctx, st: &Style, nodes: &[Inline]) {
    for n in nodes { match n {
        Inline::Text(t) => job.append(t, 0.0, fmt(c, st, Color32::TRANSPARENT)),
        Inline::Strong(k) => push(job, spans, c, &Style { w: 700.0, ..st.clone() }, k),
        Inline::Em(k)     => push(job, spans, c, &Style { ital: true, ..st.clone() }, k),
        Inline::Under(k)  => push(job, spans, c, &Style { under: true, ..st.clone() }, k),
        Inline::Strike(k) => push(job, spans, c, &Style { strike: true, ..st.clone() }, k),
        Inline::Code(t) => { let a = job.text.len();
            let mut f = fmt(c, &Style { fam: FontFamily::Monospace, scale: 0.85, ..st.clone() }, Color32::TRANSPARENT);
            job.append(t, 0.0, f); spans.push(Span::pill(a, job.text.len(), Pill::Code)); }
        Inline::User(id) => { let a = job.text.len();
            let name = c.r.user_name(*id).unwrap_or_else(|| "usuário-desconhecido".into());
            let s = Style { w: 500.0, color: crate::theme::AMBER_HOT, ..st.clone() };
            job.append(&format!("@{name}"), 0.0, fmt(c, &s, Color32::TRANSPARENT));
            spans.push(Span::pill(a, job.text.len(), Pill::Mention(Target::User(*id)))); }
        Inline::Spoiler { id, kids } => { let a = job.text.len();
            let open = c.revealed.contains(&(c.msg, *id));
            let s = if open { st.clone() } else { Style { color: Color32::TRANSPARENT, ..st.clone() } };
            push(job, spans, c, &s, kids);
            spans.push(Span::pill(a, job.text.len(), Pill::Spoiler { id: *id, open })); }
        Inline::Custom { .. } | Inline::Emoji(_) => { /* ver abaixo: sentinela / texto */ }
        _ => {}
    } }
}
```
(`Span::pill` guarda `byte_start..byte_end`; as cores de fundo **não** vão em `TextFormat.background` porque não têm raio.)

**Pintar pílulas com raio e fazer hit-test** depois de obter o galley:

```rust
let galley = ui.painter().layout_job(job);
let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
for sp in &spans {
    // converte bytes -> índice de char (CCursor usa chars): job.text[..b].chars().count()
    let (a, b) = (char_idx(&job.text, sp.start), char_idx(&job.text, sp.end));
    let ra = galley.pos_from_cursor(egui::text::CCursor::new(a));
    let rb = galley.pos_from_cursor(egui::text::CCursor::new(b));
    // uma pílula que quebra de linha precisa de um retângulo por linha: iterar galley.rows
    // e interseccionar [a,b) com cada row (row.char_count_excluding_newline() acumulado).
    let r = Rect::from_min_max(rect.min + ra.min.to_vec2(), rect.min + rb.max.to_vec2());
    ui.painter().rect_filled(r.expand2(vec2(2.0, 0.0)), sp.radius(), sp.bg(hovered));
}
ui.painter().galley(rect.min, galley.clone(), TEXT2);
if let Some(pos) = resp.interact_pointer_pos().or(resp.hover_pos()) {
    let cur = galley.cursor_from_pos(pos - rect.min).index;  // char index
    // procurar o span que contém `cur`; Spoiler -> alterna `revealed`; Mention -> abre menu/popout;
    // Link -> ui.ctx().open_url(OpenUrl::new_tab(url)); cursor de mão: ctx.set_cursor_icon(PointingHand)
}
```
Para **pílulas que quebram linha**, percorrer `galley.rows` (cada `PlacedRow` tem `pos`, `glyphs` com `pos.x`/`advance_width`) e emitir um retângulo por linha; a ordem de pintura deve ser **fundos antes do galley**.

**Emoji**:
- Unicode: com `color_fonts` + fonte de emoji registrada no fallback de `Proportional`, o emoji sai no fluxo do texto (nada a fazer). Sem isso, caem em `NotoEmoji-Regular` monocromático (feature `monochrome_emoji_fonts`) ou em tofu. **Jumbo** = outro `LayoutJob` com `font_id` 48 px.
- Do servidor (imagem): inserir `\u{2003}` (em-space) com tamanho de fonte tal que o avanço ≈ 22 px, registrar `Span::image(a, b, emoji_id)` e, depois de `galley`, pintar `ui.painter().image(texture_id, rect_do_char, uv, WHITE)`; a textura vem de um cache `HashMap<(i64, u16), TextureHandle>` (baixar PNG/WebP de 48 px; GIF animado só com *hover* ou autoplay ligado, quadro a quadro, `request_repaint_after(frame_delay)`). Tooltip `:nome:` após 150 ms.
- Atenção à altura da linha: `line_height` fixa (22) evita que a linha cresça com o emoji.

**Spoiler**: oculto = texto transparente + pílula `LINE2`; clique alterna e guarda `(message_id, spoiler_id)` em `HashSet` no `App` (só em memória; ao recarregar a mensagem fica oculta de novo, igual ao Discord). Enquanto oculto, ignorar cliques de link/menção dentro do trecho. Selecionar/copiar texto oculto: copiar a versão revelada só se revelado.

**Bloco de código**: `Frame::new().fill(L1).stroke(Stroke::new(1.0, LINE)).corner_radius(6).inner_margin(Margin::symmetric(10, 8))`; largura máx. 90% da coluna; `ScrollArea::horizontal()`; botão "Copiar" no canto superior direito que aparece em hover (opacidade por `hover_t`). Realce: **fase 1 sem cor**; **fase 2** um tokenizador próprio (~150 linhas) por linguagem para `rust, js/ts, python, json, bash, sql, c/cpp, html/css, toml/yaml, diff`, com 6 papéis de cor vindos do Grafite (palavra-chave `AMBER`, string `OK`, número/literal `TEXT`, comentário `MUTED` itálico, tipo/função `TEXT` w600, operador `TEXT2`). Evitar `syntect`/`tree-sitter` (RAM da máquina de build e do usuário). `ansi`: interpretar `ESC[<n>m` para 16 cores e negrito/sublinhado.

**Carimbos `<t:>`**: sem `chrono` locale: tabelas de meses e dias da semana em pt-BR (12 + 7 strings) e `chrono::Local` para o fuso. `R`: calcular "há/em N unidade(s)" com os limiares do moment (s<45 → "alguns segundos", etc.) e `ctx.request_repaint_after(...)` com a tabela de intervalos de 4.1.

### 6.4 Movimento: molas e hover assimétrico

```rust
// motion.rs
use egui::{Context, Id};

/// Mola do react-spring (massa 1). Posição normalizada 0->1 partindo do repouso; t em segundos.
pub fn spring(t: f32, tension: f32, friction: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    let w0 = tension.sqrt();
    let z = friction / (2.0 * w0);
    if z < 1.0 {
        let wd = w0 * (1.0 - z * z).sqrt();
        1.0 - (-z * w0 * t).exp() * ((wd * t).cos() + (z * w0 / wd) * (wd * t).sin())
    } else if (z - 1.0).abs() < 1e-4 {
        1.0 - (-w0 * t).exp() * (1.0 + w0 * t)
    } else {
        let s = (z * z - 1.0).sqrt();
        let (r1, r2) = (-w0 * (z - s), -w0 * (z + s));
        1.0 - (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r2 - r1)
    }
}
// `animate_bool_with_time_and_easing` pede `fn(f32)->f32` sem captura: uma função por mola.
// t01 vai de 0 a 1 em `dur` segundos (passado em animate_*); mapeamos para o tempo da mola.
pub fn ease_tooltip(t01: f32) -> f32 { spring(t01 * 0.20, 2400.0, 52.0) }   // overshoot fica >1: ok p/ escala
pub fn ease_modal_in(t01: f32) -> f32 { spring(t01 * 0.22, 1000.0, 48.0) }
pub fn ease_modal_out(t01: f32) -> f32 { spring(t01 * 0.22, 1200.0, 80.0) }
pub fn ease_popout(t01: f32) -> f32 { spring(t01 * 0.30, 800.0, 32.0) }

/// Hover assimétrico: entra em 150 ms (ease-out), sai em 50 ms.
pub fn hover_t(ctx: &Context, id: Id, on: bool, reduce: bool) -> f32 {
    let dt = ctx.input(|i| i.stable_dt).min(0.05);
    let mut v: f32 = ctx.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let target = if on { 1.0 } else { 0.0 };
    let step = if reduce { 1.0 } else { dt / if on { 0.15 } else { 0.05 } };
    v = if v < target { (v + step).min(target) } else { (v - step).max(target) };
    ctx.data_mut(|d| d.insert_temp(id, v));
    if v != target { ctx.request_repaint(); }
    crate::theme::ease_out(v)
}
```

Uso de modal: `let t = ctx.animate_bool_with_time_and_easing(id, open, 0.22, ease_modal_in)`; escala `lerp(0.9, 1.0, t.min(1.0))` aplicada com `ui.scope` + `Area::...` e `ui.set_opacity(t.clamp(0.0, 1.0))` (o egui não escala widgets arbitrários: para o efeito "scale" desenhar o `Frame` com `rect.scale_from_center`, ou **simplificar**: só fade + deslocamento vertical de 6 px; combina melhor com o Grafite e custa menos). Scrim: `painter.rect_filled(screen, 0.0, Color32::from_black_alpha((180.0 * t) as u8))`.

**Reduzir movimento**: `struct Motion { reduce: bool }` em `App`, com modos `Auto|Ligado|Desligado`. `Auto` lê, no Linux, `gsettings get org.gnome.desktop.interface enable-animations` (uma vez na partida, `false` = reduzir) [I]. Quando `reduce`: `ctx.global_style_mut(|s| s.animation_time = 0.0)`, `hover_t` instantâneo, molas viram `t01 ≥ 1 → 1` com duração 0.0, nada de translação/escala, cursor de texto sem piscar (`ctx.options_mut(|o| o.input_options...)` [?]), GIF/emoji animados só em hover (Discord também), spinner vira pulso de opacidade. Cor continua (hover muda de cor imediatamente, como o `50ms`).

Valores padrão do Grafite (todas em segundos, usadas por `animate_*`):

| evento | duração | curva |
|---|---|---|
| hover entra / sai | 0.15 / 0.05 | `ease_out` (cubic) / linear |
| seleção de item (pílula, barra âmbar) | 0.16 | `cubic-bezier(.2,.7,.2,1)` ≈ `ease_out` |
| barra de botões de hover da mensagem | 0.12 (opacidade) + 3 px de subida | `ease_out` (a maquete usa `translateY(3px)`) |
| tooltip | atraso 0.15 (ícones) / 0.5 (texto longo); entrada 0.15 | `ease_tooltip` |
| menu de contexto / popout | 0.17 | `ease_popout` (só opacidade + 8 px se reduzir for falso) |
| modal + scrim | 0.22 | `ease_modal_in/out` |
| pular para mensagem (flash) | entra 0.2, sai 0.6; 2 pulsos de 0.6 | fundo `alpha(AMBER, .10)` |
| toast | 0.30; permanece 3-20 s (padrão 5 s) | |
| mensagem nova no fim da lista | opcional: opacidade 0→1 em 0.12 s só para mensagens recebidas com a janela em primeiro plano e o scroll no fundo | |

### 6.5 Atalhos: despachante central

```rust
// shortcuts.rs
use egui::{Context, Key, KeyboardShortcut, Modifiers as M};
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action { QuickSwitcher, ChannelPrev, ChannelNext, UnreadPrev, UnreadNext, MentionPrev, MentionNext,
    ServerPrev, ServerNext, HistoryBack, HistoryForward, ToggleLastServer, GoServer(u8),
    MarkChannelRead, MarkServerRead, Search, Pins, Inbox, ToggleMembers, EmojiPicker, GifPicker,
    ScrollPageUp, ScrollPageDown, JumpFirstUnread, FocusInput, UploadFile, CopyChannelLink,
    ToggleMute, ToggleDeafen, ShortcutsHelp, ContextMenu, CreateServer, EditLast }

pub const CTRL: M = M::COMMAND;                          // Ctrl no Linux/Windows, Cmd no macOS
pub const CTRL_SHIFT: M = M { command: true, shift: true, ..M::NONE };
pub const CTRL_ALT: M = M { command: true, alt: true, ..M::NONE };
pub const CTRL_SHIFT_ALT: M = M { command: true, shift: true, alt: true, ..M::NONE };
pub const ALT_SHIFT: M = M { alt: true, shift: true, ..M::NONE };

/// Mais específico primeiro (consume_shortcut ignora Shift/Alt extras!).
pub const TABLE: &[(Action, KeyboardShortcut)] = &[
    (Action::MentionNext, KeyboardShortcut::new(CTRL_SHIFT_ALT, Key::ArrowDown)),
    (Action::MentionPrev, KeyboardShortcut::new(CTRL_SHIFT_ALT, Key::ArrowUp)),
    (Action::ServerNext,  KeyboardShortcut::new(CTRL_ALT, Key::ArrowDown)),
    (Action::ServerPrev,  KeyboardShortcut::new(CTRL_ALT, Key::ArrowUp)),
    (Action::UnreadNext,  KeyboardShortcut::new(ALT_SHIFT, Key::ArrowDown)),
    (Action::UnreadPrev,  KeyboardShortcut::new(ALT_SHIFT, Key::ArrowUp)),
    (Action::ChannelNext, KeyboardShortcut::new(M::ALT, Key::ArrowDown)),
    (Action::ChannelPrev, KeyboardShortcut::new(M::ALT, Key::ArrowUp)),
    (Action::QuickSwitcher, KeyboardShortcut::new(CTRL, Key::K)),
    (Action::ToggleMute,  KeyboardShortcut::new(CTRL_SHIFT, Key::M)),
    (Action::ToggleDeafen,KeyboardShortcut::new(CTRL_SHIFT, Key::D)),
    (Action::MarkServerRead, KeyboardShortcut::new(M::SHIFT, Key::Escape)),
    /* ... */
];

/// Chamar no início de `update`, antes de qualquer widget. Devolve as ações disparadas neste quadro.
pub fn poll(ctx: &Context, text_focus: bool, modal_open: bool) -> Vec<Action> {
    let mut out = Vec::new();
    ctx.input_mut(|i| {
        for (a, sc) in TABLE {
            // atalhos com Ctrl funcionam com campo de texto focado; teclas simples não
            let needs_free = sc.modifiers == M::NONE || sc.modifiers == M::SHIFT;
            if (text_focus && needs_free) || (modal_open && !matches!(a, Action::ShortcutsHelp)) { continue; }
            if i.consume_shortcut(sc) { out.push(*a); }
        }
    });
    out
}
```
`text_focus = ctx.wants_keyboard_input()`. A tabela é a fonte da tela "Teclas de atalho" (Ctrl+/) e permite remapeamento (`HashMap<Action, KeyboardShortcut>` salvo em JSON; conflitos detectados ao gravar).

**Cascata do Esc** (uma ação por toque, na ordem do cliente): fechar menu/popout/seletor aberto -> fechar busca -> cancelar edição -> cancelar resposta -> fechar modal -> se há não lidas ou o chat está longe do fim: marcar o canal como lido e saltar para o presente -> senão nada. Implementar como `fn handle_escape(&mut self) -> bool` com os `if` nessa ordem.

**Modo teclado/mouse**: `keyboard_mode = true` ao `Tab`/setas/atalho de navegação; `false` ao `PointerMoved`/clique. Guardar em `App`; o anel de foco e as dicas só aparecem em `keyboard_mode`; o fundo de hover das mensagens e a barra de botões só em `!keyboard_mode`.

Atalhos "globais" do SO (PTT com o jogo em foco) ficam **fora**: no X11 exigem `XGrabKey`/`global-hotkey`; no Wayland, portal `GlobalShortcuts`. Não fazer agora.

### 6.6 Acessibilidade

- **Rótulos** para tudo que é pintado à mão: `resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Servidor Casa, 3 menções"))` no ícone da barra, avatar clicável, botões de ícone ("Responder", "Mais"), pílulas de reação ("👍 4 reações, você reagiu"). Textos pintados com `painter().text` não geram nó: usar `ui.label` ou `widget_info`.
- **Papéis e listas**: `ctx.accesskit_node_builder(id, |node| { node.set_role(accesskit::Role::List); node.set_label("Mensagens em #geral"); })` na lista de mensagens; `Role::TreeItem` com nível para canais; `Role::Dialog` + `set_label` nos modais.
- **Região viva** para nova mensagem/erro: um nó oculto (`Role::Status`, `Live::Polite`) cujo texto é trocado ao chegar mensagem do canal aberto ("Ana: oi"); sem repetir o histórico. [I: API exata do `accesskit` dependente da versão 0.25]
- **Foco visível**: `fn focus_ring(ui, rect, radius, inset)` desenha `Stroke::new(2.0, AMBER)` com `rect.expand(2.0)` (anel externo, controles) ou `rect.shrink(2.0)` (linhas de lista/itens de menu), só quando `keyboard_mode && resp.has_focus()`. Nunca remover o foco por `Frame::none()` em botões.
- **Contraste**: contraste medido (WCAG): `MUTED` (#8a8882) sobre `L1` 5.4:1, `L2` 5.2:1, `L3` 4.8:1, **`L4` (hover) 4.3:1 (abaixo de 4.5)**; `TEXT2` 7.5-9.3:1; `AMBER` 8-10:1. Texto pequeno em `MUTED` sobre `L4` (carimbo na linha em hover) reprova: clarear `MUTED` para ~#9a988f ou usar `TEXT2` no hover e no modo de alto contraste. Modo **alto contraste**: `LINE`→`LINE2`, `MUTED`→`TEXT2`, bordas de 2 px, links sublinhados, menções com borda, âmbar sólido nas seleções.
- **Tamanho**: `chat_font_px` em `[12,14,15,16,18,20,24]` (mantendo o padrão do Grafite em 14.5 no meio da escala; ou mover o padrão para 15) e `zoom` = `ctx.set_zoom_factor` com os 11 passos acima.
- **Sem depender de cor**: estado de menção/não lido também por forma (ponto, barra, número), como o Discord.

---

## 7. Ordem de entrega (passos pequenos e verificáveis)

1. **Tokens**: criar `tokens.rs` com `sp/rad/icon/av/type scale` e trocar números soltos de `main.rs`/`theme.rs` (chat 14.5/22, avatares 40/32/24, raios 4/8/12). *Usuário vê:* chat com altura de linha 22 e avatar 40, mesmo visual com medidas coerentes.
2. **Peso variável**: aplicar `coords wght` no nome do autor (600) e títulos serifa; trocar `RichText::strong()` por helper `bold()`. *Vê:* nomes realmente mais fortes sem trocar de fonte.
3. **Markdown fase 1 (inline)**: parser de `**`, `*`/`_`, `__`, `~~`, `` ` ``, links soltos; render por `LayoutJob`. *Vê:* mensagens com negrito, itálico, sublinhado, tachado e código em linha; link abre no navegador.
4. **Markdown fase 2 (blocos)**: bloco de código com cópia, citação (`>`, `>>>`), listas, títulos `#`, subtexto `-#`. *Vê:* mensagens longas estruturadas.
5. **Spoiler + menções + carimbo**: pílulas com raio e hit-test; `<@id>`, `<@&id>`, `<#id>`, `@everyone`, `<t:...>`; autocompletar `@`/`#` no composer. *Vê:* menções âmbar clicáveis, spoiler que revela ao clicar, "há 3 minutos" que atualiza.
6. **Motion**: `motion.rs` com `hover_t`, molas e `Motion.reduce`; aplicar em lista de canais, barra de hover da mensagem, tooltips, modais. Opção "Reduzir movimento" nas configurações. *Vê:* transições curtas e, com a opção ligada, nenhuma.
7. **Atalhos**: `shortcuts.rs` com a tabela (Ctrl+K, Alt+↑/↓, Esc em cascata, Ctrl+/ abre a lista). *Vê:* navegação só com teclado e a tela de atalhos.
8. **Foco e a11y**: `keyboard_mode`, `focus_ring`, `widget_info` nos ícones, nó `List`/`Dialog`. *Vê:* anel âmbar ao navegar por Tab; Orca lê os botões.
9. **Emoji**: ligar `color_fonts` + fonte de emoji no fallback; emoji Unicode coloridos, jumbo. Depois emoji do servidor (imagem). *Vê:* emojis coloridos no chat.
10. **Realce de sintaxe e prévia de link**: tokenizador simples e *unfurl* no servidor.

---

## 8. Perguntas em aberto

1. **Cor do link**: manter `AMBER` (igual ao acento e às menções, que têm fundo) ou usar `TEXT` sublinhado para não competir com o acento? (muda 2.8 e o `hyperlink_color`).
2. **Emoji colorido**: aceitar empacotar uma fonte de emoji no binário (~1-2 MB COLRv1 ou 11 MB CBDT) e ligar `color_fonts` (primeiro build de teste necessário)? Ou ficar com emoji monocromático?
3. **Tamanho-base do chat**: manter 14.5 (maquete) ou subir para 16 como o Discord (muda tudo em razão)? E o espaço entre grupos: 16 (padrão do Discord)?
4. **Realce de sintaxe**: vale o tokenizador próprio (fase 2) ou deixar blocos monocromáticos?
5. **Reduzir movimento "Auto"**: ler a preferência do GNOME/KDE (via `gsettings`/portal) ou só toggle manual?
6. **Atalhos**: manter os binds do Discord como padrão (Ctrl+K, Alt+↑/↓, Ctrl+Shift+M...) ou adaptar alguns (no Linux `Ctrl+Alt+↑/↓` colide com troca de área de trabalho em vários ambientes)?
