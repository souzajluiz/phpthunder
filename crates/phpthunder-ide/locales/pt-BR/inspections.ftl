# Inspeções — tradução pt-BR. Mesmas chaves de en-US.

diagnostic-undefined-class = Classe '{ $name }' não definida
diagnostic-undefined-function = Função '{ $name }' não definida
diagnostic-undefined-method = Método '{ $name }' não encontrado em { $class }
diagnostic-undefined-property = Propriedade '${ $name }' não encontrada em { $class }
diagnostic-unused-import = Import '{ $name }' não utilizado
diagnostic-undefined-variable = Variável '{ $name }' não definida

fix-import-class = Importar classe { $name }
fix-remove-import = Remover import não utilizado
fix-add-method = Adicionar método '{ $name }()' em { $class }
fix-add-property = Adicionar propriedade '${ $name }' em { $class }

diagnostic-unused-variable = Variável '{ $name }' recebe valor mas nunca é usada
diagnostic-missing-return = Falta return: '{ $ty }' esperado
diagnostic-return-mismatch = Tipo de retorno incompatível: '{ $expected }' esperado, { $found } retornado
fix-remove-assignment = Remover variável não usada (manter o valor)
diagnostic-unknown-laravel-key = { $kind ->
    [Route] Rota desconhecida
    [View] View desconhecida
    [Config] Chave de config desconhecida
    [Translation] Chave de tradução desconhecida
    [Guard] Guard desconhecido
    [Disk] Disco desconhecido
    [CacheStore] Store de cache desconhecido
    [QueueConnection] Conexão de fila desconhecida
    [LogChannel] Canal de log desconhecido
    [DbConnection] Conexão de banco desconhecida
    [Mailer] Mailer desconhecido
    [InertiaPage] Página Inertia desconhecida
    [Livewire] Componente Livewire desconhecido
   *[other] Chave desconhecida
} '{ $name }'
diagnostic-simplify-order-by = orderBy(…, 'desc') pode ser orderByDesc(…)
fix-use-order-by-desc = Usar orderByDesc()
fix-blade-echo-raw = Saída sem escape ({"{"}!! !!{"}"})
fix-blade-echo-escaped = Escapar saída ({"{"}{"{"} {"}"}{"}"})
fix-rules-to-array = Converter regras para array
fix-rules-to-string = Converter regras para string

# Syntax-only inspections.
diagnostic-unreachable-code = Instrução inalcançável
diagnostic-duplicate-array-key = Chave de array '{ $key }' duplicada; vale o último valor
diagnostic-assignment-in-condition = Atribuição em condição; queria usar ==?
diagnostic-loose-null-comparison = Comparação frouxa com null também aceita 0, '' e []
diagnostic-empty-catch = Bloco catch vazio esconde erros
diagnostic-self-assignment = Valor atribuído a si mesmo
diagnostic-unused-private-member = Membro privado { $name } não usado
fix-use-strict-comparison = Usar comparação estrita
fix-remove-statement = Remover instrução
