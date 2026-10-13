# App — tradução pt-BR. Mesmas chaves de en-US.

notice-no-intentions = Nenhuma ação aqui
notice-no-suggestions = Nenhuma sugestão
notice-no-declaration = Nenhuma declaração encontrada
notice-nothing-to-search = Nada para buscar: posicione o cursor em um símbolo
notice-open-failed = Não foi possível abrir { $path }: { $error }
preview-binary = Arquivo binário ({ $size }), não exibido no editor
notice-read-failed = Não foi possível ler { $path }
notice-write-failed = Não foi possível gravar { $path }: { $error }
notice-save-failed = Não foi possível salvar { $file }: { $error }
notice-save-failed-many = Não foi possível salvar { $files }
notice-invalid-pattern = Padrão inválido: { $error }
notice-replaced = { $count ->
    [one] 1 ocorrência substituída
   *[other] { $count } ocorrências substituídas
}

notice-caret-class-or-method = Posicione o cursor em uma classe ou método
notice-caret-in-class = Posicione o cursor em uma classe ou dentro dela
notice-caret-on-name = Posicione o cursor em um nome para renomear
notice-no-implementations = Nenhuma implementação de { $name } encontrada
notice-no-parameter-info = Sem informação de parâmetros aqui
implementations-title = Implementações de { $name }
hierarchy-title = Hierarquia de tipos de { $name }
hierarchy-not-indexed = não indexado

notice-renamed = { $occurrences ->
    [one] 1 ocorrência renomeada
   *[other] { $occurrences } ocorrências renomeadas
} em { $files ->
    [one] 1 arquivo
   *[other] { $files } arquivos
}
notice-rename-exists = { $path } já existe; arquivo não renomeado
notice-rename-failed = Não foi possível renomear { $path }: { $error }

intention-implement-methods = Implementar métodos
intention-optimize-imports = Otimizar imports
notice-implemented = { $count ->
    [one] 1 método implementado
   *[other] { $count } métodos implementados
}
notice-imports-optimized = Os imports já estão otimizados

notice-formatted = Formatado com { $formatter }
notice-whitespace-trimmed = Sem formatador no projeto: espaços no fim das linhas removidos
notice-no-composer = Nenhum composer.json neste projeto

composer-title = Composer
composer-install = Instalar dependências do composer.lock
composer-update = Atualizar dependências para as últimas versões permitidas
composer-dump-autoload = Regenerar o autoloader
composer-outdated = Listar dependências diretas desatualizadas
composer-validate = Verificar o composer.json
composer-script = script
composer-steps = { $count ->
    [one] 1 passo
   *[other] { $count } passos
}

error-run-php = não foi possível executar o php: { $error }
error-no-test-runner = Nenhum executor de testes encontrado (instale pestphp/pest ou phpunit/phpunit)
error-formatter-failed = { $formatter } falhou: { $error }
error-phpstan-missing = O PHPStan não está instalado
error-phpstan-no-report = O PHPStan não retornou relatório
error-terminal-open = não foi possível abrir um terminal: { $error }
error-terminal-shell = não foi possível iniciar o shell: { $error }
error-indexing = falha na indexação: { $error }
error-rename = falha ao renomear: { $error }
error-invalid-pattern = padrão inválido

generate-title = Gerar
generate-constructor = Construtor
generate-getters = Getters
generate-setters = Setters
generate-getters-setters = Getters e setters
generate-override = Sobrescrever métodos…
generate-override-title = Sobrescrever método
notice-generate-outside-class = Posicione o cursor dentro de uma classe para gerar código
notice-nothing-to-generate = Nada para gerar aqui
templates-title = Inserir live template

error-git-start = não foi possível executar o git: { $error }

git-not-repository = Este projeto não é um repositório Git
git-detached = HEAD destacado
git-refresh = Atualizar
git-pull = Pull (fast-forward)
git-push = Push
git-discard = Descartar alterações
git-discard-confirm = Descartar?
git-clean = Sem alterações
git-stage-all = Adicionar tudo
git-unstage-all = Remover tudo
git-message-placeholder = Mensagem do commit
git-commit = Commit
git-commit-push = Commit e push
git-select-file = Selecione um arquivo para ver as alterações
git-staging = Adicionando…
git-committing = Fazendo commit…
git-pushing = Enviando…
git-pulling = Baixando…
git-discarding = Descartando…
git-switching = Trocando de branch…
git-failed = Git: { $error }
git-empty-message = Escreva uma mensagem de commit primeiro
git-nothing-staged = Adicione ao menos um arquivo para o commit
git-committed = Commit feito: { $summary }
git-pushed = Push concluído
git-pulled = Pull concluído
git-discarded = Alterações descartadas
git-switched = Branch trocada
git-branches-title = Branches
git-branch-current = atual
git-branch-remote = remota
git-create-branch = criar branch a partir do HEAD

debug-test = Depurar teste
debug-again = Depurar de novo
debug-hint = Clique nos números das linhas para criar breakpoints e depure um teste com ⌃⇧D
debug-starting = aguardando o Xdebug…
debug-running = executando
debug-paused = pausado
debug-ended = encerrado
debug-resume = Continuar
debug-step-over = Passar por cima
debug-step-into = Entrar
debug-step-out = Sair
debug-stop = Parar
debug-eval-placeholder = Avaliar expressão ($user->name)
debug-finished = Sessão de depuração encerrada
debug-failed = Depurador: { $error }
debug-no-xdebug = O Xdebug não está instalado neste PHP (instale-o ou defina PHPTHUNDER_XDEBUG com o caminho do xdebug.so)
debug-no-connection = O Xdebug não se conectou (o xdebug.mode permite e a porta está livre?)
debug-connection-closed = O Xdebug fechou a conexão
debug-no-value = sem valor

notice-changed-on-disk = Alterado no disco enquanto há edições não salvas: { $files }

file-new-file = Novo arquivo…
file-new-folder = Nova pasta…
file-new-class = Nova classe PHP…
file-rename = Renomear ou mover…
file-delete = Mover para a Lixeira
file-reveal = Mostrar no Finder
file-copy-path = Copiar caminho relativo
file-new-file-in = Novo arquivo em { $dir }
file-new-folder-in = Nova pasta em { $dir }
file-new-class-in = Nova classe PHP em { $dir }
file-rename-title = Renomear ou mover { $path }
file-prompt-hints = ↵ confirmar   esc cancelar
file-deleted = Movido para a Lixeira: { $path }
file-op-failed = Falha na operação de arquivo: { $error }
file-bad-name = Digite um nome dentro do projeto
file-exists = { $path } já existe

refactor-extract-variable = Extrair variável
refactor-extract-method = Extrair método
refactor-inline-variable = Incorporar variável
refactor-change-signature = Alterar assinatura
refactor-move-class = Mover classe para namespace
refactor-extract-variable-help = Nome da nova variável; ocorrências idênticas na instrução também são trocadas.
refactor-extract-method-help = Nome do novo método privado; variáveis definidas antes viram parâmetros.
refactor-change-signature-help = Edite a lista de parâmetros: reordene, mude tipos ou adicione parâmetros (as chamadas passam o padrão do novo parâmetro, senão null).
refactor-move-class-help = Novo namespace; o arquivo vai para a pasta PSR-4 e os imports em outros arquivos acompanham.
refactor-select-first = Selecione uma expressão ou instruções primeiro
refactor-no-callable = Posicione o cursor em uma função ou método

replace-in-path = Substituir tudo
replace-in-path-confirm = Substituir { $count } ocorrências? Clique de novo
replace-in-path-done = { $count ->
    [one] 1 ocorrência substituída
   *[other] { $count } ocorrências substituídas
} em { $files ->
    [one] 1 arquivo
   *[other] { $files } arquivos
}
replace-in-path-failed = Não foi possível gravar: { $files }

# Command names (Find Action palette, ⇧⌘A).
cmd-save = Salvar
cmd-close-tab = Fechar aba
cmd-toggle-sidebar = Mostrar ou ocultar árvore do projeto
cmd-next-tab = Próxima aba
cmd-prev-tab = Aba anterior
cmd-undo = Desfazer
cmd-redo = Refazer
cmd-toggle-comment = Comentar com comentário de linha
cmd-duplicate = Duplicar linha ou seleção
cmd-delete-line = Apagar linha
cmd-move-line-up = Mover linha para cima
cmd-move-line-down = Mover linha para baixo
cmd-complete = Completar código
cmd-goto-declaration = Ir para declaração
cmd-find-usages = Encontrar usos
cmd-goto-line = Ir para linha
cmd-goto-file = Ir para arquivo
cmd-goto-class = Ir para classe
cmd-goto-symbol = Ir para símbolo
cmd-recent-files = Arquivos recentes
cmd-search-everywhere = Buscar em tudo
cmd-back = Voltar
cmd-forward = Avançar
cmd-find = Localizar
cmd-replace = Substituir
cmd-find-next = Localizar próximo
cmd-find-previous = Localizar anterior
cmd-find-in-path = Localizar nos arquivos
cmd-quick-fix = Mostrar intenções e correções
cmd-parameter-info = Informação de parâmetros
cmd-rename = Renomear
cmd-optimize-imports = Otimizar imports
cmd-goto-implementation = Ir para implementação
cmd-type-hierarchy = Hierarquia de tipos
cmd-file-structure = Estrutura do arquivo
cmd-problems = Problemas
cmd-toggle-terminal = Terminal
cmd-toggle-claude = Claude
cmd-run-tests = Rodar testes
cmd-reformat = Reformatar código
cmd-composer = Composer
cmd-generate = Gerar
cmd-insert-template = Inserir live template
cmd-commit = Commit
cmd-push = Push
cmd-pull = Pull
cmd-add-caret-above = Adicionar cursor acima
cmd-add-caret-below = Adicionar cursor abaixo
cmd-select-next-occurrence = Selecionar próxima ocorrência
cmd-toggle-soft-wrap = Quebra de linha automática
cmd-extract-variable = Extrair variável
cmd-extract-method = Extrair método
cmd-inline-variable = Embutir variável
cmd-change-signature = Alterar assinatura
cmd-move-class = Mover classe
cmd-debug-test = Depurar teste
cmd-toggle-breakpoint = Alternar breakpoint
cmd-resume = Continuar programa
cmd-step-over = Passar por cima
cmd-step-into = Entrar
cmd-step-out = Sair
cmd-stop-debug = Parar depuração
cmd-extend-selection = Expandir seleção
cmd-shrink-selection = Reduzir seleção
cmd-join-lines = Juntar linhas
cmd-split-line = Quebrar linha
cmd-start-new-line = Nova linha abaixo
cmd-start-new-line-before = Nova linha acima
cmd-complete-statement = Completar instrução
cmd-toggle-case = Alternar maiúsculas/minúsculas
cmd-transpose = Trocar caracteres
cmd-block-comment = Comentar com comentário de bloco
cmd-match-brace = Ir para o par da chave
cmd-block-start = Ir para o início do bloco
cmd-block-end = Ir para o fim do bloco
cmd-word-completion = Completar palavra (cíclico)
cmd-unwrap = Desembrulhar/Remover
cmd-surround-with = Envolver com
cmd-sort-lines = Ordenar linhas
cmd-reverse-lines = Inverter linhas
cmd-unique-lines = Remover linhas duplicadas
cmd-select-all-occurrences = Selecionar todas as ocorrências
cmd-find-action = Localizar ação
cmd-next-error = Próximo problema
cmd-previous-error = Problema anterior
action-find-action = Localizar ação
action-surround-with = Envolver com
notice-no-problems = Nenhum problema neste arquivo
cmd-quick-doc = Documentação rápida
cmd-toggle-whitespace = Mostrar espaços em branco
notice-no-documentation = Sem documentação aqui
cmd-move-statement-up = Mover instrução para cima
cmd-move-statement-down = Mover instrução para baixo
cmd-move-element-left = Mover elemento para a esquerda
cmd-move-element-right = Mover elemento para a direita
cmd-delete-word-end = Apagar até o fim da palavra
cmd-delete-word-start = Apagar até o início da palavra
cmd-add-caret-per-line = Cursor em cada linha selecionada
cmd-toggle-bookmark = Alternar marcador
cmd-show-bookmarks = Mostrar marcadores
cmd-jump-to-last-edit = Último local editado
cmd-recent-locations = Locais recentes
cmd-paste-from-history = Colar do histórico
cmd-copy-reference = Copiar referência
cmd-goto-super-method = Ir para método pai
cmd-goto-type-declaration = Ir para declaração do tipo
cmd-goto-test = Ir para teste
action-recent-locations = Locais recentes
action-bookmarks = Marcadores
action-paste-from-history = Colar do histórico
action-go-to-test = Ir para teste
notice-no-bookmarks = Nenhum marcador ainda (F3 adiciona)
notice-clipboard-empty = Nada copiado ainda
clipboard-lines = { $count } linhas
notice-copied = Copiado { $text }
notice-no-super-method = Nenhum método pai
notice-no-type-declaration = Declaração do tipo não encontrada
notice-no-test = Nenhum teste ou classe encontrado para { $name }
notify-error = Erro
notify-tests = Testes concluídos
notify-tests-body = { $passed } passaram, { $failed } falharam
cmd-zoom-in = Aumentar zoom
cmd-zoom-out = Diminuir zoom
cmd-zoom-reset = Zoom normal
notice-zoom = Zoom { $percent }%
