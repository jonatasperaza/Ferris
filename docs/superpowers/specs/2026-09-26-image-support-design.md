# Ferris — Sub-projeto 2.11: Suporte a Imagens (`<img>`)

Status: aprovado para implementação
Data: 2026-09-26

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.7 (motor HTML/CSS/
DOM/layout/paint completo, incluindo elementos inline), 2.8 (carregamento
de página real via `ferris-loader`), 2.9 (navegação de uma aba), 2.10
(múltiplas abas) estão completos e publicados em
`github.com/jonatasperaza/Ferris`. Hoje o pipeline de layout/paint
(`ferris-layout`, `ferris-paint`, `ferris-scene`) só sabe desenhar
`RectCommand` (retângulos com cor sólida) e `TextCommand` — não existe
nenhum conceito de imagem em lugar nenhum do motor: `ferris-dom` trata
`<img>` como qualquer outro elemento (sem tratamento especial de `src`),
o layout não calcula dimensão intrínseca de imagem, `ferris-scene` não
tem um `ImageCommand`, e o compositor/renderer (wgpu) só tem o
`quad_pipeline` (retângulos sólidos) e o `text_layer` (texto via
glyphon) — nenhum pipeline de textura.

O objetivo declarado do usuário para o Ferris é se tornar um navegador
de verdade, usável no dia a dia. Praticamente toda página real tem
imagem — sem isso, qualquer site real fica visivelmente quebrado.

## Objetivo

Renderizar elementos `<img src="...">` de verdade: buscar e decodificar
PNG/JPEG (incluindo `data:` URIs em base64) a partir de arquivo local,
URL HTTP, ou dado embutido; calcular o tamanho da caixa no layout
(usando CSS `width`/`height` quando especificado, ou a dimensão
intrínseca da imagem decodificada como padrão); e desenhar a imagem de
verdade na tela via um pipeline de textura novo no compositor GPU.

**Critério de sucesso:** uma página real carregada via `ferris-loader`
(arquivo local ou URL) que contenha `<img>` com `src` apontando pra um
PNG ou JPEG de verdade (local, remoto, ou `data:` embutido) mostra essa
imagem de verdade na janela, no lugar e tamanho certos — validado por
testes automatizados de lógica pura (resolução de `src`, decodificação
real de arquivos PNG/JPEG de teste, cálculo de dimensão no layout,
extração de comandos de desenho) e por verificação manual rodando o
binário de verdade contra pelo menos uma imagem local e uma via URL.

## Arquitetura

Decodificação via crate `image` (padrão do ecossistema Rust — uma
dependência cobre PNG e JPEG, evita reinventar decodificadores).
`ferris-loader::load_page` ganha um passo novo, `extract_images`, que
percorre a árvore DOM (mesmo padrão DFS já usado por
`extract_stylesheets`), encontra cada `<img src="...">`, resolve o
`src` (arquivo/URL via `resolve_href` já existente; `data:` via decode
de base64 direto, sem fetch de rede) e decodifica os bytes, devolvendo
um `HashMap<String, DecodedImage>` (chave = o `src` cru do HTML, sem
resolver — único dentro de uma mesma página, evita precisar re-resolver
em `layout`, e naturalmente deduplica duas tags apontando pro mesmo
`src`).

`DecodedImage`/`ImageCommand` vivem em `ferris-scene` (o crate-folha já
existente, sem dependências) — assim `ferris-loader` (que constrói) e
`ferris-layout`/`ferris-paint` (que consomem) não criam uma dependência
circular entre si, mesmo padrão que motivou a extração de `ferris-scene`
na peça 2.5. `ferris-layout::layout` ganha o mapa de imagens como
parâmetro novo; `ferris-paint` emite `DrawCommand::Image` pros nós que
tiverem uma imagem decodificada associada.

O compositor ganha um pipeline de GPU novo, `renderer/image.rs`
(espelhando a estrutura de `renderer/quad.rs`): um shader de quad
texturizado (mesma transformação de posição/tamanho do `quad_pipeline`,
mas amostrando uma textura em vez de usar cor sólida), com uma textura
de GPU por imagem, subida uma vez e reaproveitada nos frames seguintes
(cache por identidade do ponteiro do `Arc<[u8]>` dos bytes decodificados
— evita re-enviar a mesma imagem pra GPU a cada frame a 120fps).

## Componentes

```rust
// ferris-scene/src/lib.rs (crate-folha, sem dependências novas)
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgba: std::sync::Arc<[u8]>, // RGBA8 straight alpha, width*height*4 bytes
}

#[derive(Debug, Clone)]
pub struct ImageCommand {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub image: DecodedImage,
}
// DrawCommand ganha uma 3ª variante: DrawCommand::Image(ImageCommand)

// ferris-loader/src/lib.rs
pub type ImageMap = std::collections::HashMap<String, ferris_scene::DecodedImage>;

pub fn load_page(source: &Source) -> Result<(Element, Stylesheet, ImageMap), LoadError>

fn extract_images(root: &Element, base: &Source) -> ImageMap
// DFS pré-ordem; pra cada <img> com atributo src:
//   - src começa com "data:" -> decodifica base64 direto (sem fetch)
//   - senão -> resolve_href(base, src) + fetch_text vira fetch_bytes
//     (nova função, paralela a fetch_text mas devolvendo Vec<u8>)
//   - decodifica via `image::load_from_memory(...)`, converte pra RGBA8
//   - falha em qualquer etapa (resolve, fetch, decodificação) -> essa
//     imagem é pulada silenciosamente, resto da extração continua

// ferris-layout/src/layout.rs
pub fn layout(root: &StyledNode, viewport_width: f32, viewport_height: f32, images: &ImageMap) -> Option<LayoutBox>
// LayoutBox ganha: pub image: Option<ferris_scene::DecodedImage>
// para um nó <img>: procura images.get(el.attributes.get("src")?);
//   se achar, width/height do CSS se especificado, senão a dimensão
//   intrínseca da DecodedImage; se não achar (falhou/sem src), caixa
//   0x0, image: None

// ferris-paint/src/paint.rs
// nó com layout_box.image = Some(decoded) -> emite DrawCommand::Image
// em vez do RectCommand normal daquele nó

// ferris-compositor/src/renderer/image.rs (novo)
pub fn build_image_instances(frame: &scene::Frame, scale_factor: f32) -> Vec<ImageDrawInfo>
// função pura, mesmo padrão de build_quad_instances — extrai e escala
// os ImageCommand de um Frame, testável sem GPU real
pub struct ImagePipeline { /* shader de textura + cache de wgpu::Texture
    por ponteiro de Arc<[u8]>; prepare()/render() no mesmo padrão de
    QuadPipeline/TextLayer, sem teste de unidade (requer GPU real —
    mesmo padrão já usado pelo próprio QuadPipeline/TextLayer, cuja
    criação de pipeline/textura/render também não tem teste de unidade,
    só verificação manual do binário) */ }
```

## Fluxo de dados

1. `load_page(source)`: busca a página → `parse_document` → monta CSS
   (igual hoje) → NOVO: `extract_images(&root, source)` roda em
   seguida, devolvendo o `ImageMap`.
2. `main.rs`'s `build_page_frame` passa o `ImageMap` pra
   `layout::layout(...)`, que preenche `LayoutBox.image` pros nós
   `<img>` com entrada correspondente.
3. `paint()` emite `DrawCommand::Image` pros nós com `image: Some(...)`.
4. `Renderer::render_frame`: `ImagePipeline::prepare` recebe os
   `ImageCommand`s do frame atual, sobe uma textura de GPU nova só na
   primeira vez que vê aquele ponteiro de `Arc<[u8]>` (cache), reaproveita
   nos frames seguintes; `render` desenha um quad texturizado por
   imagem, sem ordem relativa importante frente ao `quad_pipeline`/
   `text_layer` (imagem e texto/retângulos normalmente não se
   sobrepõem na mesma página).

## Tratamento de erro

`<img>` sem entrada no `ImageMap` (falhou ao resolver/buscar/decodificar,
`src` malformado, ou sem atributo `src`) vira `LayoutBox.image: None` —
o nó existe no layout mas com caixa 0×0 (sem tamanho reservado, sem
ícone de imagem quebrada). Nenhuma falha de imagem individual propaga
erro nem derruba o carregamento da página — mesma disciplina já usada
pra `<link>` quebrado desde a peça 2.8. A crate `image` devolve `Result`
em vez de panicar em bytes malformados; esse `Result` vira só mais um
caso de "pular essa imagem, continuar as outras".

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem `<picture>`/`srcset`** — só o atributo `src` simples é lido.
- **Sem preservar proporção quando só `width` OU só `height` é
  especificado no CSS** — usa a dimensão intrínseca pro outro eixo
  (pode distorcer); preservar proporção fica pra uma peça futura.
- **Sem ler atributos HTML `width=`/`height=`** — só CSS, consistente
  com o resto do motor (que também não traduz atributos HTML pra
  estilo em nenhum outro lugar).
- **Sem WebP, GIF, SVG** — só PNG/JPEG (o que a crate `image` decodifica
  nesta configuração).
- **Sem cache de imagem entre navegações** — recarrega e redecodifica a
  cada navegação, mesma disciplina de "sem cache" já aceita desde as
  peças 2.5/2.8.
- **Sem indicador de carregamento** — imagem grande via URL lenta
  bloqueia a navegação inteira até terminar de baixar, mesma limitação
  síncrona já documentada pra CSS/HTML desde 2.8.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`extract_images`**: decodifica um PNG e um JPEG de teste reais
  (arquivos pequenos criados no teste, mesmo padrão de arquivos-temp já
  usado em `ferris-loader`); resolve `src` relativo a arquivo e a URL;
  decodifica `data:image/png;base64,...` sem fazer fetch de rede; pula
  individualmente um `src` que não existe, um arquivo corrompido (bytes
  que não decodificam), e um `<img>` sem `src`, sem propagar erro nem
  parar a extração das demais.
- **`layout` com `<img>`**: usa a dimensão intrínseca da imagem quando
  CSS não especifica `width`/`height`; usa o `width`/`height` do CSS
  quando especificado; `image: None` e caixa 0×0 quando o `src` não tem
  entrada no `ImageMap`.
- **`build_image_instances`**: extrai só os `ImageCommand` de um
  `Frame` (ignora `Rect`/`Text`), aplica o fator de escala corretamente,
  frame vazio devolve lista vazia — mesmo padrão de teste de
  `build_quad_instances`, sem precisar de GPU real.
- **Verificação manual** (obrigatória, não substituível por teste
  automatizado): rodar o binário de verdade contra uma página com
  imagem local (arquivo PNG/JPEG de verdade no disco) E uma página com
  imagem via URL HTTP real, confirmando visualmente que a imagem
  aparece no lugar e tamanho certos — a criação/upload de textura de
  GPU (`ImagePipeline`) não tem teste de unidade, mesmo padrão já usado
  pelo `QuadPipeline`/`TextLayer` existentes, cuja lógica de GPU também
  só é verificada rodando o binário de verdade.
