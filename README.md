# Telacast

Um clone próprio do Discord em Rust: chat, servidores e canais, voz, vídeo e transmissão de tela. Cliente desktop em **egui**, servidor **axum** + **PostgreSQL**. Não usa a API nem os servidores do Discord. Projeto em desenvolvimento ativo, feito por uma pessoa com ajuda de agentes de código; espere arestas.

## O que já existe

- **Chat:** servidores, categorias e canais, DMs, amigos, respostas, edição, reações, fixadas, busca, menções com autocomplete, anexos (imagens, prévia de texto/código, até 10 por mensagem, Ctrl+V de imagem), Markdown, bloqueio.
- **Voz:** canais de voz com Opus pelo WebSocket, mudo/surdo, anel de fala, volume por pessoa, supressão de ruído (RNNoise), ganho automático, modo de baixa latência e "ouvir meu microfone".
- **Transmissão de tela (Linux):** telas e janelas com miniaturas ao vivo, resolução e fps, H.264 por NVENC (com fallback em software), prévia da própria tela, espectadores, modo foco.
- **Conta:** cadastro só por convite, trocar senha, lista de sessões, atualização do app com conferência de hash e assinatura.
- **Visual "Grafite":** cinzas em camadas, um acento âmbar, só tema escuro. Barra de título própria.

## Rodar (Linux)

```bash
docker compose -f docker-compose.dev.yml up -d              # Postgres 18
cp .env.example .env
cargo run -p telacast-server -- invite --uses 5 --days 30   # o código aparece uma vez
cargo run -p telacast-server -- serve                       # ws://127.0.0.1:8080
cargo run --release -p telacast                             # o app
cargo test -p telacast-server                               # precisa do Postgres no ar
```

Detalhes, limites e o guia de implantação (Caddy + systemd, ou túnel da Cloudflare): [`CLAUDE.md`](CLAUDE.md) e [`docs/DEPLOY.md`](docs/DEPLOY.md).

## Windows

O cliente compila para Windows (`cargo zigbuild --release --target x86_64-pc-windows-gnu -p telacast`). No Windows, telas e janelas são capturadas pelo Windows Graphics Capture; no Linux, a captura continua usando X11 + NVENC. Binários prontos estão nas [releases](../../releases). O executável não é assinado, então o Windows avisa antes de abrir.

## Estrutura

| Pasta | O que é |
|---|---|
| `app/` | cliente egui (telas, tema, voz em `voice.rs`, captura, atualização) |
| `server/` | gateway WebSocket, HTTP de anexos e atualizações, consultas SQL, testes e2e |
| `proto/` | protocolo JSON do gateway e quadros binários de áudio |
| `md/`, `qs/` | parser de Markdown do chat e lógica do Ctrl+K, sem dependências |
| `vendor/` | cópia ajustada do `nvidia-video-codec-sdk` (licença MIT em `vendor/nvidia-video-codec-sdk/LICENSE`) |
| `docs/` | planos (`ROADMAP.md`, `MOBILE-PLAN.md`), especificações de interface (`ui-spec/`) e pesquisas |

O plano completo e o que falta estão em [`ROADMAP.md`](ROADMAP.md).

## Segurança

Cadastro fechado por convite, senhas com argon2, tokens guardados só como hash, limites por IP e por usuário, TLS pelo proxy. Nada disso foi auditado por terceiros. Não ponha senhas, tokens nem convites em arquivos do repositório. Para reportar um problema de segurança, abra um aviso privado de vulnerabilidade no GitHub.

## Licença e créditos

**Ainda não há licença definida**: por padrão, todos os direitos ficam reservados ao autor até ele escolher uma. Fontes embutidas (Inter, Newsreader, JetBrains Mono e outras em `app/assets/fonts`) são distribuídas sob a SIL Open Font License. As interfaces seguem a estrutura do Discord, que é marca registrada de seus donos; este projeto não é afiliado ao Discord.
