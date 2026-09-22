# Ferris — Sub-projeto 1.5: Hardening do Compositor (fundação para sub-projeto 2)

Status: aprovado para implementação
Data: 2026-09-22

## Contexto

O sub-projeto 1 (render/compositor GPU a 120fps) foi completado e validado:
20 retângulos animados com bordas arredondadas + texto renderizam via
wgpu/glyphon, com overlay de FPS ao vivo, sustentando frame time médio de
0.59ms — muito abaixo do orçamento de 8.3ms/120fps.

A revisão final da branch inteira (`docs/superpowers/plans/2026-09-22-render-compositor.md`,
commits `c0d3dec..eb2915f`) aprovou o merge ("Ready to merge: Yes") mas
levantou 17 achados de fundação/arquitetura, não bugs de correção. Este
documento cobre os 4 que bloqueiam de verdade o próximo sub-projeto (motor
HTML/CSS/DOM): sem eles, sub-projeto 2 herdaria um crate que não pode
importar, um contrato de coordenadas indefinido, e um loop de render que
desperdiça CPU/GPU sem necessidade — indo contra o objetivo original do
Ferris (usar menos recursos que Chromium).

Os outros 13 achados (device-lost recovery, histograma de frame-time,
log de adapter/refresh-rate, guards de reentrância, alocações por frame,
padding de uniform buffer, clamp de SDF radius, testes extras, limpeza de
`error.txt`/`output.txt`, CI gate) ficam explicitamente fora de escopo
deste documento — não bloqueiam sub-projeto 2 e podem ser endereçados
depois, task por task, sem urgência.

## Objetivo

Tornar o `ferris-compositor` consumível como biblioteca por sub-projeto 2,
com um contrato de coordenadas que já funciona do jeito que CSS/DOM vão
precisar, e um loop de render que não roda mais rápido que o monitor sem
motivo.

## Arquitetura

`ferris-compositor` passa de crate binário puro para lib+bin:

- `src/lib.rs` — novo arquivo, declara `pub mod scene; pub mod perf; pub mod renderer;`
- `src/renderer/mod.rs` — ganha `pub struct Renderer` (o que hoje é
  `GpuState` em `main.rs`: device, queue, surface, config, quad_pipeline,
  text_layer, scale_factor), com métodos `new`, `resize`, `render_frame`,
  `set_scale_factor`. `choose_present_mode` ganha um parâmetro
  `uncapped: bool` que inverte a ordem de preferência: por padrão
  `[Fifo, Mailbox, Immediate]` (vsync, cap no refresh rate do monitor);
  com `uncapped: true`, `[Immediate, Mailbox, Fifo]` (comportamento atual,
  sem cap).
- `src/main.rs` — vira casca fina: `App` cria janela, instancia
  `renderer::Renderer`, lê `FERRIS_UNCAPPED` do ambiente pra decidir o
  `uncapped` do present mode, roda o loop de eventos, chama
  `renderer.render_frame(&frame)`, e pula o redraw quando a janela está
  `Occluded` ou minimizada. O `Arc<Window>` passa a viver só em
  `App.window` — `Renderer` recebe `&Window` quando precisa (ex: pra ler
  `scale_factor()`), não guarda uma cópia própria.
- `src/scene.rs` — sem mudança de tipos; `RectCommand`/`TextCommand`
  ganham um doc-comment declarando que `x/y/width/height`/`size` são
  pixels lógicos (CSS-like), nunca físicos.
- `src/renderer/quad.rs` / `src/renderer/text.rs` — `prepare()` ganha um
  parâmetro `scale_factor: f32` e multiplica posição/tamanho de cada
  comando por ele antes de montar os buffers GPU.

## Componentes

Reorganização, não criação de responsabilidade nova:
- `Renderer` (novo nome de `GpuState`) concentra tudo que a spec original
  do sub-projeto 1 já mandava viver em `renderer/`: setup wgpu, pipeline
  de quads, integração glyphon. Antes vivia em `main.rs` por conveniência
  do MVP; agora vai pro lugar certo.
- `main.rs` fica só com: janela, event loop, clock de animação, leitura do
  `FrameTimer`, formatação do overlay — nada de GPU direto.

## Fluxo de dados (contrato HiDPI)

`window.scale_factor()` é lido em `App::resumed` (valor inicial) e
atualizado em `WindowEvent::ScaleFactorChanged` (novo handler). `Renderer`
guarda o `scale_factor` atual. `scene::build_test_scene` continua gerando
coordenadas baseadas no tamanho **lógico** da janela
(`window.inner_size().to_logical::<f32>(scale_factor)`), nunca no tamanho
físico. `Renderer::render_frame` é o único lugar que multiplica
posição/tamanho por `scale_factor` — a cena (`scene.rs`) nunca sabe que
DPI existe, exatamente como CSS `px` funciona antes de virar pixel de
tela. Isso é o contrato que sub-projeto 2 (layout engine) vai herdar
diretamente: coordenadas de layout são sempre lógicas.

## Tratamento de erros

Nenhuma mudança nas garantias existentes de não-panic em falha de GPU —
esta é uma reorganização, não uma rescrita de tratamento de erro (isso é
o achado #2 parqueado, fora de escopo). Único comportamento novo:
`WindowEvent::ScaleFactorChanged` reconfigura o surface do mesmo jeito que
`WindowEvent::Resized` já faz, pra não deixar a resolução dessincronizada
depois de mover a janela pra outro monitor com DPI diferente.

## Teste e validação

- Testes existentes (17) continuam passando, com paths/nomes atualizados
  pra `Renderer` onde aplicável.
- Novo teste TDD: `choose_present_mode` recebe `uncapped: bool` — testa
  que `uncapped: false` prefere Fifo primeiro, `uncapped: true` prefere
  Immediate primeiro (reaproveita as 3 combinações de present modes
  suportados do sub-projeto 1, agora parametrizadas por `uncapped`).
- Validação manual: `cargo run` (padrão) não deve passar do refresh rate
  do monitor — confirmar via overlay de FPS que o valor bate com o
  refresh rate reportado pelo Windows, ao contrário dos ~1700fps do
  sub-projeto 1. `FERRIS_UNCAPPED=1 cargo run` deve reproduzir o
  comportamento antigo (sem cap) pra manter o modo de benchmark
  disponível.
- Validação manual do contrato HiDPI: redimensionar a janela pra um
  monitor com DPI diferente (ou simular via configuração do Windows) e
  confirmar que os retângulos/texto mantêm o tamanho lógico correto
  (não ficam minúsculos/gigantes) — evidência via screenshot, seguindo o
  mesmo padrão de verificação manual usado no sub-projeto 1.

## Fora de escopo (parqueados, endereçar depois)

Device-lost recovery, histograma de frame-time / p99 / contador de
misses, log de adapter/refresh-rate, guard de reentrância em `resumed()`,
guard de `CloseRequested` quando `gpu` é `None`, redução de alocações por
frame, padding do uniform buffer pra 16 bytes, clamp do SDF radius,
testes de `hsv_to_rgba` e viewport degenerado (1x1), teste de
`size_of::<QuadInstance>()`, remoção de `error.txt`/`output.txt`, e
configuração de CI (`cargo fmt --check` + `cargo clippy -D warnings` +
workflow). Nenhum destes bloqueia sub-projeto 2 — todos podem virar tasks
avulsas depois, sem spec própria, ou um sub-projeto 1.6 se o volume
justificar.
