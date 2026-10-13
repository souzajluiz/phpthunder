# App — tradução pt-BR. Mesmas chaves de en-US.

notice-no-suggestions = Nenhuma sugestão
notice-no-declaration = Nenhuma declaração encontrada
notice-nothing-to-search = Nada para buscar: posicione o cursor em um símbolo
notice-open-failed = Não foi possível abrir { $path }: { $error }
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
