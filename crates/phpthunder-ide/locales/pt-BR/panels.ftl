# Painéis — tradução pt-BR. Mesmas chaves de en-US.

panel-usages = Usos
panel-find = Busca
panel-problems = Problemas
panel-hierarchy = Hierarquia
panel-tests = Testes
panel-terminal = Terminal

problems-no-file = Nenhum arquivo aberto
problems-none = Nenhum problema em { $file }

usages-searching = Buscando usos de { $name }…
usages-declaration = declaração
usages-title = { $name } — { $count ->
    [one] 1 uso
   *[other] { $count } usos
} em { $files ->
    [one] 1 arquivo
   *[other] { $files } arquivos
}

find-results-searching = Buscando “{ $query }”…
find-results-title = “{ $query }” — { $matches ->
    [one] 1 ocorrência
   *[other] { $matches } ocorrências
} em { $files ->
    [one] 1 arquivo
   *[other] { $files } arquivos
}
find-results-truncated = (parou no limite)

tests-run-again = Rodar de novo
tests-run-file = Rodar arquivo
tests-run-all = Rodar todos
tests-none = Nenhum teste rodado ainda
tests-running = Rodando { $scope }…
tests-all = Todos os testes
tests-passed = { $count ->
    [one] 1 passou
   *[other] { $count } passaram
}
tests-failed = { $count ->
    [one] 1 falhou
   *[other] { $count } falharam
}
tests-skipped = { $count ->
    [one] 1 ignorado
   *[other] { $count } ignorados
}

terminal-none = Nenhum terminal rodando
terminal-start = Iniciar
terminal-exited = Processo encerrado
terminal-restart = Reiniciar

panel-commit = Commit
panel-debug = Debug
