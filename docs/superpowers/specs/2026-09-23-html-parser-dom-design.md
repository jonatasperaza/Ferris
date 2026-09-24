# Ferris — Sub-projeto 2.1: HTML Parser → DOM Tree

Status: aprovado para implementação
Data: 2026-09-23

## Contexto

Sub-projeto 1 (render/compositor GPU a 120fps) e sub-projeto 1.5 (hardening:
crate lib+bin, contrato de coordenadas HiDPI, frame pacing vsync) estão
completos e publicados em `github.com/jonatasperaza/Ferris`. O
`ferris-compositor` agora expõe `scene::{DrawCommand, RectCommand,
TextCommand, Frame}`, `renderer::Renderer`, e `perf::FrameTimer` como uma
biblioteca consumível.

O objetivo original do Ferris (navegador 100% Rust, alternativa ao
Chromium) requer um motor de parsing HTML/CSS + DOM + layout antes de
poder renderizar uma página real — hoje o compositor só desenha uma cena
de teste hardcoded (`scene::build_test_scene`). "Sub-projeto 2" (motor
HTML/CSS/DOM) foi identificado como grande demais para um único spec e
foi quebrado em peças menores:

1. **HTML parser → DOM tree** ← este documento
2. CSS parser → stylesheet
3. Resolução de estilo (cascata, computed style)
4. Layout engine (box model, block/inline layout)
5. Ponte layout → `scene::Frame` (integração com o compositor)

Cada peça segue seu próprio ciclo spec → plano → implementação. Este
documento cobre apenas a peça 1.

## Objetivo

Parsear um subset pragmático de HTML5 (elementos, atributos, texto,
comentários, aninhamento arbitrário) em uma árvore DOM navegável em Rust,
sem nenhuma dependência de GPU/renderização. Fora de escopo: DOCTYPE,
entidades HTML (`&amp;`, `&lt;`, etc — texto é armazenado literal),
tratamento especial de `<script>`/`<style>` (conteúdo é parseado como
HTML normal, não como texto raw), CSS, layout, renderização, e o
algoritmo completo de error-recovery do spec HTML5 (substituído por duas
regras pragmáticas — ver "Tratamento de erros").

**Critério de sucesso:** dado uma string HTML pequena com elementos
aninhados, atributos, texto e comentários, `Parser::parse` retorna uma
árvore `Node` cuja estrutura (tags, atributos, texto, aninhamento) bate
exatamente com o HTML de entrada, validado por testes automatizados —
sem GPU, sem verificação manual (este sub-projeto é lógica pura).

## Arquitetura

O repositório vira um Cargo workspace. O crate `ferris-compositor`
existente (hoje na raiz do repo) move para `ferris-compositor/` como
membro do workspace; um novo crate `ferris-dom` é criado em `ferris-dom/`
como segundo membro. Isso porque parsing de HTML/DOM é lógica pura sem
nenhum acoplamento a wgpu/glyphon — misturar no crate do compositor
bagunçaria a separação de responsabilidades que os sub-projetos 1/1.5 já
estabeleceram.

`ferris-dom` tem três módulos:

- `tokenizer.rs` — `Tokenizer` (texto → stream de `Token`), testável
  isoladamente sem nenhuma árvore
- `parser.rs` — `Parser` (stream de `Token` → árvore `Node`), testável
  isoladamente dado um `Vec<Token>` já pronto
- `dom.rs` — os tipos da árvore em si: `Node`, `Element`

## Componentes

- `dom::Node` — enum: `Element(Element)`, `Text(String)`,
  `Comment(String)`
- `dom::Element` — struct: `tag_name: String`, `attributes:
  HashMap<String, String>`, `children: Vec<Node>`
- `tokenizer::Token` — enum: `TagOpen { name: String, attributes:
  HashMap<String, String>, self_closing: bool }`, `TagClose { name:
  String }`, `Text(String)`, `Comment(String)`
- `tokenizer::Tokenizer` — consome um `&str`, produz `Vec<Token>` (ou um
  iterador — decisão de implementação, não muda o contrato)
- `parser::Parser` — consome `&[Token]` (ou `Vec<Token>`), produz um
  `Node::Element` raiz cujos `children` são o conteúdo de nível superior
  do documento

## Fluxo de dados

`&str` de entrada → `Tokenizer` reconhece char a char e emite `Token`s na
ordem em que aparecem no texto → `Parser` consome os tokens
sequencialmente mantendo uma pilha de elementos abertos: em
`TagOpen` não-`self_closing`, empilha um novo `Element` e o adiciona como
filho do elemento no topo da pilha atual; em `TagOpen` `self_closing`,
adiciona o `Element` como filho do topo sem empilhar; em `TagClose`,
desempilha (aplicando as regras de erro abaixo); em `Text`/`Comment`,
adiciona como filho do elemento no topo da pilha. Ao final do stream de
tokens, qualquer elemento ainda na pilha é fechado automaticamente (regra
de erro abaixo) e o `Node::Element` raiz (o único item restante na base
da pilha) é retornado.

## Tratamento de erros

Sem implementar o algoritmo de error-recovery completo do HTML5 (spec
imenso). Duas regras pragmáticas:

1. **Tag não fechada até o fim do input:** ao esgotar o stream de tokens,
   qualquer elemento ainda na pilha é fechado automaticamente, na ordem
   reversa de abertura (o mais recentemente aberto fecha primeiro).
2. **Tag de fechamento sem correspondência:** se um `TagClose` não bate
   com o `tag_name` do elemento no topo da pilha, o `TagClose` é ignorado
   (descartado, não desempilha nada, o parse continua normalmente).

Essas duas regras evitam que HTML malformado comum trave o parser, sem
replicar o algoritmo de reconstrução de árvore do spec HTML5 real.

## Testes

- **Tokenizer:** testes unitários isolados — reconhece uma tag de
  abertura simples, uma tag com múltiplos atributos, uma tag
  auto-fechada (`<br>`, `<img src="x">`), texto entre tags, um
  comentário, tags aninhadas em sequência. Cada teste monta uma string
  HTML pequena e verifica o `Vec<Token>` exato produzido.
- **Parser:** testes unitários isolados — dado um `Vec<Token>` construído
  manualmente (não passando pelo Tokenizer), verifica a forma da árvore
  `Node` resultante: aninhamento correto, atributos preservados, texto e
  comentários nas posições certas, a regra de auto-fechamento no fim do
  stream, a regra de tag de fechamento ignorada.
- **Integração:** 2-3 testes fim-a-fim (`Tokenizer` → `Parser` juntos)
  com HTML pequeno realista (ex: `<div class="a"><p>Hello <b>world</b></p><!-- note --></div>`),
  verificando a árvore final completa.

## Fora de escopo (adiado para peças futuras desta decomposição)

DOCTYPE, entidades HTML (`&amp;` etc — armazenadas literais por
enquanto), tratamento especial de `<script>`/`<style>` como texto raw,
CSS (peça 2), resolução de estilo/cascata (peça 3), layout engine (peça
4), qualquer renderização ou integração com `ferris-compositor` (peça 5),
algoritmo completo de error-recovery do HTML5.
