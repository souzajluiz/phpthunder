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
diagnostic-empty-body = Corpo vazio: a instrução seguinte sempre executa
diagnostic-duplicate-case = Case { $value } duplicado nunca é alcançado
diagnostic-duplicate-condition = Condição repete um ramo anterior; este ramo nunca executa
diagnostic-identical-branches = Os dois ramos do ternário são iguais
diagnostic-same-operands = Os dois lados da operação são iguais
diagnostic-unused-closure-use = { $name } é importada no use mas nunca lida
diagnostic-duplicate-member = { $name } declarado duas vezes
diagnostic-abstract-in-concrete-class = Método abstrato em classe que não é abstrata
diagnostic-this-in-static = $this não existe em método estático
diagnostic-redundant-else = Else redundante: o ramo do if sempre sai
fix-remove-redundant-else = Remover else redundante

## Rule-based inspections (`Rule::slug`)
diagnostic-missing-break = Case continua no próximo; adicione break
diagnostic-duplicate-match-condition = Condição { $detail } repetida no match nunca é alcançada
diagnostic-optional-before-required = Parâmetro opcional { $detail } antes de um obrigatório
diagnostic-division-by-zero = Divisão por zero
diagnostic-foreach-overwrites-array = { $detail } é o array percorrido e também variável do laço
diagnostic-foreach-variable-conflict = { $detail } sobrescreve a variável do foreach externo
diagnostic-output-function-result-used = { $detail }() imprime e retorna null sem o segundo argumento true
fix-output-function-result-used = Passar true para retornar a saída
diagnostic-rand-arguments-reversed = Mínimo maior que o máximo
fix-rand-arguments-reversed = Trocar argumentos
diagnostic-expression-result-unused = Resultado da expressão não é usado
diagnostic-continue-in-switch = continue dentro de switch age como break; use continue 2
fix-continue-in-switch = Trocar por continue 2
diagnostic-exception-immediately-rethrown = Exceção capturada só para ser relançada
diagnostic-nested-ternary = Ternário aninhado sem parênteses é erro fatal desde o PHP 8
diagnostic-dollar-brace-interpolation = Interpolação ${ "{" }var{ "}" } está obsoleta; use { "{" }$var{ "}" }
fix-dollar-brace-interpolation = Usar { "{" }$var{ "}" }
diagnostic-implicitly-nullable-parameter = Parâmetro { $detail } implicitamente anulável está obsoleto; torne o tipo anulável
fix-implicitly-nullable-parameter = Tornar o tipo anulável
diagnostic-magic-method-not-public = Método mágico { $detail } precisa ser público
fix-magic-method-not-public = Tornar público
diagnostic-magic-method-static = Método mágico { $detail } com modificador static errado
diagnostic-inconsistent-return = Função retorna valor só em alguns caminhos
diagnostic-search-result-as-boolean = { $detail }() pode retornar 0, que vale false; compare com !== false
diagnostic-format-arguments-mismatch = O formato espera { $detail } argumentos
diagnostic-redundant-promoted-assignment = Propriedade promovida já é atribuída
fix-redundant-promoted-assignment = Remover atribuição
diagnostic-value-immediately-rewritten = { $detail } é sobrescrito antes de ser lido
fix-value-immediately-rewritten = Remover primeira atribuição
diagnostic-removed-cast = O cast { $detail } foi removido no PHP 8
diagnostic-ternary-to-short = Ternário pode ser encurtado para ?:
fix-ternary-to-short = Trocar por ?:
diagnostic-ternary-to-condition = Ternário pode ser trocado pela própria condição
fix-ternary-to-condition = Trocar pela condição
diagnostic-isset-to-coalesce = Ternário com isset() pode ser trocado por ??
fix-isset-to-coalesce = Trocar por ??
diagnostic-nullsafe-can-be-used = Checagem de null pode ser trocada por ?->
fix-nullsafe-can-be-used = Trocar por ?->
diagnostic-combine-isset = Chamadas isset() podem ser combinadas
fix-combine-isset = Combinar chamadas isset()
diagnostic-boolean-comparison = Comparação com booleano pode ser simplificada
fix-boolean-comparison = Simplificar
diagnostic-array-push-single = array_push() com um valor pode ser $array[] =
fix-array-push-single = Trocar por $array[] =
diagnostic-pow-to-operator = pow() pode ser trocado por **
fix-pow-to-operator = Trocar por **
diagnostic-long-array-syntax = array() pode ser trocado por []
fix-long-array-syntax = Trocar por []
diagnostic-redundant-closing-tag = Tag de fechamento redundante
fix-redundant-closing-tag = Remover tag de fechamento
diagnostic-closure-to-arrow-function = Closure pode ser arrow function
fix-closure-to-arrow-function = Converter em arrow function
diagnostic-get-class-to-class-constant = get_class() pode ser trocado por ::class
fix-get-class-to-class-constant = Trocar por ::class
diagnostic-str-function-can-be-used = Pode ser trocado por { $detail }()
fix-str-function-can-be-used = Trocar
diagnostic-unnecessary-local-variable = Variável local { $detail } é retornada logo em seguida
fix-unnecessary-local-variable = Embutir variável
diagnostic-silence-operator = Erro silenciado com @
diagnostic-unnecessary-bool-cast = Cast (bool) desnecessário
fix-unnecessary-bool-cast = Remover cast
diagnostic-redundant-break-argument = Argumento 1 redundante
fix-redundant-break-argument = Remover argumento
diagnostic-in-array-single-element = in_array() com um elemento pode ser uma comparação
fix-in-array-single-element = Trocar por comparação
diagnostic-array-append-using-count = $array[count($array)] pode ser $array[]
fix-array-append-using-count = Trocar por $array[]
diagnostic-nested-dirname = dirname() aninhados podem usar o argumento de níveis
fix-nested-dirname = Usar o argumento de níveis
diagnostic-nested-min-max = Chamadas { $detail }() aninhadas podem ser unidas
fix-nested-min-max = Unir chamadas
diagnostic-match-only-default = match só tem o braço default
diagnostic-unnecessary-semicolon = Ponto e vírgula desnecessário
fix-unnecessary-semicolon = Remover ponto e vírgula
diagnostic-unused-parameter = Parâmetro { $detail } não usado
diagnostic-else-if = else if deve ser escrito elseif
fix-else-if = Trocar por elseif
diagnostic-var-keyword = var deve ser trocado por public
fix-var-keyword = Trocar por public
diagnostic-missing-visibility = { $detail } sem visibilidade
fix-missing-visibility = Adicionar public
diagnostic-modifier-order = Modificadores fora de ordem
fix-modifier-order = Reordenar modificadores
diagnostic-new-without-parentheses = new sem parênteses
fix-new-without-parentheses = Adicionar parênteses
diagnostic-long-type-cast = Use o cast curto ({ $detail })
fix-long-type-cast = Usar cast curto
diagnostic-multiple-classes-in-file = Arquivo declara mais de uma classe
diagnostic-phpdoc-unknown-parameter = @param { $detail } não corresponde a nenhum parâmetro
diagnostic-assert-equals-literal = Pode ser { $detail }()
fix-assert-equals-literal = Trocar asserção
diagnostic-assert-count = Pode ser assertCount()
fix-assert-count = Trocar asserção
diagnostic-assert-array-has-key = Pode ser { $detail }()
fix-assert-array-has-key = Trocar asserção
diagnostic-assert-empty = Pode ser { $detail }()
fix-assert-empty = Trocar asserção
diagnostic-assert-contains = Pode ser { $detail }()
fix-assert-contains = Trocar asserção
diagnostic-assert-instance-of = Pode ser { $detail }()
fix-assert-instance-of = Trocar asserção
diagnostic-assert-arguments-order = O valor esperado deve vir primeiro
fix-assert-arguments-order = Trocar argumentos
diagnostic-missing-implementation = A classe precisa implementar { $detail }
fix-missing-implementation = Implementar métodos
diagnostic-extends-final-class = Não é possível estender a classe final { $detail }
diagnostic-overrides-final-method = Não é possível sobrescrever o método final { $detail }
diagnostic-wrong-supertype-kind = { $detail } não pode ser usado aqui
diagnostic-instantiate-abstract = Não é possível instanciar { $detail }
diagnostic-weaker-access = A visibilidade deve ser { $detail } ou mais aberta, como na classe pai
fix-weaker-access = Usar a visibilidade da classe pai
diagnostic-static-mismatch = Sobrescrita de { $detail } muda o static
diagnostic-signature-mismatch = Declaração deve ser compatível com { $detail }
diagnostic-too-few-arguments = Espera pelo menos { $detail } argumentos
diagnostic-too-many-arguments = Espera no máximo { $detail } argumentos
diagnostic-inaccessible-member = Membro { $detail } não é acessível aqui
diagnostic-static-call-to-instance-method = Método não estático { $detail } chamado estaticamente
diagnostic-undefined-class-constant = Constante de classe { $detail } não definida
diagnostic-void-result-used = { $detail } retorna void; o resultado é sempre null
diagnostic-catch-order = Já capturada pelo catch anterior de { $detail }
diagnostic-enum-match-not-exhaustive = match não cobre os cases do enum: { $detail }
diagnostic-missing-parent-constructor-call = Falta chamar o construtor da classe pai
diagnostic-readonly-modified = Propriedade readonly { $detail } não pode ser alterada aqui
diagnostic-undefined-data-provider = Data provider { $detail } não encontrado
