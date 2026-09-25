# Ferris — Sub-projeto 2.9: UI de Navegação (Chrome)

Status: aprovado para implementação
Data: 2026-09-25

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.7 (motor HTML/CSS/
DOM/layout/paint completo, incluindo elementos inline), 2.8 (carregamento
de página real via `ferris-loader`, arquivo local ou URL HTTP) estão
completos e publicados em `github.com/jonatasperaza/Ferris`. Hoje
`ferris-compositor` abre uma janela GPU que mostra UMA página fixa,
determinada uma única vez por um argumento de linha de comando opcional
(ou o fixture embutido, se nenhum argumento for passado) — não existe
nenhuma UI de navegador: sem barra de endereço, sem histórico, sem botão
de voltar/avançar/recarregar, e a única forma de trocar de página é
matar o processo e rodar de novo com outro argumento.

O objetivo declarado do usuário para o Ferris é se tornar um navegador
de verdade, usável no dia a dia, comparável ao Google Chrome — não
apenas um motor de renderização. Esta peça é o primeiro passo nessa
direção: dar ao usuário uma forma real de navegar (digitar uma URL,
voltar, avançar, recarregar) sem sair do processo.

O escopo foi deliberadamente cortado durante o brainstorming: múltiplas
abas ficam para um sub-projeto futuro (2.10). Esta peça cobre só
navegação de uma aba/janela.

## Objetivo

Adicionar uma barra de chrome fixa no topo da janela do `ferris-compositor`
com: campo de endereço editável (clique pra focar, digitar, Enter navega,
Esc cancela a edição), botões Voltar/Avançar/Recarregar (clicáveis, com
estado habilitado/desabilitado conforme o histórico), e os atalhos de
teclado Ctrl+L (foca a barra), Alt+Seta-Esquerda/Alt+Seta-Direita
(voltar/avançar), F5 (recarregar). O conteúdo da página é desenhado
abaixo da barra, nunca escondido atrás dela.

**Critério de sucesso:** rodar `cargo run -p ferris-compositor` abre uma
janela com barra de endereço visível no topo; digitar uma URL ou caminho
de arquivo e apertar Enter carrega e mostra aquela página; voltar/avançar
navegam corretamente pelo histórico; uma falha de carregamento durante a
navegação (URL inválida, 404, arquivo inexistente) mostra um erro visível
na própria barra sem derrubar a janela — validado por testes automatizados
das partes de lógica pura (histórico, edição de texto, hit-testing de
clique, deslocamento de frame) e por verificação manual rodando o binário
de verdade.

## Arquitetura

Tudo dentro de `ferris-compositor` — a UI de chrome é específica desse
binário, nenhum outro crate a consome. Novo módulo
`ferris-compositor/src/chrome.rs` concentra: estado da barra de endereço,
histórico de navegação em memória, e a lógica pura (sem GPU) de layout e
desenho da barra como `scene::DrawCommand`s. `App` (em `main.rs`) ganha um
campo `chrome: Chrome` e passa eventos de teclado (`WindowEvent::KeyboardInput`)
e clique (`WindowEvent::MouseInput` + `WindowEvent::CursorMoved`, pra
rastrear a posição do cursor) para métodos de `Chrome`.

Nenhuma dependência externa nova — os widgets (campo de texto, botões)
são desenhados com as primitivas já existentes (`RectCommand`/
`TextCommand` de `ferris-scene`), mantendo o projeto 100% Rust do zero,
sem framework de UI.

## Componentes

```rust
// ferris-compositor/src/chrome.rs

pub struct NavigationHistory {
    entries: Vec<ferris_loader::Source>,
    current: usize,
}

impl NavigationHistory {
    pub fn new(initial: ferris_loader::Source) -> Self;
    /// Navega para `source`: descarta qualquer histórico "futuro" (entries
    /// após `current`), empurra `source` no fim, `current` aponta pra ele.
    pub fn go(&mut self, source: ferris_loader::Source);
    pub fn back(&mut self) -> Option<&ferris_loader::Source>;
    pub fn forward(&mut self) -> Option<&ferris_loader::Source>;
    pub fn current(&self) -> &ferris_loader::Source;
    pub fn can_go_back(&self) -> bool;
    pub fn can_go_forward(&self) -> bool;
}

pub struct AddressBar {
    text: String,
    focused: bool,
    cursor: usize, // índice de char (não byte) dentro de `text`
    error: Option<String>,
}

impl AddressBar {
    pub fn new(initial_text: String) -> Self;
    pub fn set_focused(&mut self, focused: bool);
    pub fn on_char(&mut self, c: char);
    pub fn on_backspace(&mut self);
    /// Enter: devolve o `Source` a navegar, ou `None` se o texto estiver
    /// vazio. Não limpa o campo nem o foco — quem chama decide isso após
    /// o resultado da navegação.
    pub fn commit(&self) -> Option<ferris_loader::Source>;
    /// Esc: reverte `text` para `reset_text` e sai do foco.
    pub fn cancel(&mut self, reset_text: &str);
    pub fn set_error(&mut self, message: Option<String>);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeAction {
    Back,
    Forward,
    Reload,
    FocusAddressBar,
}

pub struct Chrome {
    pub address_bar: AddressBar,
    pub history: NavigationHistory,
    pub bar_height: f32, // 44.0
}

impl Chrome {
    pub fn new(initial: ferris_loader::Source, initial_text: String) -> Self;
    /// Desenha o fundo da barra, o texto do campo de endereço (com cursor
    /// se focado, ou a mensagem de erro se houver uma), e os 3 botões.
    pub fn frame(&self, window_width: f32) -> ferris_compositor::scene::Frame;
    /// Converte coordenadas de clique (lógicas, já sem scale factor) numa
    /// ação, ou `None` se o clique caiu fora da barra.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ChromeAction>;
}

/// Desloca todo RectCommand/TextCommand de `frame` por `dy` no eixo Y,
/// preservando os demais campos. Usado para empurrar o conteúdo da
/// página pra baixo da barra de chrome.
pub fn translate_frame(frame: &ferris_compositor::scene::Frame, dy: f32) -> ferris_compositor::scene::Frame;
```

## Fluxo de dados

1. **Início**: `main()` mantém o argumento de CLI opcional como hoje (2.8).
   Se a página inicial falhar ao carregar, o processo ainda sai com
   `std::process::exit(1)` (comportamento preservado de 2.8) — não há UI
   pronta pra mostrar erro antes da janela existir. Se der certo,
   `Chrome::new(source, texto_inicial)` é criado com esse `Source` como
   primeira entrada do histórico.
2. **Digitação**: eventos `WindowEvent::KeyboardInput` do winit, quando
   `address_bar.focused` é `true`, viram chamadas a `on_char`/
   `on_backspace`. Enter chama `commit()`. Esc chama `cancel()`.
3. **Navegação** (Enter na barra, clique em Voltar/Avançar/Recarregar, ou
   os atalhos Ctrl+L/Alt+Seta-Esquerda/Alt+Seta-Direita/F5): toda
   navegação passa por uma função central em `main.rs`,
   `fn navigate(app: &mut App, source: ferris_loader::Source)`, que chama
   `ferris_loader::load_page(&source)`:
   - `Ok((root, stylesheet))`: atualiza `app.page_frame` via
     `build_page_frame`, `app.chrome.history.go(source)`,
     `app.chrome.address_bar.set_error(None)`, sai do modo de edição.
   - `Err(LoadError::Fetch(msg))`: `app.chrome.address_bar.set_error(Some(msg))`,
     NÃO atualiza `page_frame` nem `history` — a página anterior continua
     na tela, o processo continua rodando.
4. **Cada frame desenhado**: `translate_frame(&app.page_frame, chrome.bar_height)`
   concatenado com `chrome.frame(window_width)` (barra por cima, sempre
   nos primeiros pixels), igual já acontece hoje com o overlay de FPS.

## Tratamento de erro

A diferença central em relação à peça 2.8: uma falha de carregamento da
página **inicial** (via argumento de CLI, antes da janela existir de
verdade) ainda mata o processo com `exit(1)` — mesmo comportamento já
testado e aprovado em 2.8. Uma falha durante navegação **interativa**
(depois que a janela já está aberta e o usuário digitou uma URL nova, ou
clicou em recarregar uma página que caiu) **nunca** mata o processo —
fica como um erro visível na barra de endereço, e o usuário pode tentar
de novo ou navegar pra outro lugar. Isso é esperado e coerente com como
qualquer navegador real se comporta.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem múltiplas abas** — decisão explícita de escopo, fica pra 2.10.
- **Sem indicador de carregamento** (spinner/barra de progresso) — o
  carregamento de página já é síncrono e bloqueia a UI brevemente (mesma
  limitação documentada em 2.8: sem timeout de leitura HTTP); esta peça
  não adiciona feedback visual de "carregando", só o resultado final.
  Fica como limitação conhecida, candidata a uma peça futura.
- **Histórico não persiste** entre execuções do processo (sem sessão
  salva em disco).
- **Sem favoritos/bookmarks** — fora de escopo desta peça.
- **Seleção de texto no campo de endereço não suportada** — só cursor +
  digitar/apagar caractere a caractere; sem arrastar pra selecionar,
  copiar/colar, ou mover cursor com Home/End/setas.
- **Sem indicador visual de "clicável"** (hover) nos botões — clique
  funciona, mas não há feedback visual de passar o mouse por cima.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`NavigationHistory`**: `go` navega e atualiza `current`; `go` depois
  de `back` descarta o histórico "futuro"; `back`/`forward` nos limites
  (início/fim) devolvem `None` sem mover `current`; `can_go_back`/
  `can_go_forward` corretos em cada posição.
- **`AddressBar`**: `on_char` insere no cursor; `on_backspace` remove
  antes do cursor (e não panica com `text` vazio); `commit` com texto
  vazio devolve `None`; `commit` com texto não-vazio devolve
  `parse_source` do texto; `cancel` reverte o texto e desfoca;
  `set_error`/limpar erro.
- **`Chrome::hit_test`**: clique dentro de cada botão devolve a
  `ChromeAction` certa; clique no campo de texto devolve
  `FocusAddressBar`; clique fora da barra (Y maior que `bar_height`)
  devolve `None`; bordas exatas dos retângulos dos botões (dentro vs.
  fora, um pixel de cada lado).
- **`translate_frame`**: desloca `y` de todo `RectCommand`/`TextCommand`
  pelo `dy` dado; preserva `x`/`width`/`height`/`color`/`content`/`size`
  inalterados; frame vazio devolve frame vazio.
- **Verificação manual** (obrigatória, não substituível por teste
  automatizado): rodar `cargo run -p ferris-compositor`, digitar uma URL
  real na barra e confirmar que navega; clicar Voltar/Avançar/Recarregar
  e confirmar que funcionam; testar os 3 atalhos de teclado; forçar um
  erro de navegação (URL inválida) e confirmar que aparece na barra sem
  a janela fechar.
