# Workbench — tradução pt-BR. Mesmas chaves de en-US.

welcome-hint = Abra um projeto PHP ou Laravel para começar. Indexação, autocomplete e inspeções rodam no próprio IDE, sem configuração.
welcome-open-folder = Abrir pasta…
welcome-recent = PROJETOS RECENTES
welcome-no-recent = Os projetos que você abrir aparecem aqui.
shortcuts-title = Abra um arquivo pela árvore do projeto
sidebar-project = PROJETO

rail-project = Projeto
rail-run-tests = Rodar testes

action-search-everywhere = Buscar em tudo
action-go-to-file = Ir para arquivo
action-go-to-class = Ir para classe
action-go-to-symbol = Ir para símbolo
action-go-to-line = Ir para linha
action-recent-files = Arquivos recentes
action-find-in-path = Buscar no projeto
action-file-structure = Estrutura do arquivo
action-choose = Escolher

palette-placeholder = Digite um nome
palette-placeholder-line = linha[:coluna]
palette-placeholder-recent = Filtrar arquivos recentes
palette-nothing-found = Nada encontrado
palette-hints = ↑↓ navegar   ↵ abrir   esc fechar
palette-line-item = Linha { $line }, coluna { $column }
palette-symbol-detail = { $owner } · linha { $line }

list-position = { $current } de { $total }
completion-keys = ↵ inserir · ⇥ substituir
status-claude-running = Claude rodando em { $project }
status-claude-exited = Claude encerrado

find-placeholder = Buscar
find-match-case = Diferenciar maiúsculas
find-words = Palavras inteiras
find-regex = Regex
find-bad-pattern = Padrão inválido
find-no-results = Sem resultados
find-count = { $count ->
    [one] 1 resultado
   *[other] { $count } resultados
}
replace-placeholder = Substituir
replace-one = Substituir
replace-all = Substituir tudo

search-title = Buscar no projeto
search-hints = ↵ buscar   esc fechar
search-placeholder = Texto a buscar
search-mask-placeholder = Máscara de arquivo, ex.: *.php
search-submit = Buscar

rename-placeholder = Novo nome
rename-title = Renomear { $name }
rename-hints = ↵ renomear   esc cancelar
rename-help = Atualiza todos os usos no projeto; o arquivo de uma classe com o mesmo nome também é renomeado.

status-indexing = Indexando…
status-indexed = { $files ->
    [one] 1 arquivo indexado
   *[other] { $files } arquivos indexados
} · { $elapsed }
status-inspecting = Inspecionando…
status-position = Ln { $line }, Col { $column }
gutter-route-action = Ir para { $target }
gutter-model-table = Ir para a migration de { $table }
gutter-run = Executar: { $command }
panel-move-right = Mover para a direita
panel-move-bottom = Mover para baixo

# Beside dependencies in composer.json and package.json, when the lock file lacks one.
package-not-installed = não instalado

# Markdown tab view toggle (tooltips).
markdown-editor = Só o editor
markdown-split = Editor e pré-visualização
markdown-preview = Só a pré-visualização

# Claude plan limits in the status bar while the Claude panel runs.
claude-limit-session = 5h
claude-limit-week = Semana
claude-limit-used = { $window }: { $percent }% usado
claude-limit-resets = {" "}· reinicia em { $left }
