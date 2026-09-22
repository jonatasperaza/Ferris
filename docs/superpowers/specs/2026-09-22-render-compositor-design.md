# Ferris — Sub-projeto 1: Render/Compositor GPU a 120fps

Status: aprovado para implementação
Data: 2026-09-22

## Contexto

Ferris é um navegador com motor de renderização próprio escrito 100% em
Rust, visando uso de memória muito menor que navegadores baseados em
Chromium e uma experiência de UI fluida a 120fps, no estilo do editor
Zed. O projeto completo (parser HTML/CSS, layout, JS engine, UI shell,
extensões, networking) é grande demais para um único spec, e foi
quebrado nos seguintes sub-projetos independentes:

1. **Render/compositor GPU a 120fps** ← este documento
2. Núcleo HTML/CSS parser + DOM + layout engine
3. Decisão e integração de JS engine
4. UI shell (chrome do navegador: abas, omnibox, settings)
5. Compatibilidade de extensões (WebExtensions API)
6. Networking/segurança (TLS, sandboxing, cookies)

Cada sub-projeto segue seu próprio ciclo spec → plano →
implementação. Este documento cobre apenas o sub-projeto 1.

## Objetivo deste sub-projeto

Provar viabilidade de um pipeline de renderização em Rust capaz de
sustentar 120fps antes de investir no parser/DOM completo. Este
milestone renderiza formas geométricas simples e texto — não uma
página web real, não há DOM ainda.

**Critério de sucesso:** cena animada (retângulos em movimento + texto)
rodando 60 segundos contínuos com frame time médio ≤ 8.3ms (120fps) em
GPU dedicada de teste, validado por overlay on-screen mostrando FPS e
frame time em tempo real.

## Arquitetura

Crate `ferris-compositor`:

- Janela e event loop via `winit`
- Superfície GPU via `wgpu` (abstração sobre Vulkan/Metal/DX12/GL —
  mesma escolha do Zed/GPUI)
- Renderização de texto via `glyphon` (shaping + rasterização GPU,
  evita reinventar font shaping — tarefa de anos por si só)
- Formas simples (retângulos, bordas arredondadas) via pipeline
  próprio de quads instanciados sobre `wgpu`
- Loop de frame na thread principal, target 120Hz: `PresentMode::
  Immediate` onde suportado, fallback `Mailbox`/`Fifo` conforme
  capacidade do adapter/monitor

Sem DOM ou parser neste milestone. A cena é uma lista de comandos de
desenho montada em código Rust (estática ou animada via clock), não
carregada de HTML.

## Componentes

- `ferris-compositor` (crate raiz) — entry point, event loop winit
- `renderer/` — setup wgpu (device, queue, surface), pipeline de quads
  (vertex + instance buffer), integração glyphon
- `scene.rs` — enum `DrawCommand` (`Rect`, `Text`, ...) e struct
  `Frame` (lista de comandos do frame atual)
- `perf.rs` — contador de FPS, histograma de frame-time, tracking de
  budget (16.6ms@60fps vs 8.3ms@120fps)

## Fluxo de dados

Por frame:
1. Monta `Frame` com `DrawCommand`s (hardcoded, animados via clock)
2. Renderer traduz `DrawCommand`s em draw calls batelados — um
   instance buffer por tipo de comando
3. Submit para GPU
4. Present

Sem retained state entre frames neste milestone: cada frame recalcula
a lista de comandos do zero. Simplicidade é priorizada sobre
otimização — retained scene graph com damage tracking é preocupação de
sub-projeto futuro, quando houver DOM real gerando a cena.

## Tratamento de erros

- **Sem adapter GPU compatível ou sem suporte a 120Hz:** log do
  problema, fallback para o refresh rate disponível. Nunca crash.
- **Device lost (perda de contexto GPU):** recriar surface e
  reconectar device/queue, sem travar a aplicação.

## Teste e validação

- Overlay on-screen exibindo FPS instantâneo e frame time (ms) em
  tempo real.
- Cena de teste: retângulos coloridos em movimento + bloco de texto,
  rodando 60 segundos contínuos.
- Critério de aprovação: frame time médio ≤ 8.3ms durante a janela de
  teste, em GPU dedicada.
- Sem teste automatizado de pixel neste milestone — validação é visual
  (observação direta) + métricas de frame time. Testes de regressão
  visual ficam para quando houver conteúdo real (DOM) sendo
  renderizado.

## Fora de escopo (adiado para sub-projetos futuros)

- Parsing de HTML/CSS e DOM
- Layout engine (box model, flexbox, grid)
- Scene graph retido com damage tracking
- JS engine
- UI shell (abas, omnibox)
- Extensões
- Networking
- Testes automatizados de regressão visual
- Benchmark comparativo formal vs Chromium (métricas próprias bastam
  para este milestone)
