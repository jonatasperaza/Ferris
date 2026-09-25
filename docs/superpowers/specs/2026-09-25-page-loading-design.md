# Ferris — Sub-projeto 2.8: Carregamento de Página Real

Status: aprovado para implementação
Data: 2026-09-25

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.7 (motor HTML/CSS/
DOM/layout/paint completo, incluindo elementos inline) estão completos
e publicados em `github.com/jonatasperaza/Ferris`. O `ferris-compositor`
hoje só mostra um fixture HTML+CSS embutido em tempo de compilação
(`include_str!`), sem nenhum jeito de carregar conteúdo real.

Este documento cobre a peça 8 da decomposição do "sub-projeto 2" (motor
HTML/CSS/DOM): carregar uma página real (arquivo local ou URL via
HTTP), extrair suas folhas de estilo (`<link rel="stylesheet">` e
`<style>`), e desenhar essa página de verdade na janela — o primeiro
passo em direção a "abrir qualquer site", em vez de só o fixture fixo.

Durante o brainstorming, foi identificado um bug latente já existente:
o parser HTML (`ferris-dom`) não trata `<style>`/`<script>`/`<head>`/
`<link>`/`<meta>`/`<title>` de forma especial — o conteúdo textual
dessas tags viraria texto visível na página se alguma vez aparecessem
no fixture (nunca aconteceu até agora, já que o fixture não usa essas
tags). Esta peça corrige isso via uma pequena stylesheet padrão de
user-agent.

## Objetivo

Dado um caminho de arquivo local ou uma URL `http(s)://`, carregar o
HTML, extrair e concatenar (na ordem em que aparecem no documento)
todo CSS referenciado via `<link rel="stylesheet" href="...">` e
`<style>...</style>`, aplicar uma stylesheet padrão de user-agent que
esconde tags de metadado (`head`/`style`/`script`/`link`/`meta`/
`title`), e alimentar o pipeline já existente (`resolve_styles` →
`layout` → `paint`) pra desenhar a página real na janela GPU.

**Critério de sucesso:** `cargo run -p ferris-compositor
caminho/pagina.html` (ou uma URL `https://...`) abre a janela e mostra
a página real, com CSS externo/inline aplicado corretamente — validado
por testes automatizados de lógica pura (parsing de URL/caminho,
extração de folhas de estilo, ordem de concatenação, resiliência a
falha de um `<link>` específico) e por verificação manual rodando o
binário de verdade contra pelo menos um arquivo local e uma URL real.

## Arquitetura

Novo crate `ferris-loader`, nono membro do workspace, dependendo de
`ferris-dom`, `ferris-css`, `ferris-style` (path dependencies) mais
duas dependências externas novas: `ureq` (cliente HTTP bloqueante, sem
runtime assíncrono — consistente com o resto do projeto, que não usa
`async`/`tokio` em lugar nenhum) e `url` (parsing/junção de URLs
relativas).

`ferris-dom` ganha uma função pública nova, aditiva:
`pub fn parse_document(html: &str) -> Element` — encapsula o padrão
"tokenizar + parsear + descartar o wrapper sintético 'document' + achar
o primeiro elemento real", hoje duplicado como helper de teste em 4
arquivos diferentes (`ferris-style`, `ferris-layout`, `ferris-paint`
× 2). Nunca panica: entrada vazia ou sem nenhum elemento real devolve
um `Element::new("html")` vazio.

`ferris-compositor/src/main.rs` passa a aceitar um argumento de linha
de comando opcional (caminho ou URL) — sem argumento, continua
mostrando o fixture embutido (comportamento de hoje preservado); com
argumento, usa `ferris_loader::{parse_source, load_page}`.

## Componentes

```rust
// ferris-loader/src/lib.rs
pub enum Source {
    File(std::path::PathBuf),
    Url(String),
}

pub fn parse_source(arg: &str) -> Source {
    if arg.starts_with("http://") || arg.starts_with("https://") {
        Source::Url(arg.to_string())
    } else {
        Source::File(std::path::PathBuf::from(arg))
    }
}

pub enum LoadError {
    Fetch(String), // erro de I/O ou HTTP formatado como string — não
                    // expõe o tipo de erro de `ureq`/`std::io` na API
}

pub fn load_page(source: &Source) -> Result<(ferris_dom::dom::Element, ferris_css::stylesheet::Stylesheet), LoadError>
```

Internamente (privadas): `fetch_text(source: &Source) -> Result<String,
LoadError>` (lê arquivo ou faz `GET` via `ureq`); `resolve_href(base:
&Source, href: &str) -> Source` (junta um `href` relativo contra a
fonte base — `Path::join` pra arquivo, `url::Url::join` pra URL);
`extract_stylesheets(root: &Element, base: &Source) -> Vec<String>`
(percorre a árvore em ordem de documento — DFS pré-ordem, mesma ordem
em que as peças anteriores sempre trataram filhos — coletando o texto
de cada `<style>` direto e buscando+resolvendo cada
`<link rel="stylesheet" href="...">`, pulando individualmente
qualquer falha de busca sem propagar erro).

```rust
// ferris-dom/src/parser.rs (ou lib.rs, aditivo)
pub fn parse_document(html: &str) -> Element
```

## Fluxo de dados

`load_page(source)`:
1. `fetch_text(source)` — se falhar, retorna `Err(LoadError::Fetch(...))` imediatamente (página principal é obrigatória).
2. `ferris_dom::parser::parse_document(&html)` → `Element` raiz.
3. Monta a lista de fontes de CSS: começa com a stylesheet padrão fixa
   (`"head, style, script, link, meta, title { display: none; }"`),
   depois `extract_stylesheets(&root, source)` (cada folha de estilo
   externa/inline encontrada, na ordem do documento).
4. Concatena todas as fontes de CSS (`join("\n")`), tokeniza+parseia
   via `ferris_css` uma vez só.
5. Devolve `(root, stylesheet)`.

`main.rs`: `std::env::args().nth(1)` — se `Some(arg)`, chama
`parse_source` + `load_page`; em caso de `Err`, loga a mensagem no
stderr e encerra o processo (`std::process::exit(1)`) — não tem página
pra desenhar sem a principal. Se `None` (sem argumento), continua
usando o fixture embutido via `include_str!`, exatamente como hoje —
comportamento padrão preservado.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem cache, sem recarregar** — a página é buscada uma vez no início
  (mesma decisão já tomada na peça 2.5 pro fixture), sem suporte a
  navegação/recarregar/histórico.
- **`<link>`/`<style>` fora de `<head>`** são tratados igual (mesma
  extração, em qualquer profundidade) — real, mas não é uma limitação
  desta peça, é o comportamento correto.
- **Sem redirecionamento HTTP explícito além do que `ureq` já faz por
  padrão** — não implementamos lógica própria de redirect, confiamos
  no comportamento padrão da biblioteca.
- **Sem cookies, sem cabeçalhos HTTP customizados, sem autenticação** —
  só `GET` simples.
- **`@import` dentro de CSS não é seguido** — `ferris-css` já trata
  `@import` como at-rule desconhecida (ignorada, peça 2.2), continua
  assim.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`ferris_dom::parse_document`**: caso normal (HTML válido);
  entrada vazia → `Element` "html" sem filhos, sem panic; entrada só
  com texto solto (sem tag nenhuma) → não panica.
- **`parse_source`**: `"http://..."`/`"https://..."` → `Source::Url`;
  qualquer outra coisa → `Source::File`.
- **`resolve_href`**: arquivo base + `href` relativo → caminho
  correto; URL base + `href` relativo → URL correta; URL base + `href`
  absoluto (`https://outro-dominio/x.css`) → usa o absoluto.
- **`extract_stylesheets`**: só `<link>`; só `<style>`; os dois juntos
  na ordem certa do documento; `<link>` cuja busca falha não propaga
  erro (só é pulado, resto continua).
- **`load_page`** com arquivo local de teste (HTML + CSS externo via
  `<link>` + `<style>` inline juntos), conferindo que a stylesheet
  final tem as regras esperadas na ordem certa, incluindo a padrão de
  UA na frente.
- **Verificação manual** (obrigatória, não substituível por teste
  automatizado): rodar `cargo run -p ferris-compositor <arquivo
  local>` e confirmar visualmente que a página aparece; rodar contra
  uma URL `https://` real e confirmar o mesmo — peças 2.4-2.7 só
  acharam bugs reais rodando de verdade.
