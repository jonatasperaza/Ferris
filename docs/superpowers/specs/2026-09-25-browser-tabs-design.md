# Ferris — Sub-projeto 2.10: Múltiplas Abas

Status: aprovado para implementação
Data: 2026-09-25

## Contexto

Sub-projetos 1 (compositor), 1.5 (hardening), 2.1-2.7 (motor HTML/CSS/
DOM/layout/paint completo, incluindo elementos inline), 2.8 (carregamento
de página real via `ferris-loader`), 2.9 (UI de navegação de uma aba só:
barra de endereço editável, histórico com Voltar/Avançar/Recarregar,
atalhos Ctrl+L/Alt+Setas/F5, tudo em `ferris-compositor/src/chrome.rs`)
estão completos e publicados em `github.com/jonatasperaza/Ferris`. Hoje
`App` (em `ferris-compositor/src/main.rs`) tem UM `Chrome` (uma
`NavigationHistory` + `AddressBar`) e UM `page_frame` cacheado — a janela
inteira representa uma única aba. Múltiplas abas foram deliberadamente
cortadas do escopo de 2.9 e adiadas pra esta peça, justamente pra
construir em cima de uma navegação de uma aba já sólida e revisada.

O objetivo declarado do usuário para o Ferris é se tornar um navegador
de verdade, usável no dia a dia, comparável ao Google Chrome. Abas são
uma peça central dessa experiência.

## Objetivo

Adicionar uma tira de abas fixa acima da barra de chrome já existente,
permitindo múltiplas páginas carregadas simultaneamente na mesma janela,
cada uma com sua própria barra de endereço, histórico de navegação
independente, e conteúdo cacheado — com controles pra abrir aba nova
(botão "+" ou Ctrl+T), fechar aba (botão "x" em cada aba ou Ctrl+W), e
trocar de aba (clique na aba ou Ctrl+Tab, com volta ao início).

**Critério de sucesso:** rodar `cargo run -p ferris-compositor` abre a
janela com uma aba (a página inicial, via argumento de CLI ou o fixture
padrão); abrir novas abas, navegar independentemente em cada uma,
fechar abas do meio/início/fim, e trocar entre elas preserva o estado de
cada uma corretamente (histórico, texto da barra, conteúdo visível);
fechar a última aba encerra a janela — validado por testes automatizados
das partes de lógica pura (ajuste de índice ao fechar aba, hit-testing da
tira de abas, histórico de aba em branco) e por verificação manual
rodando o binário de verdade.

## Arquitetura

`App` troca seus campos `chrome: Option<Chrome>` / `page_frame:
Option<scene::Frame>` (de 2.9) por `tabs: Vec<Tab>` + `active_tab:
usize`. Cada `Tab` contém seu próprio `Chrome` (barra de endereço +
histórico) e seu próprio `page_frame` cacheado — trocar de aba só muda
qual `Tab` é desenhado e recebe eventos de teclado/clique, sem recarregar
nada. Uma tira de abas nova, `ferris-compositor/src/tabs.rs` (mesmo
padrão de `chrome.rs`: lógica pura + desenho via `scene::DrawCommand`,
sem dependência de UI externa nova), desenha os títulos das abas acima
da barra de chrome já existente. O conteúdo da página é empurrado pra
baixo pela soma da altura da tira de abas + altura da barra de chrome
(mesmo mecanismo de `translate_frame` já usado em 2.9).

`Chrome` (de 2.9) ganha um construtor novo, `Chrome::new_blank()`, para
uma aba recém-aberta sem página carregada — seu campo `history` muda de
`NavigationHistory` para `Option<NavigationHistory>` (`None` = aba em
branco, sem histórico ainda; vira `Some` na primeira navegação
bem-sucedida daquela aba).

## Componentes

```rust
// ferris-compositor/src/chrome.rs (Chrome estendido)
pub struct Chrome {
    pub address_bar: AddressBar,
    pub history: Option<NavigationHistory>,
    pub bar_height: f32,
}

impl Chrome {
    pub fn new(initial: ferris_loader::Source, initial_text: String) -> Self; // já existe (2.9)
    pub fn new_blank() -> Self; // NOVO — aba em branco, barra focada, sem histórico
    // hit_test/frame: can_go_back()/can_go_forward() tratados como falso
    // quando history é None (botões desenhados desabilitados, sem disparar ação)
}

// ferris-compositor/src/tabs.rs (novo)
pub struct TabStrip {
    pub height: f32, // constante, ex: 32.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TabAction {
    Select(usize),
    Close(usize),
    New,
}

impl TabStrip {
    /// Converte coordenadas de clique (pixels lógicos) numa ação, ou
    /// `None` se o clique caiu fora de qualquer elemento da tira. Cada
    /// aba tem LARGURA FIXA (ex: 160px) e um "x" de fechar embutido no
    /// canto direito dela — clicar no "x" devolve `Close(i)`, clicar no
    /// resto do corpo da mesma aba devolve `Select(i)`. Como a largura é
    /// fixa (não depende da largura da janela), `hit_test` não precisa
    /// de `window_width`; abas além da borda direita da janela ficam
    /// simplesmente fora de qualquer área clicável (sem scroll/overflow
    /// tratado nesta peça — mesma classe de limitação já aceita pro
    /// texto longo da barra de endereço em 2.9).
    pub fn hit_test(&self, tab_count: usize, x: f32, y: f32) -> Option<TabAction>;
    /// Desenha uma aba por título (truncado pra caber na largura fixa),
    /// destacando a aba ativa, mais o botão "+" logo depois da última
    /// aba. `window_width` só é usada pro retângulo de fundo da tira
    /// inteira, não afeta a posição de nenhuma aba.
    pub fn frame(&self, titles: &[String], active: usize, window_width: f32) -> scene::Frame;
}

// ferris-compositor/src/main.rs
struct Tab {
    chrome: chrome::Chrome,
    page_frame: Option<scene::Frame>,
}

struct App {
    tabs: Vec<Tab>,
    active_tab: usize,
    // window, gpu, frame_timer, last_frame_start, occluded, modifiers,
    // cursor_position — inalterados de 2.9
}
```

Título de cada aba na tira: `chrome::source_display_text` (já existente,
de 2.9) do `Source` atual daquela aba, truncado pra caber na largura
disponível dividida pelo número de abas; uma aba em branco (`history:
None`) mostra o texto fixo "Nova aba". Sem extração de `<title>` HTML —
o motor não expõe isso em lugar nenhum ainda, fica fora de escopo desta
peça.

## Fluxo de dados

1. **Início**: `main()` computa a fonte inicial (argumento de CLI, ou o
   caminho padrão do fixture) exatamente como em 2.9, carrega a página,
   e cria a primeira aba com `Chrome::new(...)` — `tabs =
   vec![essa_aba]`, `active_tab = 0`. Falha aqui ainda mata o processo
   com `std::process::exit(1)` (comportamento preservado de 2.8/2.9).
2. **Aba nova** (clique no "+" da tira, ou Ctrl+T): `tabs.push(Tab {
   chrome: Chrome::new_blank(), page_frame: None })`, `active_tab =
   tabs.len() - 1` — sempre inserida no fim e sempre vira a ativa; a
   barra de endereço dessa aba já nasce focada.
3. **Fechar aba** (clique no "x" daquela aba na tira, ou Ctrl+W fecha a
   ativa): remove `tabs[i]`. Se `tabs` ficar vazio, `event_loop.exit()`
   (fechar a última aba fecha a janela). Senão, se `i` era a
   `active_tab` ou vinha antes dela, `active_tab` é ajustado pra
   continuar apontando pra uma aba válida (a aba que ficou na mesma
   posição visual, ou a última, se a fechada era a última).
4. **Trocar de aba** (clique numa aba da tira, ou Ctrl+Tab — próxima
   aba, voltando pra 0 depois da última): só muda `active_tab`; nada é
   recarregado, o `page_frame` já cacheado daquela aba é reaproveitado
   direto no próximo frame.
5. **Navegar numa aba em branco** (digitar uma URL/caminho e apertar
   Enter pela primeira vez): como `history` é `None`, a navegação
   bem-sucedida CRIA o histórico agora (`chrome.history =
   Some(NavigationHistory::new(source))`) em vez de chamar `.go()` num
   histórico que ainda não existe.
6. **Cada frame desenhado**: composição vira `TabStrip::frame(...)` +
   `chrome.frame(...)` da aba ativa + `translate_frame` do
   `page_frame` da aba ativa (deslocado pela soma das duas alturas) —
   as outras abas não são desenhadas nem recebem eventos de
   teclado/clique, só existem em memória.

## Tratamento de erro

Mesmo modelo de 2.9, agora por aba: uma falha de navegação interativa
(URL/caminho digitado errado, Voltar/Avançar/Recarregar numa página que
sumiu) nunca derruba o processo — o erro aparece só na barra daquela
aba específica; as outras abas continuam com seu próprio estado
intocado. Falha da página inicial (a primeira aba, via CLI) ainda mata
o processo, igual já documentado em 2.8/2.9.

## Limitações conhecidas (documentadas, não corrigidas nesta peça)

- **Sem extração de `<title>` HTML** — o texto da aba é sempre a
  URL/caminho da página (via `source_display_text`), nunca o título
  real do documento.
- **Sem reordenar abas por arrastar** — a ordem é sempre a de criação;
  fechar não reordena as restantes.
- **Sem sessão persistida** — fechar a janela perde todas as abas,
  igual histórico já não persiste desde 2.9.
- **Sem limite de abas** — nenhum cap artificial nesta peça (YAGNI).
- **Sem clique-do-meio pra fechar aba, sem arrastar aba pra fora da
  janela** — fora de escopo.

## Testes

TDD por componente, mesmo padrão das peças anteriores:
- **`Chrome::new_blank`**: `history` é `None`; `can_go_back`/
  `can_go_forward` (agora avaliados com `history` `None`) devolvem
  falso sem panicar; barra nasce focada.
- **Histórico criado na primeira navegação de uma aba em branco**: uma
  navegação bem-sucedida a partir de `history: None` resulta em
  `history: Some(...)` com a entrada certa; uma navegação que FALHA a
  partir de `history: None` mantém `history: None` (não cria histórico
  vazio nem quebra).
- **`TabStrip::hit_test`**: clique em cada aba existente devolve
  `Select(i)` certo; clique no "x" de cada aba devolve `Close(i)`
  certo; clique no botão "+" devolve `New`; clique fora de tudo devolve
  `None`; casos de borda em pixel exato entre abas adjacentes.
- **Lógica de fechar aba** (em `main.rs`, testável sem GPU): fechar a
  aba ativa do meio ajusta `active_tab` pra uma aba válida adjacente;
  fechar a primeira aba quando ela é a ativa ajusta certo; fechar a
  última aba quando ela é a ativa ajusta certo; fechar a única aba
  aberta é detectável (sinaliza fechar a janela) sem panicar com índice
  fora dos limites.
- **Verificação manual** (obrigatória, não substituível por teste
  automatizado): rodar `cargo run -p ferris-compositor`, abrir 3+ abas
  (clique no "+" e Ctrl+T), navegar independentemente em cada uma pra
  páginas diferentes, confirmar que Voltar/Avançar numa aba não afeta
  as outras; fechar uma aba do meio e confirmar que a aba certa fica
  ativa depois; usar Ctrl+Tab pra circular entre as abas; fechar todas
  as abas uma a uma e confirmar que a última fecha a janela.
